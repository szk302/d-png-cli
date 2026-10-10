//! draw.io's built-in sidebar shapes (AWS, Azure, Google Cloud, Kubernetes, UML, ...),
//! catalogued by `scripts/shape_catalog.py` from the draw.io that the vscode mode
//! follows (26.0.2). Each sidebar palette is a built-in library named with the
//! `drawio/` prefix, such as `drawio/aws4-compute`; the catalog holds cells and
//! styles, never icon images.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::PathBuf;

use crate::{
    library::{self, Entry, Library, Placement},
    storage,
};

const CATALOG: &[u8] = include_bytes!("../assets/shapes/catalog.json.gz");

/// Name prefix of the built-in libraries. File-based library names never contain `/`.
pub const PREFIX: &str = "drawio/";

/// Most entries one preview shows; more would be too small to tell apart.
pub const PREVIEW_LIMIT: usize = 60;

/// Whether `library` is one of the built-in libraries.
pub fn is_builtin(library: &Library) -> bool {
    library.name.starts_with(PREFIX)
}

/// Every built-in library, in name order.
pub fn libraries() -> Result<Vec<Library>> {
    let catalog: Value = serde_json::from_slice(&storage::read_limited(
        flate2::read::GzDecoder::new(CATALOG),
    )?)?;
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
                path: PathBuf::new(),
                entries,
            })
        })
        .collect()
}

/// One page laying out the given entries, each below a label naming its
/// library, index (1-based) and title, for `drawio-png-cli library preview`. The label
/// goes above the entry because sidebar shapes often show their own label below.
pub fn preview(entries: &[(&Library, usize)]) -> Result<String> {
    const ICON: f64 = 96.0;
    const SLOT: f64 = 160.0;
    const WIDTH: f64 = 960.0;
    const LABEL: f64 = 30.0;
    // Room below each entry for its own label, such as verticalLabelPosition=bottom.
    const OWN_LABEL: f64 = 36.0;
    ensure!(!entries.is_empty(), "no entries to preview");
    ensure!(
        entries.len() <= PREVIEW_LIMIT,
        "{} entries match; narrow the query or --library to at most {PREVIEW_LIMIT}",
        entries.len()
    );
    let mut xml = String::from(
        r#"<mxfile><diagram name="Preview" id="preview"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#,
    );
    let (mut x, mut y, mut row) = (0.0, 0.0, 0.0_f64);
    for (library, index) in entries {
        let entry = &library.entries[*index];
        // Shrink large single vertices to a common size; other entries keep
        // theirs, so their slot follows their actual extent.
        let (single, extent_width, extent_height) = entry.extent()?;
        // draw.io turns a rotated vertex about its centre; lay out its turned bounds.
        let turn = if single {
            rotation(entry.cell_style()?.as_deref().unwrap_or("")).to_radians()
        } else {
            0.0
        };
        let (cos, sin) = (turn.cos().abs(), turn.sin().abs());
        let bounds = |w: f64, h: f64| (w * cos + h * sin, w * sin + h * cos);
        let (turned_width, turned_height) = bounds(extent_width, extent_height);
        let scale = if single {
            (ICON / turned_width.max(turned_height)).min(1.0)
        } else {
            1.0
        };
        let (width, height) = (extent_width * scale, extent_height * scale);
        let (shown_width, shown) = bounds(width, height);
        let slot = shown_width.max(SLOT - 20.0) + 20.0;
        if x > 0.0 && x + slot > WIDTH {
            (x, y, row) = (0.0, y + row, 0.0);
        }
        let placement = Placement {
            page: 1,
            x: x + (slot - width) / 2.0,
            y: y + LABEL + 4.0 + (shown - height) / 2.0,
            width: (scale < 1.0).then_some(width),
            height: (scale < 1.0).then_some(height),
            id: None,
            label: None,
        };
        let (next, _) = library::insert(&xml, entry, &placement).with_context(|| {
            format!(
                "cannot place {} #{} {:?}",
                library.name,
                index + 1,
                entry.title
            )
        })?;
        let text = format!("{} #{}\n{}", library.name, index + 1, entry.title);
        let label = Entry::model(
            String::new(),
            slot - 10.0,
            LABEL,
            format!(
                r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="{}" style="text;html=0;align=center;verticalAlign=bottom;whiteSpace=wrap;fontSize=10;fontColor=#333333;" vertex="1" parent="1"><mxGeometry width="{}" height="{LABEL}" as="geometry"/></mxCell></root></mxGraphModel>"#,
                escape(&text),
                slot - 10.0
            ),
        );
        let label_at = Placement {
            page: 1,
            x: x + 5.0,
            y,
            width: None,
            height: None,
            id: None,
            label: None,
        };
        xml = library::insert(&next, &label, &label_at)?.0;
        row = row.max(LABEL + 4.0 + shown + OWN_LABEL);
        x += slot;
    }
    Ok(xml)
}

/// The rotation=… of a style in degrees, 0 when absent.
fn rotation(style: &str) -> f64 {
    style
        .split(';')
        .filter_map(|part| part.strip_prefix("rotation="))
        .next_back()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0)
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
