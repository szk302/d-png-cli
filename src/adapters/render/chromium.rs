//! Headless Chromium controlled directly through the Chrome DevTools Protocol.
use super::{
    chromium_assets::{AssetServer, AssetStore},
    process::{ChildGuard, TIMEOUT, executable, log_excerpt},
    resample,
};
use crate::{MAX_BYTES, domain::drawio_png as png_data};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    env,
    ffi::OsString,
    fs,
    io::ErrorKind,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};

pub fn discover() -> Result<PathBuf> {
    for key in ["DRAWIO_PNG_CHROME_PATH", "CHROME_PATH"] {
        if let Some(path) = env::var_os(key) {
            let path = PathBuf::from(path);
            ensure!(
                executable(&path),
                "{key} is not an executable file: {}",
                path.display()
            );
            return path
                .canonicalize()
                .with_context(|| format!("cannot resolve {key}"));
        }
    }
    // Chrome is the default; Chromium is used only when no Chrome is installed.
    let groups: [&[&str]; 2] = if cfg!(windows) {
        [&["chrome.exe"], &["chromium.exe"]]
    } else {
        [
            &["google-chrome", "google-chrome-stable", "chrome"],
            &["chromium", "chromium-browser"],
        ]
    };
    if let Some(path) = env::var_os("PATH") {
        let directories: Vec<_> = env::split_paths(&path).collect();
        for names in groups {
            for directory in &directories {
                for name in names {
                    let candidate = directory.join(name);
                    if executable(&candidate) {
                        return Ok(candidate.canonicalize()?);
                    }
                }
            }
        }
    }
    let paths = standard_paths(env::consts::OS, |key| env::var_os(key));
    paths
        .into_iter()
        .find(|p| executable(p))
        .context("Chromium/Chrome not found; set DRAWIO_PNG_CHROME_PATH or use --no-render")
}

/// Standard install locations, with every Chrome candidate before any Chromium.
fn standard_paths(os: &str, var: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let (mut chrome, mut chromium) = (Vec::new(), Vec::new());
    match os {
        "macos" => {
            for base in [
                Some(PathBuf::from("/Applications")),
                var("HOME").map(|h| PathBuf::from(h).join("Applications")),
            ]
            .into_iter()
            .flatten()
            {
                chrome.push(base.join("Google Chrome.app/Contents/MacOS/Google Chrome"));
                chromium.push(base.join("Chromium.app/Contents/MacOS/Chromium"));
            }
        }
        "windows" => {
            for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
                if let Some(base) = var(key) {
                    let base = PathBuf::from(base);
                    chrome.push(base.join("Google/Chrome/Application/chrome.exe"));
                    chromium.push(base.join("Chromium/Application/chrome.exe"));
                }
            }
        }
        _ => {
            chrome.extend([
                PathBuf::from("/usr/bin/google-chrome"),
                PathBuf::from("/opt/google/chrome/chrome"),
            ]);
            chromium.push(PathBuf::from("/usr/bin/chromium"));
        }
    }
    chrome.append(&mut chromium);
    chrome
}

/// Chrome's own services (Google Chrome in particular) connect to Google even
/// with --disable-background-networking: component updates, optimization guide
/// models, push messaging, accounts. Fetch interception only sees the page's
/// requests, so stop these at name resolution. Without --allow-network nothing
/// but loopback resolves; with it, only the known service hosts are blocked so
/// diagram images and fonts from any other host still load.
///
/// An HTTP proxy resolves names itself, so Chrome must not use the proxy that it
/// would inherit from the environment or system settings. Without
/// --allow-network every request is local and the proxy is disabled. With it,
/// the user's proxy is kept for diagram assets, and requests through it to the
/// service hosts are not blocked.
const CHROME_SERVICE_HOSTS: &[&str] = &[
    "accounts.google.com",
    "android.clients.google.com",
    "clients2.google.com",
    "clientservices.googleapis.com",
    "content-autofill.googleapis.com",
    "edgedl.me.gvt1.com",
    "mtalk.google.com",
    "optimizationguide-pa.googleapis.com",
    "safebrowsing.googleapis.com",
    "update.googleapis.com",
];

