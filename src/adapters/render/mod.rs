//! Renderers: headless Chromium, which follows the VS Code extension or draw.io
//! Desktop, and the draw.io Desktop application itself.
use anyhow::{Result, ensure};
use std::env;

pub mod chromium;
mod chromium_assets;
pub mod desktop;
pub mod interrupt;
mod process;
mod resample;

pub use process::TIMEOUT;

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
    chromium_mode: Option<chromium::ChromiumMode>,
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
            desktop::render(xml)
        }
        Renderer::Chromium => chromium::render_configured(xml, allow_network, chromium_mode),
        Renderer::Auto => {
            if let Some(program) = desktop::discover_optional()? {
                ensure!(
                    chromium_mode.is_none(),
                    "Desktop was selected; use --renderer chromium with --chromium-mode"
                );
                ensure!(
                    !allow_network,
                    "Desktop was selected; use --renderer chromium with --allow-network"
                );
                let args =
                    desktop::parse_extra_args(env::var_os("DRAWIO_PNG_DRAWIO_ARGS").as_deref())?;
                desktop::render_with_args(&program, xml, TIMEOUT, &args)
            } else {
                chromium::render_configured(xml, allow_network, chromium_mode)
            }
        }
    }
}
