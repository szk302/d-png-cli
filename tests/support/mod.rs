//! Helpers shared by the integration tests.
#![allow(dead_code)]
use assert_cmd::{Command, cargo::cargo_bin_cmd};
use drawio_png_cli::{
    app::ports::{LibraryInfo, LibraryRepository},
    domain::{
        diagnostic::Diagnostic,
        library::{self, Library, Origin},
        reference::LibraryName,
    },
};
use std::{collections::BTreeMap, io::Write};

/// The command with no renderer, project or library settings from the environment.
pub fn cli() -> Command {
    let mut command = cargo_bin_cmd!("drawio-png-cli");
    for name in [
        "DRAWIO_PNG_DRAWIO_PATH",
        "DRAWIO_PNG_DRAWIO_ARGS",
        "DRAWIO_PNG_CHROME_PATH",
        "CHROME_PATH",
        "DRAWIO_PNG_CHROME_ARGS",
        "DRAWIO_PNG_RENDERER",
        "DRAWIO_PNG_DRAWIO_WEB_PATH",
        "DRAWIO_PNG_PROJECT",
    ] {
        command.env_remove(name);
    }
    // Neither the developer's settings nor their cache take part.
    command
        .env(
            "DRAWIO_PNG_HOME",
            std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-user-area"),
        )
        .env(
            "DRAWIO_PNG_CACHE",
            std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-cache"),
        );
    command
}

pub fn stdout(command: &mut Command) -> String {
    String::from_utf8(command.assert().success().get_output().stdout.clone()).unwrap()
}

pub fn stderr(command: &mut Command, code: i32) -> String {
    String::from_utf8(command.assert().code(code).get_output().stderr.clone()).unwrap()
}

/// Compresses a model the way draw.io stores library entries.
pub fn compress(xml: &str) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(urlencoding::encode(xml).as_bytes())
        .unwrap();
    STANDARD.encode(encoder.finish().unwrap())
}

/// A library file of the given entries, escaped as draw.io writes it.
pub fn library_file(entries: serde_json::Value) -> String {
    let text = entries
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<mxlibrary>{text}</mxlibrary>")
}

pub fn library(name: &str, entries: serde_json::Value) -> Library {
    library::parse(
        LibraryName::parse(name).unwrap(),
        library_file(entries).as_bytes(),
    )
    .unwrap()
}

/// Libraries held in memory, as one repository.
pub struct Memory {
    pub origin: Origin,
    pub libraries: Vec<Library>,
}

impl LibraryRepository for Memory {
    fn origin(&self) -> Origin {
        self.origin
    }

    fn list(&self) -> Result<Vec<LibraryInfo>, Diagnostic> {
        Ok(self
            .libraries
            .iter()
            .map(|l| LibraryInfo {
                name: l.name.clone(),
                title: l.title.clone(),
                entries: l.entries.len(),
                location: None,
                normalized: true,
                version: None,
            })
            .collect())
    }

    fn load(&self, name: &LibraryName) -> Result<Option<Library>, Diagnostic> {
        Ok(self.libraries.iter().find(|l| &l.name == name).cloned())
    }
}

/// A cell as compared by meaning: element names, attributes as numbers where
/// they are numbers, and styles as sets of keys and named styles.
#[derive(Debug, PartialEq)]
pub enum Node {
    Element {
        name: String,
        attributes: BTreeMap<String, Value>,
        children: Vec<Node>,
    },
}

#[derive(Debug, PartialEq)]
pub enum Value {
    Number(String),
    Text(String),
    Style {
        named: Vec<String>,
        keys: BTreeMap<String, String>,
    },
}

fn value(key: &str, text: &str) -> Value {
    if key == "style" {
        let mut named = Vec::new();
        let mut keys = BTreeMap::new();
        for item in text.split(';').filter(|i| !i.is_empty()) {
            match item.split_once('=') {
                Some((k, v)) => {
                    keys.insert(k.to_owned(), v.to_owned());
                }
                None => named.push(item.to_owned()),
            }
        }
        named.sort();
        return Value::Style { named, keys };
    }
    match text.parse::<f64>() {
        Ok(number) if !text.is_empty() => Value::Number(format!("{number}")),
        _ => Value::Text(text.to_owned()),
    }
}

fn node(n: roxmltree::Node<'_, '_>) -> Node {
    Node::Element {
        name: n.tag_name().name().to_owned(),
        attributes: n
            .attributes()
            .map(|a| (a.name().to_owned(), value(a.name(), a.value())))
            .collect(),
        children: n.children().filter(|c| c.is_element()).map(node).collect(),
    }
}

/// The cells of every page, compared by meaning.
pub fn cells(xml: &str) -> Vec<Vec<Node>> {
    let doc = roxmltree::Document::parse(xml).unwrap();
    doc.descendants()
        .filter(|n| n.has_tag_name("root"))
        .map(|root| {
            root.children()
                .filter(|c| c.is_element())
                .map(node)
                .collect()
        })
        .collect()
}

#[track_caller]
pub fn assert_same_cells(actual: &str, expected: &str) {
    let (a, b) = (cells(actual), cells(expected));
    assert_eq!(a.len(), b.len(), "page count");
    for (page, (a, b)) in a.iter().zip(&b).enumerate() {
        for (index, (x, y)) in a.iter().zip(b).enumerate() {
            assert_eq!(x, y, "page {}, cell {index}", page + 1);
        }
        assert_eq!(a.len(), b.len(), "cells on page {}", page + 1);
    }
}

/// The element with this id and its geometry.
pub fn cell<'a>(doc: &'a roxmltree::Document<'a>, id: &str) -> roxmltree::Node<'a, 'a> {
    doc.descendants()
        .find(|n| n.attribute("id") == Some(id))
        .unwrap_or_else(|| panic!("no cell {id}"))
}

pub fn geometry<'a>(node: roxmltree::Node<'a, 'a>) -> roxmltree::Node<'a, 'a> {
    let inner = if node.has_tag_name("mxCell") {
        node
    } else {
        node.children().find(|n| n.has_tag_name("mxCell")).unwrap()
    };
    inner
        .children()
        .find(|n| n.has_tag_name("mxGeometry"))
        .unwrap()
}
