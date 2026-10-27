//! draw.io's built-in sidebar shapes (AWS, Azure, Google Cloud, Kubernetes, UML, ...),
//! catalogued by `scripts/shape_catalog.py` from the draw.io that the vscode mode
//! follows (26.0.2). Each sidebar palette is a built-in library named with the
//! `drawio/` prefix, such as `drawio/aws4-compute`; the catalog holds cells and
//! styles, never icon images.
use anyhow::Result;
use serde_json::Value;

use crate::{
    adapters::fs,
    domain::library::{Entry, Library},
};

const CATALOG: &[u8] = include_bytes!("../../assets/shapes/catalog.json.gz");

/// Name prefix of the built-in libraries. File-based library names never contain `/`.
pub const PREFIX: &str = "drawio/";

/// Whether `library` is one of the built-in libraries.
pub fn is_builtin(library: &Library) -> bool {
    library.name.starts_with(PREFIX)
}

/// Every built-in library, in name order.
pub fn libraries() -> Result<Vec<Library>> {
    let catalog: Value =
        serde_json::from_slice(&fs::read_limited(flate2::read::GzDecoder::new(CATALOG))?)?;
    let invalid = || anyhow::anyhow!("invalid shape catalog");
    catalog["palettes"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|palette| {
            let entries = palette["entries"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|entry| {
                    let cells = entry["xml"].as_str().ok_or_else(invalid)?;
                    Ok(Entry::model(
                        entry["title"].as_str().unwrap_or("").to_owned(),
                        entry["w"].as_f64().unwrap_or(0.0),
                        entry["h"].as_f64().unwrap_or(0.0),
                        format!(
                            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{cells}</root></mxGraphModel>"#
                        ),
                    ))
                })
                .collect::<Result<_>>()?;
            let name = palette["name"].as_str().ok_or_else(invalid)?;
            Ok(Library {
                name: format!("{PREFIX}{name}"),
                title: palette["title"].as_str().map(str::to_owned),
                entries,
            })
        })
        .collect()
}
