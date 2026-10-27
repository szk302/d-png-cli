//! VS Code's `.vscode/settings.json` (JSON with comments and trailing
//! commas): the extension's `customLibraries` setting is rewritten in place,
//! keeping the other settings, comments and the user's own items.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{ops::Range, path::PathBuf};

use crate::{
    adapters::fs,
    app::{
        ports::EditorSettings,
        vscode::{CustomLibrary, EntryIds},
    },
    domain::diagnostic::{Code, Diagnostic},
};

/// The setting of `hediet.vscode-drawio` that lists custom libraries.
pub const KEY: &str = "hediet.vscode-drawio.customLibraries";

/// An object member: its key, the key's range and the value's range.
type Member = (String, Range<usize>, Range<usize>);

/// A JSONC reader that finds where values are, without building them.
struct Scanner<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, at: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    /// Skips whitespace and `//` and `/* */` comments.
    fn space(&mut self) -> Result<()> {
        loop {
            let rest = &self.text[self.at..];
            if let Some(c) = rest.chars().next().filter(|c| c.is_whitespace()) {
                self.at += c.len_utf8();
            } else if rest.starts_with("//") {
                self.at += rest.find('\n').unwrap_or(rest.len());
            } else if let Some(comment) = rest.strip_prefix("/*") {
                let end = comment.find("*/").context("unterminated comment")?;
                self.at += end + 4;
            } else {
                return Ok(());
            }
        }
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        ensure!(
            self.peek() == Some(byte),
            "expected '{}' at byte {}",
            byte as char,
            self.at
        );
        self.at += 1;
        Ok(())
    }

    /// A string's range, quotes included.
    fn string(&mut self) -> Result<Range<usize>> {
        let start = self.at;
        self.expect(b'"')?;
        let bytes = self.text.as_bytes();
        while let Some(&b) = bytes.get(self.at) {
            self.at += 1;
            match b {
                b'\\' => self.at += 1,
                b'"' => return Ok(start..self.at),
                _ => {}
            }
        }
        bail!("unterminated string")
    }

    /// The range of the value here: a string, number, literal, object or
    /// array, comments and trailing commas allowed inside.
    fn value(&mut self) -> Result<Range<usize>> {
        let start = self.at;
        match self.peek().context("expected a value")? {
            b'"' => {
                self.string()?;
            }
            b'{' => {
                self.at += 1;
                self.members()?;
            }
            b'[' => {
                self.at += 1;
                self.elements()?;
            }
            _ => {
                while self
                    .peek()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
                {
                    self.at += 1;
                }
                ensure!(self.at > start, "unexpected character at byte {start}");
            }
        }
        Ok(start..self.at)
    }

    /// The members of an object whose `{` was read: each key's text, its
    /// range and its value's range. Ends after `}`.
    fn members(&mut self) -> Result<Vec<Member>> {
        let mut members = Vec::new();
        loop {
            self.space()?;
            if self.peek() == Some(b'}') {
                self.at += 1;
                return Ok(members);
            }
            let key = self.string()?;
            let name: String = serde_json::from_str(&self.text[key.clone()])?;
            self.space()?;
            self.expect(b':')?;
            self.space()?;
            let value = self.value()?;
            members.push((name, key, value));
            self.space()?;
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {}
                _ => bail!("expected ',' or '}}' at byte {}", self.at),
            }
        }
    }

    /// The ranges of an array's elements; its `[` was read. Ends after `]`.
    fn elements(&mut self) -> Result<Vec<Range<usize>>> {
        let mut elements = Vec::new();
        loop {
            self.space()?;
            if self.peek() == Some(b']') {
                self.at += 1;
                return Ok(elements);
            }
            elements.push(self.value()?);
            self.space()?;
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {}
                _ => bail!("expected ',' or ']' at byte {}", self.at),
            }
        }
    }
}

/// JSONC text as JSON: comments and trailing commas removed.
fn plain(text: &str) -> Result<Value> {
    let mut out = String::new();
    let mut scanner = Scanner::new(text);
    while scanner.at < text.len() {
        let before = scanner.at;
        scanner.space()?;
        if scanner.at > before {
            out.push(' ');
            continue;
        }
        match scanner.peek() {
            Some(b'"') => {
                let range = scanner.string()?;
                out.push_str(&text[range]);
            }
            Some(b',') => {
                scanner.at += 1;
                let mut ahead = Scanner {
                    text,
                    at: scanner.at,
                };
                ahead.space()?;
                if !matches!(ahead.peek(), Some(b'}' | b']')) {
                    out.push(',');
                }
            }
            Some(_) => {
                let c = text[scanner.at..].chars().next().expect("a character");
                out.push(c);
                scanner.at += c.len_utf8();
            }
            None => break,
        }
    }
    Ok(serde_json::from_str(&out)?)
}