fn network_args(allow_network: bool) -> Vec<String> {
    let mut args = vec![host_resolver_rules(allow_network)];
    if !allow_network {
        args.push("--no-proxy-server".into());
    }
    args
}

fn host_resolver_rules(allow_network: bool) -> String {
    let rules = if allow_network {
        CHROME_SERVICE_HOSTS
            .iter()
            .map(|host| format!("MAP {host} ~NOTFOUND"))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        // IP literals also match MAP *, so keep loopback for drawio-png-cli's own origins.
        "MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE 127.0.0.1".into()
    };
    format!("--host-resolver-rules={rules}")
}

fn extra_args() -> Result<Vec<String>> {
    let Some(value) = env::var_os("DRAWIO_PNG_CHROME_ARGS") else {
        return Ok(Vec::new());
    };
    let value = value
        .to_str()
        .context("DRAWIO_PNG_CHROME_ARGS must be valid Unicode")?;
    let args = shell_words::split(value)
        .context("invalid DRAWIO_PNG_CHROME_ARGS: check quotes and escapes")?;
    for arg in &args {
        // These options bypass process ownership, the local transport or network policy.
        let key = arg.split('=').next().unwrap_or(arg);
        ensure!(
            arg.starts_with("--")
                && arg != "--"
                && !matches!(
                    key,
                    "--remote-debugging-port"
                        | "--remote-debugging-pipe"
                        | "--remote-debugging-address"
                        | "--user-data-dir"
                        | "--headless"
                        | "--disable-web-security"
                        | "--disable-site-isolation-trials"
                        | "--disable-features"
                        | "--load-extension"
                        | "--disable-extensions-except"
                        | "--proxy-server"
                        | "--proxy-pac-url"
                        | "--host-resolver-rules"
                        | "--no-startup-window"
                ),
            "unsupported DRAWIO_PNG_CHROME_ARGS option: {arg}; drawio-png-cli manages the profile, transport and network policy"
        );
    }
    Ok(args)
}

/// Output policy within the Chromium backend; this never launches Desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ChromiumMode {
    /// Native 1x screenshot, without pixel resampling
    Raw,
    /// 2x capture and Hamming1 downsampling matching the Desktop reference
    Desktop,
    /// SVG-to-Canvas export matching the VS Code extension
    #[default]
    Vscode,
}

impl ChromiumMode {
    fn name(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Desktop => "desktop",
            Self::Vscode => "vscode",
        }
    }
}

pub fn render(xml: &str, allow_network: bool) -> Result<Vec<u8>> {
    render_configured(xml, allow_network, None)
}

pub(crate) fn render_configured(
    xml: &str,
    allow_network: bool,
    explicit_mode: Option<ChromiumMode>,
) -> Result<Vec<u8>> {
    let mode = if let Some(mode) = explicit_mode {
        mode
    } else if let Some(value) = env::var_os("DRAWIO_PNG_CHROMIUM_MODE") {
        let value = value
            .to_str()
            .context("DRAWIO_PNG_CHROMIUM_MODE must be valid Unicode")?;
        <ChromiumMode as clap::ValueEnum>::from_str(value, false).map_err(|_| {
            anyhow::anyhow!("invalid DRAWIO_PNG_CHROMIUM_MODE: expected raw, desktop, or vscode")
        })?
    } else {
        ChromiumMode::default()
    };
    render_mode(xml, allow_network, mode)
}

pub fn render_mode(xml: &str, allow_network: bool, mode: ChromiumMode) -> Result<Vec<u8>> {
    let args = extra_args()?;
    let program = discover()?;
    render_with_mode(
        &program,
        xml,
        TIMEOUT,
        &args,
        env::var_os("DRAWIO_PNG_DRAWIO_WEB_PATH").map(PathBuf::from),
        allow_network,
        mode,
    )
}

/// Explicit inputs allow isolated browser integration tests without global environment edits.
pub fn render_with(
    program: &Path,
    xml: &str,
    timeout: Duration,
    args: &[String],
    web_root: Option<PathBuf>,
    allow_network: bool,
) -> Result<Vec<u8>> {
    render_with_mode(
        program,
        xml,
        timeout,
        args,
        web_root,
        allow_network,
        ChromiumMode::default(),
    )
}

