//! The library file format: `<mxlibrary>` in every form draw.io writes, and
//! the normalized form of spec 9.3, which draw.io and the VS Code extension
//! both read the same way.
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use crate::domain::{
    codec, diagram, image,
    library::{Entry, Library, unique},
    reference::{EntryId, LibraryName, is_name},
};

/// The fields of one entry in their document order.
type Fields = Vec<(String, Value)>;

/// A JSON object read with its keys in document order.
struct Ordered(Fields);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a library entry object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut fields: Vec<(String, Value)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    match fields.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = value,
                        None => fields.push((key, value)),
                    }
                }
                Ok(Ordered(fields))
            }
        }
        deserializer.deserialize_map(Fields)
    }
}

/// The title and the entries' fields of an `<mxlibrary>` file.
fn read(bytes: &[u8]) -> Result<(Option<String>, Vec<Fields>)> {
    let text = diagram::utf8(bytes)?;
    let doc = diagram::parse(text)?;
    let root = doc.root_element();
    ensure!(
        root.has_tag_name("mxlibrary"),
        "library root must be <mxlibrary>"
    );
    let title = root
        .attribute("title")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    let json: String = root.children().filter_map(|n| n.text()).collect();
    let items: Vec<Ordered> = serde_json::from_str(json.trim()).context("invalid library JSON")?;
    Ok((title, items.into_iter().map(|o| o.0).collect()))
}

/// Entries for the fields of a library: a valid, unused `id` field is kept;
/// the other entries take the ID `hints` gives their title, if unused, or
/// else one derived from the title.
fn entries(items: Vec<Fields>, hints: &HashMap<String, EntryId>) -> Result<Vec<Entry>> {
    let field = |fields: &Fields, key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_str().map(str::to_owned))
    };
    let mut used = HashSet::new();
    let explicit: Vec<Option<EntryId>> = items
        .iter()
        .map(|fields| {
            field(fields, "id")
                .filter(|id| EntryId::is_valid(id) && used.insert(id.clone()))
                .map(|id| EntryId::parse(&id).expect("valid ID"))
        })
        .collect();
    items
        .into_iter()
        .zip(explicit)
        .enumerate()
        .map(|(index, (fields, id))| {
            let title = field(&fields, "title").unwrap_or_default();
            let id = id
                .or_else(|| {
                    hints
                        .get(&title)
                        .filter(|hint| used.insert(hint.to_string()))
                        .cloned()
                })
                .unwrap_or_else(|| unique(EntryId::derive(&title), &mut used));
            Entry::from_fields(id, fields).with_context(|| format!("library entry {}", index + 1))
        })
        .collect()
}

/// Reads an `<mxlibrary>` file in any form draw.io writes (JSON as text,
/// CDATA or entity-escaped; compressed or plain models).
pub fn parse(name: LibraryName, bytes: &[u8]) -> Result<Library> {
    let (title, items) = read(bytes)?;
    Ok(Library {
        name,
        title,
        entries: entries(items, &HashMap::new())?,
    })
}

/// A file given to `library add` or `library pack`.
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    /// The file's name, for titles and the default library name.
    pub file_name: Option<&'a str>,
    pub bytes: &'a [u8],
}

/// A file name without its extension.
fn stem(file_name: &str) -> &str {
    let name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// The library name a file name suggests (spec 9.11): its stem in lowercase
/// with runs of anything but a-z and 0-9 turned into '-'.
pub fn name_from_file(file_name: &str) -> Option<LibraryName> {
    let mut name = String::new();
    let mut gap = false;
    for c in stem(file_name).chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !name.is_empty() {
                name.push('-');
            }
            gap = false;
            name.push(c);
        } else {
            gap = true;
        }
    }
    name.truncate(63);
    let name = name.trim_end_matches('-');
    is_name(name)
        .then(|| LibraryName::new(None, name).ok())
        .flatten()
}

/// A number as JSON, without a fraction when it has none.
fn number(value: f64) -> Value {
    if value.fract() == 0.0 && (0.0..1e15).contains(&value) {
        Value::from(value as u64)
    } else {
        Value::from(value)
    }
}

