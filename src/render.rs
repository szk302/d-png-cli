use anyhow::{Context, Result, bail, ensure};
use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{MAX_BYTES, png_data, storage};

pub const TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn discover() -> Result<PathBuf> {
    discover_optional()?
        .context("draw.io Desktop not found; install it or set DRAWIO_PNG_DRAWIO_PATH")
}

pub(crate) fn discover_optional() -> Result<Option<PathBuf>> {
    if let Some(path) = env::var_os("DRAWIO_PNG_DRAWIO_PATH") {
        let path = PathBuf::from(path);
        ensure!(
            executable(&path),
            "DRAWIO_PNG_DRAWIO_PATH is not an executable file: {}",
            path.display()
        );
        return path
            .canonicalize()
            .map(Some)
            .context("cannot resolve DRAWIO_PNG_DRAWIO_PATH");
    }
    let names: &[&str] = if cfg!(windows) {
        &["drawio.exe", "draw.io.exe"]
    } else {
        &["drawio", "draw.io"]
    };
    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            for name in names {
                let candidate = directory.join(name);
                if executable(&candidate) {
                    return Ok(Some(candidate.canonicalize()?));
                }
            }
        }
    }
    let mut defaults = Vec::<PathBuf>::new();
    match env::consts::OS {
        "macos" => {
            defaults.push("/Applications/draw.io.app/Contents/MacOS/draw.io".into());
            if let Some(home) = env::var_os("HOME") {
                defaults.push(
                    PathBuf::from(home).join("Applications/draw.io.app/Contents/MacOS/draw.io"),
                );
            }
        }
        "windows" => {
            for key in ["ProgramFiles", "LOCALAPPDATA"] {
                if let Some(base) = env::var_os(key) {
                    let base = PathBuf::from(base);
                    defaults.push(base.join("draw.io/draw.io.exe"));
                    defaults.push(base.join("Programs/draw.io/draw.io.exe"));
                }
            }
        }
        _ => defaults.push("/opt/drawio/drawio".into()),
    }
    for path in defaults {
        if executable(&path) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub(crate) struct ChildGuard {
    pub(crate) child: Child,
    pub(crate) finished: bool,
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.finished {
            #[cfg(unix)]
            // The child starts a fresh process group; kill its helpers on timeout too.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub(crate) fn log_excerpt(log: &mut File) -> String {
    let _ = log.seek(SeekFrom::Start(0));
    let mut bytes = Vec::new();
    let _ = log.take(8192).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

fn parse_extra_args(value: Option<&OsStr>) -> Result<Vec<String>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let text = value
        .to_str()
        .context("DRAWIO_PNG_DRAWIO_ARGS must be valid Unicode")?;
    shell_words::split(text).context("invalid DRAWIO_PNG_DRAWIO_ARGS: check quotes and escapes")
}

pub fn render(xml: &str) -> Result<Vec<u8>> {
    let args = parse_extra_args(env::var_os("DRAWIO_PNG_DRAWIO_ARGS").as_deref())?;
    render_with_args(&discover()?, xml, TIMEOUT, &args)
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum Renderer {
    #[default]
    Auto,
    Desktop,
    Chromium,
}

pub fn render_selected(xml: &str, renderer: Renderer, allow_network: bool) -> Result<Vec<u8>> {
    render_selected_with_mode(xml, renderer, allow_network, None)
}

pub fn render_selected_with_mode(
    xml: &str,
    renderer: Renderer,
    allow_network: bool,
    chromium_mode: Option<crate::chromium::ChromiumMode>,
) -> Result<Vec<u8>> {
    match renderer {
        Renderer::Desktop => {
            ensure!(
                chromium_mode.is_none(),
                "--chromium-mode requires --renderer chromium"
            );
            ensure!(
                !allow_network,
                "--allow-network requires the Chromium renderer"
            );
            render(xml)
        }
        Renderer::Chromium => crate::chromium::render_configured(xml, allow_network, chromium_mode),
        Renderer::Auto => {
            if let Some(program) = discover_optional()? {
                ensure!(
                    chromium_mode.is_none(),
                    "Desktop was selected; use --renderer chromium with --chromium-mode"
                );
                ensure!(
                    !allow_network,
                    "Desktop was selected; use --renderer chromium with --allow-network"
                );
                let args = parse_extra_args(env::var_os("DRAWIO_PNG_DRAWIO_ARGS").as_deref())?;
                render_with_args(&program, xml, TIMEOUT, &args)
            } else {
                crate::chromium::render_configured(xml, allow_network, chromium_mode)
            }
        }
    }
}

/// Kept independent of discovery so renderer failures can be tested without Desktop.
pub fn render_with(program: &Path, xml: &str, timeout: Duration) -> Result<Vec<u8>> {
    render_with_args(program, xml, timeout, &[])
}

fn render_with_args(
    program: &Path,
    xml: &str,
    timeout: Duration,
    args: &[String],
) -> Result<Vec<u8>> {
    // Declared first so it is dropped after Desktop and its temporary directory.
    let _interrupt = crate::interrupt::Guard::new();
    let directory = tempfile::Builder::new()
        .prefix("drawio-png-cli-render-")
        .tempdir()?;
    let input = directory.path().join("input.drawio");
    let output = directory.path().join("output.png");
    fs::write(&input, xml)?;
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
            OsStr::new("--export"),
            OsStr::new("--format"),
            OsStr::new("png"),
            OsStr::new("--page-index"),
            OsStr::new("1"),
            OsStr::new("--output"),
        ])
        .arg(&output)
        .arg(&input)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log.try_clone()?))
        .spawn()
        .with_context(|| format!("cannot start draw.io Desktop: {}", program.display()))?;
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let deadline = Instant::now() + timeout;
    loop {
        crate::interrupt::check()?;
        if let Some(status) = child.child.try_wait()? {
            child.finished = true;
            ensure!(
                status.success(),
                "draw.io Desktop failed ({status}): {}",
                log_excerpt(&mut log)
            );
            break;
        }
        if Instant::now() >= deadline {
            bail!(
                "draw.io Desktop timed out after {} seconds: {}",
                timeout.as_secs_f64(),
                log_excerpt(&mut log)
            );
        }
        ensure!(
            log.metadata()?.len() <= MAX_BYTES as u64,
            "draw.io Desktop log exceeds 64 MiB limit"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let png = storage::read(&output).with_context(|| {
        format!(
            "draw.io Desktop produced no readable PNG: {}",
            log_excerpt(&mut log)
        )
    })?;
    png_data::validate(&png).context("draw.io Desktop produced an invalid PNG")?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::parse_extra_args;
    use std::ffi::OsStr;

    #[test]
    fn optional_args_allow_quotes_and_escapes() {
        assert!(parse_extra_args(None).unwrap().is_empty());
        for value in ["", " \t\n"] {
            assert!(
                parse_extra_args(Some(OsStr::new(value)))
                    .unwrap()
                    .is_empty()
            );
        }
        let args = parse_extra_args(Some(OsStr::new(
            r#"--disable-gpu --user-data-dir="/tmp/日本語 profile" 'a b' escaped\ space "" 'C:\path with spaces'"#,
        ))).unwrap();
        assert_eq!(
            args,
            [
                "--disable-gpu",
                "--user-data-dir=/tmp/日本語 profile",
                "a b",
                "escaped space",
                "",
                r"C:\path with spaces"
            ]
        );
    }

    #[test]
    fn args_are_not_shell_expanded() {
        let args = parse_extra_args(Some(OsStr::new(
            r#"'$HOME' "$(touch marker)" '`id`' *.xml ~/profile ; | >"#,
        )))
        .unwrap();
        assert_eq!(
            args,
            [
                "$HOME",
                "$(touch marker)",
                "`id`",
                "*.xml",
                "~/profile",
                ";",
                "|",
                ">"
            ]
        );
    }

    #[test]
    fn unmatched_quotes_are_errors() {
        for value in ["'unfinished", "\"unfinished"] {
            let error = parse_extra_args(Some(OsStr::new(value))).unwrap_err();
            assert!(error.to_string().contains("DRAWIO_PNG_DRAWIO_ARGS"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_args_are_errors() {
        use std::os::unix::ffi::OsStrExt;
        let error = parse_extra_args(Some(OsStr::from_bytes(b"\xff"))).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("DRAWIO_PNG_DRAWIO_ARGS must be valid Unicode")
        );
    }
}