pub fn render_with_mode(
    program: &Path,
    xml: &str,
    timeout: Duration,
    args: &[String],
    web_root: Option<PathBuf>,
    allow_network: bool,
    mode: ChromiumMode,
) -> Result<Vec<u8>> {
    // Declared first so it is dropped after the browser and its profile.
    let _interrupt = super::interrupt::Guard::new();
    ensure!(xml.len() <= MAX_BYTES, "XML exceeds 64 MiB limit");
    let deadline = Instant::now() + timeout;
    let profile = tempfile::Builder::new()
        .prefix("drawio-png-cli-chromium-")
        .tempdir()?;
    let server = AssetServer::start(web_root, allow_network, mode == ChromiumMode::Vscode)?;
    let mut log = tempfile::tempfile()?;
    let mut command = Command::new(program);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .args(args)
        .args([
            "--headless",
            "--remote-debugging-address=127.0.0.1",
            "--remote-debugging-port=0",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--disable-extensions",
            "--no-first-run",
            "--no-default-browser-check",
        ])
        .arg(format!("--user-data-dir={}", profile.path().display()))
        .args(network_args(allow_network))
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log.try_clone()?))
        .spawn()
        .with_context(|| format!("cannot start Chromium: {}", program.display()))?;
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    #[cfg(windows)]
    let _job = WindowsJob::assign(&child.child)?;
    let result = (|| -> Result<Vec<u8>> {
        let (port, endpoint) = loop {
            remaining(deadline)?;
            if let Some(status) = child.child.try_wait()? {
                bail!("Chromium exited before startup ({status})");
            }
            ensure!(
                log.metadata()?.len() <= MAX_BYTES as u64,
                "Chromium log exceeds 64 MiB limit"
            );
            if let Ok(value) = fs::read_to_string(profile.path().join("DevToolsActivePort")) {
                let mut lines = value.lines();
                if let (Some(port), Some(endpoint)) = (lines.next(), lines.next()) {
                    let port: u16 = port.parse().context("invalid Chromium debugging port")?;
                    ensure!(
                        endpoint.starts_with("/devtools/browser/"),
                        "invalid Chromium debugging endpoint"
                    );
                    break (port, endpoint.to_owned());
                }
            }
            thread::sleep(Duration::from_millis(20));
        };
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&address, remaining(deadline)?)?;
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_BYTES * 2))
            .max_frame_size(Some(MAX_BYTES * 2));
        let (socket, _) = tungstenite::client::client_with_config(
            format!("ws://127.0.0.1:{port}{endpoint}"),
            stream,
            Some(config),
        )
        .map_err(|e| anyhow::anyhow!("Chromium CDP handshake failed: {e}"))?;
        let mut cdp = Cdp {
            socket,
            next_id: 0,
            session: None,
            deadline,
            assets: server.store.clone(),
            allow_network,
            transferred: 0,
            log: log.try_clone()?,
        };
        let target = cdp.call("Target.createTarget", json!({"url":"about:blank"}))?;
        let attached = cdp.call(
            "Target.attachToTarget",
            json!({"targetId": target["targetId"], "flatten":true}),
        )?;
        cdp.session = Some(
            attached["sessionId"]
                .as_str()
                .context("missing CDP session")?
                .into(),
        );
        cdp.call("Page.enable", json!({}))?;
        cdp.call("Network.enable", json!({}))?;
        cdp.call("Network.setBypassServiceWorker", json!({"bypass":true}))?;
        cdp.call("Fetch.enable", json!({"patterns":[{"urlPattern":"*"}]}))?;
        cdp.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"source":include_str!("../../../assets/chromium-init.js")}),
        )?;
        cdp.call(
            "Emulation.setDefaultBackgroundColorOverride",
            json!({"color":{"r":0,"g":0,"b":0,"a":0}}),
        )?;
        cdp.call(
            "Emulation.setDeviceMetricsOverride",
            json!({"width":800,"height":600,"deviceScaleFactor":if mode == ChromiumMode::Desktop {resample::SCALE} else {1},"mobile":false}),
        )?;
        let navigation = cdp.call(
            "Page.navigate",
            json!({"url": format!("{}/export3.html", cdp.assets.origin)}),
        )?;
        ensure!(
            navigation.get("errorText").is_none(),
            "cannot load draw.io assets: {navigation}"
        );
        loop {
            if cdp
                .eval(if mode == ChromiumMode::Vscode {
                    "document.readyState === 'complete' && typeof Editor === 'function' && typeof Editor.prototype.exportToCanvas === 'function'"
                } else {
                    "document.readyState === 'complete' && typeof render === 'function'"
                })?
                .as_bool()
                == Some(true)
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let window = cdp.call("Runtime.evaluate", json!({"expression":"window"}))?;
        let rendered = cdp.call("Runtime.callFunctionOn", json!({
            "objectId":window["result"]["objectId"],
            "functionDeclaration":format!("function(xml, bundled, mode, preload) {{ {} return drawioPngRender(xml, bundled, mode, preload); }}", include_str!("../../../assets/chromium-render.js")),
            "arguments":[{"value":xml},{"value":cdp.assets.bundled},{"value":mode.name()},{"value":cdp.assets.preload}],
            "returnByValue":true, "awaitPromise":true
        }))?;
        ensure!(
            rendered.get("exceptionDetails").is_none(),
            "draw.io JavaScript error: {}",
            rendered["exceptionDetails"]
        );
        if mode != ChromiumMode::Vscode {
            return capture(&mut cdp, mode, deadline);
        }
        let data = rendered["result"]["value"]
            .as_str()
            .context("Chromium returned no PNG")?;
        ensure!(
            data.len() <= MAX_BYTES.div_ceil(3) * 4,
            "encoded PNG exceeds size limit"
        );
        let png = STANDARD
            .decode(data)
            .context("invalid Chromium PNG encoding")?;
        cdp.eval("true")?;
        png_data::validate(&png).context("Chromium produced an invalid PNG")?;
        remaining(deadline)?;
        Ok(png)
    })();
    result.with_context(|| format!("Chromium rendering failed: {}", log_excerpt(&mut log)))
}

