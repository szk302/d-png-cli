//! The page `library preview` renders: entries laid out in rows, each under a label.
use anyhow::{Context, Result, ensure};

use crate::domain::{
    codec,
    expand::{self, Resolved},
    library::Entry,
    reference::Ref,
};

/// Most entries one preview shows; more would be too small to tell apart.
pub const PREVIEW_LIMIT: usize = 60;

/// One page laying out the given entries, each below a label naming its
/// reference and title, for `d-png-cli library preview`. The label goes
/// above the entry because sidebar shapes often show their own label below.
/// Entries are placed with the same expansion as references in diagrams.
pub fn preview(entries: &[(Ref, &Entry)]) -> Result<String> {
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
    let mut cells = String::new();
    let mut resolved = Resolved::default();
    let (mut x, mut y, mut row) = (0.0, 0.0, 0.0_f64);
    for (index, (reference, entry)) in entries.iter().enumerate() {
        // Shrink large single vertices to a common size; other entries keep
        // theirs, so their slot follows their actual extent.
        let (single, extent_width, extent_height) = entry
            .extent()
            .with_context(|| format!("cannot place {reference}"))?;
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
        let size = if scale < 1.0 {
            format!(
                r#" width="{}" height="{}""#,
                codec::number(width),
                codec::number(height)
            )
        } else {
            String::new()
        };
        cells.push_str(&format!(
            r#"<mxCell id="entry{index}" parent="1" libref="{}"><mxGeometry x="{}" y="{}"{size} as="geometry"/></mxCell>"#,
            codec::attribute(&reference.to_string()),
            codec::number(x + (slot - width) / 2.0),
            codec::number(y + LABEL + 4.0 + (shown - height) / 2.0),
        ));
        let text = format!("{reference}\n{}", entry.title);
        cells.push_str(&format!(
            r#"<mxCell id="label{index}" value="{}" style="text;html=0;align=center;verticalAlign=bottom;whiteSpace=wrap;fontSize=10;fontColor=#333333;" vertex="1" parent="1"><mxGeometry x="{}" y="{}" width="{}" height="{LABEL}" as="geometry"/></mxCell>"#,
            escape(&text),
            codec::number(x + 5.0),
            codec::number(y),
            codec::number(slot - 10.0),
        ));
        resolved.entries.insert(reference.clone(), (*entry).clone());
        row = row.max(LABEL + 4.0 + shown + OWN_LABEL);
        x += slot;
    }
    let xml = format!(
        r#"<mxfile><diagram name="Preview" id="preview"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{cells}</root></mxGraphModel></diagram></mxfile>"#
    );
    expand::expand(&xml, &resolved)
        .map(|expansion| expansion.xml)
        .map_err(|problems| anyhow::anyhow!("cannot lay out the preview: {problems}"))
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
