//! Renderers: headless Chromium, which follows the VS Code extension or
//! draw.io Desktop, and the draw.io Desktop application itself. They
//! implement the `Renderer` port and code their failures (spec 2.3).
use std::env;

use crate::{
    app::ports::Renderer,
    domain::diagnostic::{Code, Diagnostic},
};

pub mod chromium;
mod chromium_assets;
pub mod desktop;
pub mod interrupt;
mod process;
mod resample;

pub use process::TIMEOUT;

/// The `--renderer` values (spec 5.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RendererKind {
    /// Chromium, matching the VS Code extension 1.9.0 (draw.io 26.0.2).
    #[default]
    Vscode,
    /// Chromium, matching draw.io Desktop's 2x capture and downsampling.
    DesktopCompat,
    /// Chromium, the 1x capture as it is.
    Raw,
    /// The draw.io Desktop application.
    DesktopApp,
}

impl RendererKind {
    pub const NAMES: [&'static str; 4] = ["vscode", "desktop-compat", "raw", "desktop-app"];

    pub fn as_str(self) -> &'static str {
        match self {
            RendererKind::Vscode => "vscode",
            RendererKind::DesktopCompat => "desktop-compat",
            RendererKind::Raw => "raw",
            RendererKind::DesktopApp => "desktop-app",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "vscode" => RendererKind::Vscode,
            "desktop-compat" => RendererKind::DesktopCompat,
            "raw" => RendererKind::Raw,
            "desktop-app" => RendererKind::DesktopApp,
            _ => return None,
        })
    }

    /// `D_PNG_RENDERER`, if set.
    pub fn from_env() -> Result<Option<Self>, Diagnostic> {
        let Some(value) = env::var_os("D_PNG_RENDERER") else {
            return Ok(None);
        };
        let text = value.to_str().unwrap_or("");
        Self::parse(text).map(Some).ok_or_else(|| {
            Diagnostic::new(
                Code::Usage,
                format!(
                    "invalid D_PNG_RENDERER {text:?}: expected {}",
                    Self::NAMES.join(", ")
                ),
            )
        })
    }

    pub fn renderer(self) -> Box<dyn Renderer> {
        match self {
            RendererKind::Vscode => Box::new(ChromiumRenderer(chromium::ChromiumMode::Vscode)),
            RendererKind::DesktopCompat => {
                Box::new(ChromiumRenderer(chromium::ChromiumMode::Desktop))
            }
            RendererKind::Raw => Box::new(ChromiumRenderer(chromium::ChromiumMode::Raw)),
            RendererKind::DesktopApp => Box::new(DesktopAppRenderer),
        }
    }
}

/// Headless Chromium with one of its output modes.
pub struct ChromiumRenderer(pub chromium::ChromiumMode);

impl Renderer for ChromiumRenderer {
    fn render(&self, xml: &str, page: usize, allow_network: bool) -> Result<Vec<u8>, Diagnostic> {
        chromium::render_page(xml, page, allow_network, self.0).map_err(|e| coded(&e))
    }
}

/// The draw.io Desktop application's export.
pub struct DesktopAppRenderer;

impl Renderer for DesktopAppRenderer {
    fn render(&self, xml: &str, page: usize, allow_network: bool) -> Result<Vec<u8>, Diagnostic> {
        if allow_network {
            return Err(Diagnostic::new(
                Code::Usage,
                "--allow-network needs a Chromium renderer, not desktop-app",
            ));
        }
        desktop::render_page(xml, page).map_err(|e| coded(&e))
    }
}

/// Codes a renderer failure by what went wrong.
fn coded(error: &anyhow::Error) -> Diagnostic {
    let message = format!("{error:#}");
    let has = |text: &str| message.contains(text);
    let code = if has("not found; install")
        || has("not found; set D_PNG_CHROME_PATH")
        || has("is not an executable file")
        || has("cannot start Chromium")
        || has("cannot start draw.io Desktop")
    {
        Code::RendererNotFound
    } else if has("timed out") {
        Code::RenderTimeout
    } else if has("external resource blocked") {
        Code::NetworkBlocked
    } else if has("Unsupported shape")
        || has("require local draw.io assets")
        || has("draw.io resource failed")
        || has("cannot load draw.io assets")
        || has("D_PNG_DRAWIO_WEB_PATH")
    {
        Code::AssetMissing
    } else if has("exceeds 64 MiB") {
        Code::LimitExceeded
    } else {
        Code::RenderFailed
    };
    Diagnostic::new(code, message)
}
