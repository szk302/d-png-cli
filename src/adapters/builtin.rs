//! draw.io's built-in sidebar shapes (AWS, Azure, Google Cloud, Kubernetes, UML, ...),
//! catalogued by `scripts/shape_catalog.py` from the draw.io that the vscode
//! renderer follows (26.0.2). Each sidebar palette is a library of the
//! `@drawio` scope, such as `@drawio/aws4-compute`; the catalog holds cells
//! and styles, never icon images.
use anyhow::{Context, Result};
use serde_json::Value;
use std::{cell::OnceCell, collections::HashSet};

use crate::{
    adapters::fs,
    app::ports::{LibraryInfo, LibraryRepository},
    domain::{
        diagnostic::{Code, Diagnostic},
        library::{self, Entry, Library, Origin},
        reference::{BUILTIN_SCOPE, EntryId, LibraryName},
    },
};

const CATALOG: &[u8] = include_bytes!("../../assets/shapes/catalog.json.gz");

/// The built-in libraries, read from the bundled catalog on first use.
#[derive(Default)]
pub struct BuiltinLibraries {
    libraries: OnceCell<Vec<Library>>,
}

impl BuiltinLibraries {
    fn all(&self) -> Result<&[Library], Diagnostic> {
        if let Some(libraries) = self.libraries.get() {
            return Ok(libraries);
        }
        let libraries = catalog().map_err(|e| Diagnostic::from_error(Code::LibraryInvalid, &e))?;
        Ok(self.libraries.get_or_init(|| libraries))
    }
}

impl LibraryRepository for BuiltinLibraries {
    fn origin(&self) -> Origin {
        Origin::Builtin
    }

    fn list(&self) -> Result<Vec<LibraryInfo>, Diagnostic> {
        Ok(self
            .all()?
            .iter()
            .map(|library| LibraryInfo {
                name: library.name.clone(),
                title: library.title.clone(),
                entries: library.entries.len(),
                location: None,
            })
            .collect())
    }

    fn load(&self, name: &LibraryName) -> Result<Option<Library>, Diagnostic> {
        if !name.is_builtin() {
            return Ok(None);
        }
        Ok(self.all()?.iter().find(|l| &l.name == name).cloned())
    }
}

/// Every built-in library, in name order. Entry IDs come from the titles.
fn catalog() -> Result<Vec<Library>> {
    let catalog: Value =
        serde_json::from_slice(&fs::read_limited(flate2::read::GzDecoder::new(CATALOG))?)?;
    let invalid = || anyhow::anyhow!("invalid shape catalog");
    let mut libraries: Vec<Library> = catalog["palettes"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|palette| {
            let mut used = HashSet::new();
            let entries = palette["entries"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|entry| {
                    let cells = entry["xml"].as_str().ok_or_else(invalid)?;
                    let title = entry["title"].as_str().unwrap_or("").to_owned();
                    Ok(Entry::model(
                        library::unique(EntryId::derive(&title), &mut used),
                        title,
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
                name: LibraryName::new(Some(BUILTIN_SCOPE), name)
                    .map_err(anyhow::Error::new)
                    .context("invalid palette name in the shape catalog")?,
                title: palette["title"].as_str().map(str::to_owned),
                entries,
            })
        })
        .collect::<Result<_>>()?;
    libraries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(libraries)
}