fn capture(cdp: &mut Cdp, mode: ChromiumMode, deadline: Instant) -> Result<Vec<u8>> {
    let bounds = loop {
        let result = cdp.eval("document.getElementById('LoadingComplete') ? JSON.parse(document.getElementById('LoadingComplete').getAttribute('bounds')) : null")?;
        if !result.is_null() {
            break result;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let dimension = |size: &str, offset: &str| -> Result<u32> {
        let value = (bounds[size].as_f64().context("invalid render bounds")?
            + bounds[offset].as_f64().context("invalid render bounds")?)
        .ceil()
            + if mode == ChromiumMode::Desktop {
                1.0
            } else {
                0.0
            };
        ensure!(
            value.is_finite() && value >= 1.0 && value <= (MAX_BYTES / 4) as f64,
            "invalid or oversized render bounds"
        );
        Ok(value as u32)
    };
    let width = dimension("width", "x")?;
    let height = dimension("height", "y")?;
    let dpr = if mode == ChromiumMode::Desktop {
        resample::capture_size(width, height)?;
        resample::SCALE
    } else {
        ensure!(
            u64::from(width) * u64::from(height) * 4 <= MAX_BYTES as u64,
            "raw capture exceeds 64 MiB limit"
        );
        1
    };
    cdp.call(
        "Emulation.setDeviceMetricsOverride",
        json!({"width":width,"height":height,"deviceScaleFactor":dpr,"mobile":false}),
    )?;
    let ready = cdp.call("Runtime.evaluate", json!({"expression":"document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))", "awaitPromise":true}))?;
    ensure!(
        ready.get("exceptionDetails").is_none(),
        "cannot wait for capture fonts"
    );
    cdp.eval("true")?;
    let screenshot = cdp.call("Page.captureScreenshot", json!({"format":"png","captureBeyondViewport":true,"clip":{"x":0,"y":0,"width":width,"height":height,"scale":1}}))?;
    let data = screenshot["data"]
        .as_str()
        .context("Chromium returned no PNG")?;
    ensure!(
        data.len() <= MAX_BYTES.div_ceil(3) * 4,
        "encoded PNG exceeds size limit"
    );
    let png = STANDARD
        .decode(data)
        .context("invalid Chromium PNG encoding")?;
    cdp.eval("true")?;
    if mode == ChromiumMode::Desktop {
        resample::half_png(&png, width, height, deadline).context("cannot resize Chromium PNG")
    } else {
        png_data::validate(&png).context("Chromium produced an invalid PNG")?;
        remaining(deadline)?;
        Ok(png)
    }
}

fn remaining(deadline: Instant) -> Result<Duration> {
    super::interrupt::check()?;
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .context("Chromium rendering timed out")
}

struct Cdp {
    socket: WebSocket<TcpStream>,
    next_id: u64,
    session: Option<String>,
    deadline: Instant,
    assets: std::sync::Arc<AssetStore>,
    allow_network: bool,
    transferred: u64,
    log: fs::File,
}
impl Cdp {
    fn send(&mut self, method: &str, params: Value) -> Result<u64> {
        let timeout = remaining(self.deadline)?;
        self.socket.get_mut().set_read_timeout(Some(timeout))?;
        self.socket.get_mut().set_write_timeout(Some(timeout))?;
        self.next_id += 1;
        let mut message = json!({"id":self.next_id,"method":method,"params":params});
        if let Some(session) = &self.session {
            message["sessionId"] = json!(session);
        }
        self.socket.send(Message::text(message.to_string()))?;
        Ok(self.next_id)
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.send(method, params)?;
        loop {
            ensure!(
                self.log.metadata()?.len() <= MAX_BYTES as u64,
                "Chromium log exceeds 64 MiB limit"
            );
            self.socket.get_mut().set_read_timeout(Some(
                remaining(self.deadline)?.min(Duration::from_millis(100)),
            ))?;
            let message = match self.socket.read() {
                Err(tungstenite::Error::Io(e))
                    // Signals interrupt reads with a timeout even with SA_RESTART.
                    if matches!(
                        e.kind(),
                        ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                    ) =>
                {
                    remaining(self.deadline)
                        .with_context(|| format!("Chromium timed out during {method}"))?;
                    continue;
                }
                other => other.context("Chromium CDP connection closed")?,
            };
            let Message::Text(message) = message else {
                continue;
            };
            let value: Value = serde_json::from_str(&message).context("invalid CDP response")?;
            if value["id"].as_u64() == Some(id) {
                ensure!(
                    value.get("error").is_none(),
                    "Chromium {method} failed: {}",
                    value["error"]
                );
                return Ok(value["result"].clone());
            }
            self.event(&value)?;
        }
    }
    fn event(&mut self, event: &Value) -> Result<()> {
        let params = &event["params"];
        match event["method"].as_str().unwrap_or("") {
            "Fetch.requestPaused" => {
                let url = params["request"]["url"].as_str().unwrap_or("");
                let method = params["request"]["method"].as_str().unwrap_or("GET");
                // The document itself loads over loopback HTTP; see AssetServer.
                let document = params["resourceType"].as_str() == Some("Document");
                if let Some(asset) = (!document)
                    .then(|| self.assets.respond(method, url))
                    .flatten()
                {
                    let headers: Vec<Value> = asset
                        .headers
                        .iter()
                        .map(|(name, value)| json!({"name":name,"value":value}))
                        .collect();
                    self.send(
                        "Fetch.fulfillRequest",
                        json!({"requestId":params["requestId"],"responseCode":asset.status,"responseHeaders":headers,"body":STANDARD.encode(&asset.body)}),
                    )?;
                    return Ok(());
                }
                let local = url.starts_with(&format!("{}/", self.assets.origin));
                let allowed = local
                    || url.starts_with("data:")
                    || url.starts_with("blob:")
                    || (self.allow_network
                        && (url.starts_with("https://") || url.starts_with("http://")));
                if allowed {
                    self.send(
                        "Fetch.continueRequest",
                        json!({"requestId":params["requestId"]}),
                    )?;
                } else {
                    self.send(
                        "Fetch.failRequest",
                        json!({"requestId":params["requestId"],"errorReason":"BlockedByClient"}),
                    )?;
                    bail!(
                        "external resource blocked; use --allow-network to permit HTTP(S) images/fonts: {}",
                        short(url)
                    );
                }
            }
            "Network.responseReceived" => {
                let response = &params["response"];
                let url = response["url"].as_str().unwrap_or("");
                ensure!(
                    response["status"].as_f64().unwrap_or(200.0) < 400.0
                        || url.ends_with("/favicon.ico"),
                    "draw.io resource failed ({}): {}; check DRAWIO_PNG_DRAWIO_WEB_PATH or --allow-network",
                    response["status"],
                    short(url)
                );
            }
            // A dialog blocks the page's JavaScript until the deadline; report it instead.
            "Page.javascriptDialogOpening" => {
                bail!(
                    "draw.io opened a {} dialog: {}",
                    params["type"].as_str().unwrap_or("JavaScript"),
                    short(params["message"].as_str().unwrap_or(""))
                );
            }
            "Network.loadingFailed" => {
                bail!(
                    "draw.io resource failed: {}; check assets and --allow-network",
                    params["errorText"]
                );
            }
            "Network.dataReceived" => {
                self.transferred = self
                    .transferred
                    .saturating_add(params["dataLength"].as_u64().unwrap_or(0));
                ensure!(
                    self.transferred <= MAX_BYTES as u64,
                    "browser resource data exceeds 64 MiB limit"
                );
            }
            _ => {}
        }
        Ok(())
    }
    fn eval(&mut self, expression: &str) -> Result<Value> {
        let result = self.call("Runtime.evaluate", json!({"expression":format!("(() => {{ if (window.drawioPngErrors?.length) throw Error(window.drawioPngErrors.join('; ')); return ({expression}); }})()"),"returnByValue":true}))?;
        ensure!(
            result.get("exceptionDetails").is_none(),
            "draw.io JavaScript error: {}",
            result["exceptionDetails"]
        );
        Ok(result["result"]["value"].clone())
    }
}
fn short(value: &str) -> String {
    value.chars().take(256).collect()
}

// Closing this job also terminates Chromium's subprocesses on Windows.
#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl WindowsJob {
    fn assign(child: &std::process::Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            ensure!(
                !handle.is_null(),
                "cannot create Chromium job: {}",
                std::io::Error::last_os_error()
            );
            let job = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            ensure!(
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32
                ) != 0,
                "cannot configure Chromium job: {}",
                std::io::Error::last_os_error()
            );
            ensure!(
                AssignProcessToJobObject(handle, child.as_raw_handle()) != 0,
                "cannot assign Chromium job: {}",
                std::io::Error::last_os_error()
            );
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_paths_list_chrome_before_chromium_in_earlier_locations() {
        let macos = standard_paths("macos", |key| (key == "HOME").then(|| "/Users/u".into()));
        assert_eq!(
            macos,
            [
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "/Users/u/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "/Applications/Chromium.app/Contents/MacOS/Chromium",
                "/Users/u/Applications/Chromium.app/Contents/MacOS/Chromium",
            ]
            .map(PathBuf::from)
        );
        let windows = standard_paths("windows", |key| match key {
            "ProgramFiles" => Some("P".into()),
            "LOCALAPPDATA" => Some("L".into()),
            _ => None,
        });
        assert_eq!(
            windows,
            [
                PathBuf::from("P").join("Google/Chrome/Application/chrome.exe"),
                PathBuf::from("L").join("Google/Chrome/Application/chrome.exe"),
                PathBuf::from("P").join("Chromium/Application/chrome.exe"),
                PathBuf::from("L").join("Chromium/Application/chrome.exe"),
            ]
        );
        assert_eq!(
            standard_paths("linux", |_| None).last(),
            Some(&PathBuf::from("/usr/bin/chromium"))
        );
    }
}
