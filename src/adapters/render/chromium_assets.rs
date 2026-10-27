use crate::adapters::fs as storage;
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tiny_http::{Header, Response, Server};

/// draw.io 31.4.5, matching the Desktop reference for the raw and desktop modes.
pub(crate) const ASSETS: &[(&str, &[u8])] = &[
    (
        "js/viewer.min.js",
        include_bytes!("../../../assets/drawio/js_viewer.min.js.gz"),
    ),
    (
        "js/export-init.js",
        include_bytes!("../../../assets/drawio/js_export-init.js.gz"),
    ),
    (
        "js/export.js",
        include_bytes!("../../../assets/drawio/js_export.js.gz"),
    ),
    (
        "mxgraph/css/common.css",
        include_bytes!("../../../assets/drawio/mxgraph_css_common.css.gz"),
    ),
];

/// draw.io 26.0.2, pinned by the stable VS Code extension 1.9.0, for the vscode mode.
pub(crate) const VSCODE_ASSETS: &[(&str, &[u8])] = &[
    (
        "js/viewer.min.js",
        include_bytes!("../../../assets/drawio/vscode/js_viewer.min.js.gz"),
    ),
    (
        "js/export-init.js",
        include_bytes!("../../../assets/drawio/vscode/js_export-init.js.gz"),
    ),
    (
        "js/export.js",
        include_bytes!("../../../assets/drawio/vscode/js_export.js.gz"),
    ),
    (
        "mxgraph/css/common.css",
        include_bytes!("../../../assets/drawio/vscode/mxgraph_css_common.css.gz"),
    ),
];

/// Shape and stencil bundles the VS Code extension loads up front. draw.io
/// releases without a `shapes/` directory cannot load these shapes on demand.
const VSCODE_PRELOAD: &[&str] = &["js/shapes-14-6-5.min.js", "js/stencils.min.js"];

/// Answers requests for the page and its assets at `origin`.
pub(crate) struct AssetStore {
    pub(crate) origin: String,
    pub(crate) bundled: bool,
    /// Scripts from the local web root to load before rendering.
    pub(crate) preload: Vec<&'static str>,
    root: Option<PathBuf>,
    assets: HashMap<String, Vec<u8>>,
    csp: String,
}

/// Serves the page document over loopback HTTP, so it keeps a local address
/// space and may load images from local hosts with --allow-network. Subresources
/// are fulfilled through CDP instead: fetched through this server under Fetch
/// interception, a script response could stall until the deadline.
pub(crate) struct AssetServer {
    pub(crate) store: Arc<AssetStore>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

/// An HTTP response for one asset request.
pub(crate) struct Asset {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(&'static str, String)>,
    pub(crate) body: Vec<u8>,
}

// `root` must be canonicalized, as it is in `AssetStore::new`.
fn local_file(root: &Path, url: &str) -> Result<PathBuf> {
    let decoded = crate::domain::diagram::percent_decode(url.split('?').next().unwrap_or(url))?;
    let relative = decoded.strip_prefix('/').context("invalid asset path")?;
    ensure!(!relative.contains('\\'), "invalid asset path");
    ensure!(
        Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "invalid asset path"
    );
    let path = root.join(relative).canonicalize()?;
    ensure!(
        path.starts_with(root) && path.is_file(),
        "asset outside web root"
    );
    Ok(path)
}

impl AssetStore {
    fn new(
        root: Option<PathBuf>,
        allow_network: bool,
        vscode: bool,
        origin: String,
    ) -> Result<Self> {
        let root = root
            .map(|p| {
                p.canonicalize()
                    .context("invalid DRAWIO_PNG_DRAWIO_WEB_PATH")
            })
            .transpose()?;
        if let Some(root) = &root {
            ensure!(
                root.is_dir() && root.join("export3.html").is_file(),
                "DRAWIO_PNG_DRAWIO_WEB_PATH must contain export3.html"
            );
        }
        let bundled = root.is_none();
        let preload = match &root {
            Some(root) if vscode => VSCODE_PRELOAD
                .iter()
                .copied()
                .filter(|name| root.join(name).is_file())
                .collect(),
            _ => Vec::new(),
        };
        let mut assets = HashMap::new();
        if bundled {
            for &(name, bytes) in if vscode { VSCODE_ASSETS } else { ASSETS } {
                let bytes = storage::read_limited(flate2::read::GzDecoder::new(bytes))?;
                assets.insert(format!("/{name}"), bytes);
            }
            assets.insert(
                "/export3.html".into(),
                include_bytes!("../../../assets/chromium.html").to_vec(),
            );
        }
        // CSP also covers WebSockets, workers and requests not intercepted by CDP.
        let sources = if allow_network {
            "'self' data: blob: http: https:"
        } else {
            "'self' data: blob:"
        };
        let csp = format!(
            "default-src {sources}; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src {sources} 'unsafe-inline'; object-src 'none'; frame-src 'none'; worker-src 'none'; base-uri 'self'"
        );
        Ok(Self {
            origin,
            bundled,
            preload,
            root,
            assets,
            csp,
        })
    }