/// The entries of `library add` and `library pack` sources: the entries of
/// `<mxlibrary>` files, and one `data` entry per image file. IDs are unique
/// across all sources. The title is the first library file's title, if any.
pub fn read_sources(sources: &[Source<'_>]) -> Result<(Option<String>, Vec<Entry>)> {
    read_sources_after(sources, None)
}

/// Like `read_sources`, keeping the IDs a previous version of the library
/// gave to entries of the same title (`library pack`).
pub fn read_sources_after(
    sources: &[Source<'_>],
    previous: Option<&Library>,
) -> Result<(Option<String>, Vec<Entry>)> {
    let mut title = None;
    let mut items = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let label = source
            .file_name
            .map_or_else(|| format!("source {}", index + 1), str::to_owned);
        if let Some(found) = image::inspect(source.bytes) {
            let data = format!(
                "data:{};base64,{}",
                found.media_type,
                STANDARD.encode(source.bytes)
            );
            items.push(vec![
                (
                    "title".to_owned(),
                    Value::from(source.file_name.map(stem).unwrap_or("")),
                ),
                ("w".to_owned(), number(found.width)),
                ("h".to_owned(), number(found.height)),
                ("aspect".to_owned(), Value::from("fixed")),
                ("data".to_owned(), Value::from(data)),
            ]);
            continue;
        }
        let looks_like_image = source.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"]
                .iter()
                .any(|e| lower.ends_with(e))
        });
        if looks_like_image {
            bail!("{label}: cannot read the image's size");
        }
        let (found_title, found) =
            read(source.bytes).with_context(|| format!("{label}: not a library"))?;
        title = title.or(found_title);
        items.extend(found);
    }
    let hints = previous
        .map(|library| {
            library
                .entries
                .iter()
                .map(|e| (e.title.clone(), e.id.clone()))
                .collect()
        })
        .unwrap_or_default();
    Ok((title, entries(items, &hints)?))
}

/// Writes the normalized form (spec 9.3): `<mxlibrary title>` around a JSON
/// array, one entry per line, `id` first, and `<`, `>` and `&` escaped in
/// JSON so that no XML entity or CDATA is needed.
pub fn serialize(library: &Library) -> String {
    const ORDER: [&str; 4] = ["title", "w", "h", "aspect"];
    let lines: Vec<String> = library
        .entries
        .iter()
        .map(|entry| {
            let fields = entry.fields();
            let get = |key: &str| {
                fields
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
            };
            let mut out: Vec<(String, Value)> = vec![("id".into(), Value::from(entry.id.as_str()))];
            for key in ORDER {
                let value = match key {
                    "title" => {
                        Some(get("title").unwrap_or_else(|| Value::from(entry.title.as_str())))
                    }
                    "w" if fields.is_empty() => Some(number(entry.width)),
                    "h" if fields.is_empty() => Some(number(entry.height)),
                    _ => get(key),
                };
                if let Some(value) = value {
                    out.push((key.into(), value));
                }
            }
            match (get("xml"), get("data")) {
                (Some(xml), _) => out.push(("xml".into(), xml)),
                (None, Some(data)) => out.push(("data".into(), data)),
                (None, None) => out.push((
                    "xml".into(),
                    Value::from(entry.graph_model().unwrap_or_default()),
                )),
            }
            for (key, value) in fields {
                if !matches!(
                    key.as_str(),
                    "id" | "title" | "w" | "h" | "aspect" | "xml" | "data"
                ) {
                    out.push((key.clone(), value.clone()));
                }
            }
            let members: Vec<String> = out
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).expect("string"),
                        serde_json::to_string(v).expect("JSON value")
                    )
                })
                .collect();
            format!("{{{}}}", members.join(","))
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
                .replace('&', "\\u0026")
        })
        .collect();
    let open = match &library.title {
        Some(title) => format!("<mxlibrary title=\"{}\">", codec::attribute(title)),
        None => "<mxlibrary>".to_owned(),
    };
    if lines.is_empty() {
        format!("{open}[\n]</mxlibrary>\n")
    } else {
        format!("{open}[\n{}\n]</mxlibrary>\n", lines.join(",\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_names_come_from_file_names() {
        for (file, name) in [
            ("simple-icons.xml", Some("simple-icons")),
            ("Simple Icons.xml", Some("simple-icons")),
            ("dir/My_Lib (2).XML", Some("my-lib-2")),
            ("Logo.svg", Some("logo")),
            ("図形集.xml", None),
            ("...", None),
        ] {
            assert_eq!(
                name_from_file(file).map(|n| n.to_string()).as_deref(),
                name,
                "{file}"
            );
        }
    }

    #[test]
    fn normalized_files_keep_unknown_fields_in_order() {
        let source = br#"<mxlibrary title="T &amp; U"><![CDATA[[{"zz":1,"xml":"<mxGraphModel/>","title":"A & B","w":10,"h":2.5,"aa":[true]}]]]></mxlibrary>"#;
        let library = parse(LibraryName::parse("lib").unwrap(), source).unwrap();
        let text = serialize(&library);
        assert_eq!(
            text,
            "<mxlibrary title=\"T &amp; U\">[\n{\"id\":\"a-b\",\"title\":\"A \\u0026 B\",\"w\":10,\"h\":2.5,\"xml\":\"\\u003cmxGraphModel/\\u003e\",\"zz\":1,\"aa\":[true]}\n]</mxlibrary>\n"
        );
        // Normalizing is idempotent and reads back the same entries.
        let again = parse(LibraryName::parse("lib").unwrap(), text.as_bytes()).unwrap();
        assert_eq!(serialize(&again), text);
        assert_eq!(again.entries[0].title, "A & B");
    }
}