/// The indentation of the line holding byte `at`.
fn indentation(text: &str, at: usize) -> &str {
    let line = text[..at].rfind('\n').map_or(0, |n| n + 1);
    let rest = &text[line..at];
    &rest[..rest.len() - rest.trim_start().len()]
}

fn item(library: &CustomLibrary, indent: &str, unit: &str) -> String {
    let text = |s: &str| serde_json::to_string(s).expect("string");
    format!(
        "{indent}{{\n{indent}{unit}\"entryId\": {},\n{indent}{unit}\"libName\": {},\n{indent}{unit}\"file\": {}\n{indent}}}",
        text(&library.entry_id),
        text(&library.lib_name),
        text(&library.file)
    )
}

/// The items as the setting's value, laid out as `--write` writes them.
pub fn value(items: &[CustomLibrary]) -> String {
    if items.is_empty() {
        return "[]\n".to_owned();
    }
    let lines: Vec<String> = items.iter().map(|i| item(i, "  ", "  ")).collect();
    format!("[\n{}\n]\n", lines.join(",\n"))
}

/// The settings text with `customLibraries` holding the user's items (those
/// `ids` does not manage), as they were written, and then `items`.
pub fn update(text: &str, ids: &EntryIds, items: &[CustomLibrary]) -> Result<String> {
    let text = if text.trim().is_empty() { "{}\n" } else { text };
    let mut scanner = Scanner::new(text);
    scanner.space()?;
    let open = scanner.at;
    scanner.expect(b'{')?;
    let members = scanner.members()?;
    let close = scanner.at - 1;
    scanner.space()?;
    ensure!(
        scanner.at == text.len(),
        "unexpected text after the settings"
    );
    let found = members.iter().find(|(name, _, _)| name == KEY);
    let (indent, unit) = match found.or(members.first()) {
        Some((_, key, _)) => {
            let indent = indentation(text, key.start).to_owned();
            let unit = if indent.is_empty() {
                "  ".to_owned()
            } else {
                indent.clone()
            };
            (indent, unit)
        }
        None => ("  ".to_owned(), "  ".to_owned()),
    };
    let inner = format!("{indent}{unit}");
    // The user's items stay as they were written.
    let mut lines: Vec<String> = Vec::new();
    if let Some((_, _, value)) = found {
        let mut array = Scanner::new(text);
        array.at = value.start;
        ensure!(array.peek() == Some(b'['), "{KEY} must be an array");
        array.at += 1;
        for element in array.elements()? {
            let entry: Value = plain(&text[element.clone()])?;
            let entry_id = entry["entryId"].as_str().unwrap_or("");
            if !ids.manages(entry_id) {
                lines.push(format!("{inner}{}", &text[element]));
            }
        }
    }
    lines.extend(items.iter().map(|library| item(library, &inner, &unit)));
    let array = if lines.is_empty() {
        "[]".to_owned()
    } else {
        format!("[\n{}\n{indent}]", lines.join(",\n"))
    };
    let mut out = text.to_owned();
    match found {
        Some((_, _, value)) => out.replace_range(value.clone(), &array),
        None => match members.last() {
            Some((_, _, value)) => {
                let mut after = Scanner::new(text);
                after.at = value.end;
                after.space()?;
                let entry = format!("\n{indent}\"{KEY}\": {array}");
                if after.peek() == Some(b',') {
                    out.insert_str(after.at + 1, &entry);
                } else {
                    out.insert_str(value.end, &format!(",{entry}"));
                }
            }
            None => {
                out.replace_range(
                    open..close + 1,
                    &format!("{{\n{indent}\"{KEY}\": {array}\n}}"),
                );
            }
        },
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    // What was written must read back.
    plain(&out).context("the result is not valid JSON with comments")?;
    Ok(out)
}

/// `.vscode/settings.json` of a workspace.
pub struct VsCodeSettings {
    pub path: PathBuf,
}

impl VsCodeSettings {
    /// Whether the settings already name this library file.
    pub fn lists(&self, file: &str) -> bool {
        let Ok(bytes) = fs::read(&self.path) else {
            return false;
        };
        let text = String::from_utf8_lossy(&bytes);
        plain(&text).is_ok_and(|value| {
            value[KEY]
                .as_array()
                .is_some_and(|items| items.iter().any(|i| i["file"] == file))
        })
    }
}

impl EditorSettings for VsCodeSettings {
    fn write_custom_libraries(
        &self,
        ids: &EntryIds,
        items: &[CustomLibrary],
    ) -> Result<(), Diagnostic> {
        let text = if self.path.is_file() {
            let bytes =
                fs::read(&self.path).map_err(|e| Diagnostic::from_error(Code::InputRead, &e))?;
            String::from_utf8(bytes).map_err(|_| {
                Diagnostic::new(
                    Code::ConfigInvalid,
                    format!("{}: not UTF-8", self.path.display()),
                )
            })?
        } else {
            String::new()
        };
        let updated = update(&text, ids, items).map_err(|e| {
            Diagnostic::new(
                Code::ConfigInvalid,
                format!("{}: {e:#}; nothing was written", self.path.display()),
            )
        })?;
        let write = || -> Result<()> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            fs::atomic_write(&self.path, updated.as_bytes())
        };
        write().map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(entry_id: &str, name: &str) -> CustomLibrary {
        CustomLibrary {
            entry_id: entry_id.into(),
            lib_name: name.into(),
            file: format!("${{workspaceFolder}}/.drawio-png/libraries/{name}.xml"),
        }
    }

    fn ids() -> EntryIds {
        EntryIds::new(None).unwrap()
    }

    #[test]
    fn a_new_file_gets_the_setting() {
        let out = update("", &ids(), &[library("drawio-png-cli", "icons")]).unwrap();
        assert_eq!(
            out,
            "{\n  \"hediet.vscode-drawio.customLibraries\": [\n    {\n      \"entryId\": \"drawio-png-cli\",\n      \"libName\": \"icons\",\n      \"file\": \"${workspaceFolder}/.drawio-png/libraries/icons.xml\"\n    }\n  ]\n}\n"
        );
    }

    #[test]
    fn other_settings_comments_and_items_are_kept() {
        let text = r#"{
    // Editor settings
    "editor.tabSize": 4, /* keep */
    "hediet.vscode-drawio.customLibraries": [
        { "entryId": "mine", "libName": "Mine", "file": "/x.xml" }, // my own
        { "entryId": "drawio-png-cli", "libName": "old", "file": "old.xml" },
        { "entryId": "drawio-png-cli @acme", "libName": "gone", "file": "gone.xml" },
    ],
    "files.eol": "\n",
}
"#;
        let out = update(text, &ids(), &[library("drawio-png-cli", "new")]).unwrap();
        assert!(
            out.starts_with("{\n    // Editor settings\n    \"editor.tabSize\": 4, /* keep */\n"),
            "{out}"
        );
        assert!(out.contains(r#"{ "entryId": "mine", "libName": "Mine", "file": "/x.xml" }"#));
        assert!(!out.contains("old.xml") && !out.contains("gone.xml"));
        assert!(out.contains("            \"libName\": \"new\","), "{out}");
        assert!(out.ends_with("    \"files.eol\": \"\\n\",\n}\n"), "{out}");
        let value = plain(&out).unwrap();
        assert_eq!(value[KEY].as_array().unwrap().len(), 2);
        // Writing again changes nothing.
        assert_eq!(
            update(&out, &ids(), &[library("drawio-png-cli", "new")]).unwrap(),
            out
        );
    }

    #[test]
    fn the_setting_is_added_to_existing_settings() {
        for text in ["{\"a\": 1}", "{\n  \"a\": 1,\n}\n", "{}"] {
            let out = update(text, &ids(), &[library("drawio-png-cli", "icons")]).unwrap();
            let value = plain(&out).unwrap();
            assert_eq!(value[KEY][0]["libName"], "icons", "{text}");
            if text != "{}" {
                assert_eq!(value["a"], 1, "{text}");
            }
        }
        // With nothing to write, the array is emptied of managed items.
        let out = update("{\"a\": 1}", &ids(), &[]).unwrap();
        assert_eq!(plain(&out).unwrap()[KEY], serde_json::json!([]));
    }

    #[test]
    fn broken_settings_are_not_touched() {
        for text in [
            "{ \"a\": }",
            "[1]",
            "{\"hediet.vscode-drawio.customLibraries\": {}}",
            "{ /* open",
            "{\"a\": 1} extra",
        ] {
            assert!(update(text, &ids(), &[]).is_err(), "{text}");
        }
    }
}
