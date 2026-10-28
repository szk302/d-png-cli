//! Optional, persistent cell font preferences shared by both renderers.
use anyhow::{Result, ensure};
use std::ops::Range;

use crate::{MAX_BYTES, domain::diagram};

/// One family per option; CSS lists are built here, not accepted as option syntax.
pub fn parse_family(value: &str) -> Result<String, String> {
    let family = value.trim();
    if family.is_empty() || value.chars().any(char::is_control) || family.contains([';', ',', '='])
    {
        return Err(
            "specify one nonempty font family without control characters, ';', ',' or '='".into(),
        );
    }
    Ok(family.to_owned())
}

pub struct FontPolicy {
    default: String,
    fallbacks: Vec<String>,
}

impl FontPolicy {
    pub fn new(default: &str, fallbacks: &[String]) -> Result<Self> {
        let parse = |value: &str| parse_family(value).map_err(anyhow::Error::msg);
        Ok(Self {
            default: css_family(&parse(default)?),
            fallbacks: fallbacks
                .iter()
                .map(|value| parse(value).map(|v| css_family(&v)))
                .collect::<Result<_>>()?,
        })
    }

    /// Resolve cell-level defaults into XML so rendering and reopening share them.
    /// Explicit fonts inside HTML labels are left intact.
    pub fn apply(&self, xml: &str) -> Result<String> {
        let xml = diagram::normalize(xml)?;
        let doc = roxmltree::Document::parse(&xml)?;
        let mut changes: Vec<(Range<usize>, String)> = Vec::new();
        let mut output_size = xml.len();
        let top = doc.root_element();
        let models: Vec<_> = if top.has_tag_name("mxGraphModel") {
            vec![top]
        } else {
            top.children()
                .filter(|n| n.has_tag_name("diagram"))
                .flat_map(|page| page.children().filter(|n| n.has_tag_name("mxGraphModel")))
                .collect()
        };
        for model in models {
            // Only graph cells, including user-object wrappers; ignore extension data.
            for root in model.children().filter(|n| n.has_tag_name("root")) {
                for entry in root.children().filter(|n| n.is_element()) {
                    let cells: Vec<_> = if entry.has_tag_name("mxCell") {
                        vec![entry]
                    } else {
                        entry
                            .children()
                            .filter(|n| n.has_tag_name("mxCell"))
                            .collect()
                    };
                    for cell in cells {
                        if cell.attribute("vertex") != Some("1")
                            && cell.attribute("edge") != Some("1")
                        {
                            continue;
                        }
                        let attr = cell
                            .attributes()
                            .find(|a| a.name() == "style" && a.namespace().is_none());
                        let style = attr.map(|a| a.value()).unwrap_or("");
                        let updated = self.cell_style(style)?;
                        if updated == style {
                            continue;
                        }
                        let (range, replacement) = if let Some(attr) = attr {
                            (
                                attr.range(),
                                format!("style=\"{}\"", xml_attribute(&updated)),
                            )
                        } else {
                            // Insert immediately after the element name, before any attributes.
                            let start = cell.range().start + "<mxCell".len();
                            (
                                start..start,
                                format!(" style=\"{}\"", xml_attribute(&updated)),
                            )
                        };
                        output_size = output_size - range.len() + replacement.len();
                        ensure!(
                            output_size <= MAX_BYTES,
                            "font-adjusted XML exceeds 64 MiB limit"
                        );
                        changes.push((range, replacement));
                    }
                }
            }
        }
        changes.sort_by_key(|(range, _)| range.start);
        let mut output = String::with_capacity(output_size);
        let mut start = 0;
        for (range, replacement) in changes {
            output.push_str(&xml[start..range.start]);
            output.push_str(&replacement);
            start = range.end;
        }
        output.push_str(&xml[start..]);
        Ok(output)
    }

    fn cell_style(&self, style: &str) -> Result<String> {
        // draw.io registers fontSource under the entire fontFamily value. Turning
        // that name into a CSS list would break the existing web font binding.
        if style
            .split(';')
            .filter_map(|entry| entry.strip_prefix("fontSource="))
            .next_back()
            .is_some_and(|source| !source.is_empty() && source != "none")
        {
            return Ok(style.to_owned());
        }
        // mxStylesheet uses the last value when a key occurs more than once.
        let specified = style
            .split(';')
            .filter_map(|entry| entry.strip_prefix("fontFamily="))
            .next_back();
        let primary = specified
            .filter(|v| !v.is_empty() && *v != "none")
            .unwrap_or(&self.default);
        if matches!(
            primary,
            "inherit" | "initial" | "unset" | "revert" | "revert-layer"
        ) {
            return Ok(style.to_owned());
        }
        let family = if self.fallbacks.is_empty() {
            primary.to_owned()
        } else {
            let mut families = split_families(primary)?;
            let mut position = families
                .iter()
                .position(|f| generic(f))
                .unwrap_or(families.len());
            for fallback in &self.fallbacks {
                if !families
                    .iter()
                    .any(|f| family_key(f) == family_key(fallback))
                {
                    families.insert(position, fallback.clone());
                    position += 1;
                }
            }
            families.join(", ")
        };
        if specified == Some(family.as_str()) {
            return Ok(style.to_owned());
        }
        // A final value overrides named styles while retaining their other properties.
        let mut updated = style
            .split(';')
            .filter(|entry| !entry.starts_with("fontFamily="))
            .collect::<Vec<_>>()
            .join(";");
        if !updated.is_empty() && !updated.ends_with(';') {
            updated.push(';');
        }
        updated.push_str("fontFamily=");
        updated.push_str(&family);
        updated.push(';');
        Ok(updated)
    }
}

fn generic(family: &str) -> bool {
    matches!(
        family.to_ascii_lowercase().as_str(),
        "serif"
            | "sans-serif"
            | "monospace"
            | "cursive"
            | "fantasy"
            | "system-ui"
            | "ui-serif"
            | "ui-sans-serif"
            | "ui-monospace"
            | "ui-rounded"
            | "emoji"
            | "math"
            | "fangsong"
    )
}

fn css_family(family: &str) -> String {
    if generic(family) {
        family.to_ascii_lowercase()
    } else {
        format!("\"{}\"", family.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn family_key(family: &str) -> String {
    let family = family.trim();
    let family = if (family.starts_with('"') && family.ends_with('"'))
        || (family.starts_with('\'') && family.ends_with('\''))
    {
        &family[1..family.len() - 1]
    } else {
        family
    };
    family
        .replace("\\\"", "\"")
        .replace("\\'", "'")
        .replace("\\\\", "\\")
        .to_lowercase()
}

fn split_families(value: &str) -> Result<Vec<String>> {
    let mut families = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut start = 0;
    for (index, c) in value.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() {
            if matches!(c, '\'' | '"') {
                quote = Some(c);
            } else if c == ',' {
                families.push(value[start..index].trim().to_owned());
                start = index + 1;
            }
        }
    }
    ensure!(
        quote.is_none() && !escaped,
        "invalid fontFamily list: unmatched quote or escape"
    );
    families.push(value[start..].trim().to_owned());
    ensure!(
        families.iter().all(|f| !f.is_empty()),
        "invalid fontFamily list: empty family"
    );
    Ok(families)
}

fn xml_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
        .replace('\t', "&#9;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}