    /// Returns the response for a request to `url`, or `None` if it is not local.
    pub(crate) fn respond(&self, method: &str, url: &str) -> Option<Asset> {
        let path = url.strip_prefix(&self.origin)?;
        if !path.starts_with('/') {
            return None;
        }
        let path = path.split(['?', '#']).next().unwrap_or(path);
        let body = if method != "GET" {
            None
        } else if let Some(root) = &self.root {
            local_file(root, path).and_then(|p| storage::read(&p)).ok()
        } else {
            self.assets.get(path).cloned()
        };
        let (body, status) = body
            .map(|b| (b, 200))
            .unwrap_or((b"asset not found".to_vec(), 404));
        let mime = match Path::new(path).extension().and_then(|s| s.to_str()) {
            Some("js") => "application/javascript",
            Some("css") => "text/css",
            Some("html") => "text/html; charset=utf-8",
            Some("xml") => "application/xml",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("woff2") => "font/woff2",
            _ => "application/octet-stream",
        };
        Some(Asset {
            status,
            headers: vec![
                ("Content-Type", mime.into()),
                ("Content-Security-Policy", self.csp.clone()),
                ("Cache-Control", "no-store".into()),
            ],
            body,
        })
    }
}

impl AssetServer {
    pub(crate) fn start(root: Option<PathBuf>, allow_network: bool, vscode: bool) -> Result<Self> {
        let server = Server::http("127.0.0.1:0")
            .map_err(|e| anyhow::anyhow!("cannot start asset server: {e}"))?;
        let origin = format!("http://{}", server.server_addr());
        let store = Arc::new(AssetStore::new(root, allow_network, vscode, origin)?);
        let stop = Arc::new(AtomicBool::new(false));
        let (signal, served) = (stop.clone(), store.clone());
        let worker = thread::spawn(move || {
            while !signal.load(Ordering::Relaxed) {
                let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(50)) else {
                    continue;
                };
                let url = format!("{}{}", served.origin, request.url());
                let Some(asset) = served.respond(request.method().as_str(), &url) else {
                    let _ = request.respond(Response::empty(400));
                    continue;
                };
                let mut response = Response::from_data(asset.body).with_status_code(asset.status);
                for (name, value) in asset.headers {
                    response.add_header(Header::from_bytes(name, value).unwrap());
                }
                let _ = request.respond(response);
            }
        });
        Ok(Self {
            store,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for AssetServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn web_root_rejects_traversal() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("ok.js"), "ok").unwrap();
        // Match the store's precondition even when the temporary directory's
        // path contains a symlink (for example, /var -> /private/var on macOS).
        let canonical_root = root.path().canonicalize().unwrap();
        assert_eq!(
            local_file(&canonical_root, "/ok.js").unwrap(),
            canonical_root.join("ok.js")
        );
        for path in ["/../secret", "/%2e%2e/secret", "/%5csecret", "/C:/secret"] {
            assert!(local_file(&canonical_root, path).is_err());
        }
    }

    #[test]
    fn store_answers_only_its_origin() {
        let server = AssetServer::start(None, false, false).unwrap();
        let store = &server.store;
        let page = store
            .respond("GET", &format!("{}/export3.html?x=1", store.origin))
            .unwrap();
        assert_eq!(page.status, 200);
        assert_eq!(page.body, include_bytes!("../../../assets/chromium.html"));
        assert!(
            page.headers
                .iter()
                .any(|(k, v)| *k == "Content-Security-Policy" && v.contains("'self' data: blob:;"))
        );
        let missing = store
            .respond("GET", &format!("{}/missing.js", store.origin))
            .unwrap();
        assert_eq!(missing.status, 404);
        let post = store
            .respond("POST", &format!("{}/export3.html", store.origin))
            .unwrap();
        assert_eq!(post.status, 404);
        // Another port on the same host, or a prefix match, is not this origin.
        for url in [
            "http://127.0.0.1:1/export3.html".to_owned(),
            format!("{}0/export3.html", store.origin),
            "https://example.com/export3.html".to_owned(),
        ] {
            assert!(store.respond("GET", &url).is_none(), "{url}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn store_accepts_symlinked_root_but_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("web");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("export3.html"), "test").unwrap();
        fs::write(root.join("ok.js"), "ok").unwrap();
        let secret = temp.path().join("secret.js");
        fs::write(&secret, "secret").unwrap();
        symlink(&secret, root.join("escape.js")).unwrap();
        let alias = temp.path().join("alias");
        symlink(&root, &alias).unwrap();

        let server = AssetServer::start(Some(alias), false, false).unwrap();
        let store = &server.store;
        for (path, status, body) in [
            ("/ok.js", 200, "ok"),
            ("/escape.js", 404, "asset not found"),
        ] {
            let asset = store
                .respond("GET", &format!("{}{path}", store.origin))
                .unwrap();
            assert_eq!(asset.status, status);
            assert_eq!(asset.body, body.as_bytes());
        }
    }
}
