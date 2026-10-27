//! How results and diagnostics look on the terminal and in `--json`.
use serde_json::{Value, json};

use crate::{
    app::{
        diagram::{Read, Written},
        libraries::Hit,
        library::{EntryDetails, Shown},
        ports::LibraryInfo,
    },
    domain::{
        codec,
        diagnostic::{Diagnostic, Location},
        expand::{Expanded, ExpandedRef},
        library::Origin,
    },
};

fn location_json(location: &Location, value: &mut serde_json::Map<String, Value>) {
    if let Some(page) = location.page {
        value.insert("page".into(), json!(page));
    }
    if let Some(name) = &location.page_name {
        value.insert("pageName".into(), json!(name));
    }
    if let Some(cell) = &location.cell {
        value.insert("cell".into(), json!(cell));
    }
}

pub fn diagnostic_json(d: &Diagnostic) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("code".into(), json!(d.code.as_str()));
    value.insert("message".into(), json!(d.message));
    location_json(&d.location, &mut value);
    if let Some(reference) = &d.reference {
        value.insert("ref".into(), json!(reference));
    }
    if !d.candidates.is_empty() {
        value.insert("candidates".into(), json!(d.candidates));
    }
    Value::Object(value)
}

/// `error[CODE] page 1 "name", cell "id": message`, then candidates.
pub fn diagnostic_text(level: &str, d: &Diagnostic) -> String {
    let mut place = Vec::new();
    if let Some(page) = d.location.page {
        place.push(match &d.location.page_name {
            Some(name) => format!("page {page} {name:?}"),
            None => format!("page {page}"),
        });
    }
    if let Some(cell) = &d.location.cell {
        place.push(format!("cell {cell:?}"));
    }
    let mut text = format!("{level}[{}] ", d.code);
    if !place.is_empty() {
        text.push_str(&place.join(", "));
        text.push_str(": ");
    }
    text.push_str(&d.message);
    if !d.candidates.is_empty() {
        text.push_str(&format!("\n  candidates: {}", d.candidates.join(", ")));
    }
    text
}

/// The errors and warnings on standard error, as text or one JSON line.
pub fn diagnostics(errors: &[Diagnostic], warnings: &[Diagnostic], json: bool) -> String {
    if json {
        let value = json!({
            "errors": errors.iter().map(diagnostic_json).collect::<Vec<_>>(),
            "warnings": warnings.iter().map(diagnostic_json).collect::<Vec<_>>(),
        });
        return format!("{value}\n");
    }
    let mut text = String::new();
    for d in errors {
        text.push_str(&diagnostic_text("error", d));
        text.push('\n');
    }
    for d in warnings {
        text.push_str(&diagnostic_text("warning", d));
        text.push('\n');
    }
    text
}

pub fn expanded_json(expanded: &[ExpandedRef]) -> Value {
    Value::Array(
        expanded
            .iter()
            .map(|e| {
                let mut value = serde_json::Map::new();
                location_json(&e.location, &mut value);
                value.insert("ref".into(), json!(e.reference));
                match &e.result {
                    Expanded::Cells(ids) => {
                        value.insert("type".into(), json!("cells"));
                        value.insert("ids".into(), json!(ids));
                    }
                    Expanded::Image => {
                        value.insert("type".into(), json!("image"));
                    }
                }
                Value::Object(value)
            })
            .collect(),
    )
}

pub fn written_json(written: &Written, output: &str, renderer: Option<&str>) -> Value {
    json!({
        "output": output,
        "width": written.width,
        "height": written.height,
        "renderer": renderer,
        "page": written.page,
        "pages": written.pages,
        "expanded": expanded_json(&written.expanded),
        "warnings": written.warnings.iter().map(diagnostic_json).collect::<Vec<_>>(),
    })
}

pub fn read_json(read: &Read, output: &str) -> Value {
    let located = |location: &Location, extra: Value| {
        let mut value = serde_json::Map::new();
        location_json(location, &mut value);
        if let Value::Object(extra) = extra {
            value.extend(extra);
        }
        Value::Object(value)
    };
    json!({
        "output": output,
        "pages": read.pages,
        "compacted": read.compacted.iter().map(|c| located(&c.location, json!({"ref": c.reference, "replacedChars": c.replaced_chars}))).collect::<Vec<_>>(),
        "keptImages": read.kept.iter().map(|k| located(&k.location, json!({"chars": k.chars}))).collect::<Vec<_>>(),
    })
}

fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// A size for display, without floating-point noise.
fn size(value: f64) -> String {
    codec::number((value * 100.0).round() / 100.0)
}

pub fn library_json(origin: Origin, info: &LibraryInfo) -> Value {
    json!({
        "name": info.name.to_string(),
        "origin": origin.as_str(),
        "title": info.title,
        "entries": info.entries,
        "file": info.location,
    })
}

pub fn library_line(info: &LibraryInfo) -> String {
    format!(
        "{}\t-\t{}\t{}\n",
        info.name,
        info.entries,
        info.title.as_deref().map_or("-".into(), one_line)
    )
}

pub fn hit_json(hit: &Hit) -> Value {
    json!({
        "ref": hit.reference.to_string(),
        "library": hit.reference.library.to_string(),
        "id": hit.reference.entry.as_str(),
        "title": hit.title,
        "kind": hit.kind.as_str(),
        "width": hit.width,
        "height": hit.height,
        "origin": hit.origin.as_str(),
    })
}

pub fn hit_line(hit: &Hit) -> String {
    format!(
        "{}\t{}\t{}x{}\t{}\n",
        hit.reference,
        hit.kind,
        size(hit.width),
        size(hit.height),
        one_line(&hit.title)
    )
}

pub fn shown_json(shown: &Shown) -> Value {
    match shown {
        Shown::Library(origin, info) => library_json(*origin, info),
        Shown::Entry(details) => entry_json(details),
    }
}

fn entry_json(details: &EntryDetails) -> Value {
    let mut value = json!({
        "ref": details.reference.to_string(),
        "title": details.title,
        "kind": details.kind.as_str(),
        "width": details.width,
        "height": details.height,
        "origin": details.origin.as_str(),
        "example": details.example,
    });
    let object = value.as_object_mut().expect("object");
    if let Some(description) = &details.description {
        if let Some(label) = &description.label {
            object.insert("label".into(), json!(label));
        }
        if let Some(style) = &description.style {
            object.insert("style".into(), json!(style));
        }
        if !description.data.is_empty() {
            object.insert(
                "data".into(),
                Value::Object(
                    description
                        .data
                        .iter()
                        .map(|(k, v)| (k.clone(), json!(v)))
                        .collect(),
                ),
            );
        }
        object.insert(
            "cells".into(),
            Value::Array(
                description
                    .cells
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "kind": if c.edge { "edge" } else { "vertex" },
                            "label": c.label,
                            "parent": c.parent,
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some((fingerprint, image)) = &details.image {
        object.insert(
            "image".into(),
            json!({"fingerprint": fingerprint, "mediaType": image.media_type, "bytes": image.bytes}),
        );
    }
    value
}

pub fn shown_text(shown: &Shown) -> String {
    match shown {
        Shown::Library(origin, info) => format!(
            "name: {}\ntitle: {}\norigin: {}\nentries: {}\nfile: {}\n",
            info.name,
            info.title.as_deref().map_or("-".into(), one_line),
            origin.as_str(),
            info.entries,
            info.location.as_deref().unwrap_or("-"),
        ),
        Shown::Entry(details) => {
            let mut text = format!(
                "ref: {}\ntitle: {}\nkind: {}\nsize: {}x{}\norigin: {}\n",
                details.reference,
                one_line(&details.title),
                details.kind,
                size(details.width),
                size(details.height),
                details.origin.as_str(),
            );
            if let Some(description) = &details.description {
                if let Some(label) = &description.label {
                    text.push_str(&format!("label: {}\n", one_line(label)));
                }
                if let Some(style) = &description.style {
                    text.push_str(&format!("style: {}\n", one_line(style)));
                }
                for (key, value) in &description.data {
                    text.push_str(&format!("data: {key}={}\n", one_line(value)));
                }
                text.push_str("cells:\n");
                for cell in &description.cells {
                    text.push_str(&format!(
                        "  {}\t{}\t{}\t{}\n",
                        cell.id,
                        if cell.edge { "edge" } else { "vertex" },
                        one_line(&cell.label),
                        cell.parent.as_deref().unwrap_or("-")
                    ));
                }
            }
            if let Some((fingerprint, image)) = &details.image {
                text.push_str(&format!(
                    "image: {} {} bytes, fingerprint {fingerprint}\n",
                    image.media_type, image.bytes
                ));
            }
            text.push_str(&format!("example: {}\n", details.example));
            text
        }
    }
}
