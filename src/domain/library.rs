//! draw.io custom libraries (`<mxlibrary>`): their entries, the IDs that
//! references use, and the kinds of entry that decide what a reference may
//! override (spec 9.2, 9.4).
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use roxmltree::Node;
use serde_json::Value;
use std::{collections::HashSet, fmt};

use crate::domain::{
    codec::{self, inflate},
    diagram,
    reference::{EntryId, LibraryName},
};

/// Where a library comes from (spec 9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Registered into the project with `library add`; no scope.
    Project,
    /// Installed from the source of its scope.
    Installed,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Project => "project",
            Origin::Installed => "installed",
        }
    }
}

impl LibraryName {
    /// The origin a name belongs to: unscoped names are the project's,
    /// scoped ones are installed.
    pub fn origin(&self) -> Origin {
        match self.scope() {
            None => Origin::Project,
            Some(_) => Origin::Installed,
        }
    }
}

/// What an entry is, which decides how a reference may use it (spec 9.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// One vertex showing embedded image data.
    Image,
    /// One vertex of any other kind.
    Shape,
    /// One top-level edge, possibly with label children.
    Edge,
    /// One top-level vertex with descendants.
    Group,
    /// Several top-level cells.
    Set,
    /// Data that cannot be expanded.
    Invalid,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::Image => "image",
            EntryKind::Shape => "shape",
            EntryKind::Edge => "edge",
            EntryKind::Group => "group",
            EntryKind::Set => "set",
            EntryKind::Invalid => "invalid",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "image" => EntryKind::Image,
            "shape" => EntryKind::Shape,
            "edge" => EntryKind::Edge,
            "group" => EntryKind::Group,
            "set" => EntryKind::Set,
            "invalid" => EntryKind::Invalid,
            _ => return None,
        })
    }

    /// Whether a reference may set the width and height.
    pub fn resizable(self) -> bool {
        matches!(self, EntryKind::Image | EntryKind::Shape)
    }
}

impl fmt::Display for EntryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One library entry, as the editor's sidebar shows it.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: EntryId,
    pub title: String,
    pub width: f64,
    pub height: f64,
    content: Content,
    /// The entry's fields as read from its library file, in their order,
    /// so that a normalized file keeps what it does not interpret.
    fields: Vec<(String, Value)>,
}

#[derive(Clone, Debug)]
enum Content {
    /// An `mxGraphModel`, possibly compressed as draw.io stores it.
    Model(String),
    /// An image data URI or URL.
    Image { data: String, aspect: bool },
}

impl Entry {
    /// An entry holding an uncompressed `mxGraphModel`.
    pub fn model(id: EntryId, title: String, width: f64, height: f64, xml: String) -> Self {
        Entry {
            id,
            title,
            width,
            height,
            content: Content::Model(xml),
            fields: Vec::new(),
        }
    }

    /// An entry from the fields of a library file: `xml` or `data`, `title`,
    /// `w`, `h` and `aspect`.
    pub(crate) fn from_fields(id: EntryId, fields: Vec<(String, Value)>) -> Result<Self> {
        let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let text = |key: &str| get(key).and_then(Value::as_str);
        let number = |key: &str| get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let content = if let Some(xml) = text("xml") {
            Content::Model(xml.to_owned())
        } else if let Some(data) = text("data") {
            Content::Image {
                data: data.to_owned(),
                aspect: text("aspect") != Some("variable"),
            }
        } else {
            bail!("the entry has neither xml nor data");
        };
        Ok(Entry {
            id,
            title: text("title").unwrap_or("").to_owned(),
            width: number("w"),
            height: number("h"),
            content,
            fields,
        })
    }

    /// The fields of its library file, in their order.
    pub(crate) fn fields(&self) -> &[(String, Value)] {
        &self.fields
    }

    /// Why the entry cannot be registered: data that cannot be expanded, or
    /// references, which are never expanded inside an entry.
    pub fn problem(&self) -> Option<String> {
        if let Err(e) = self.try_kind() {
            return Some(format!("{e:#}"));
        }
        let model = self.graph_model().ok()?;
        (model.contains("libref=") || model.contains("image=lib:"))
            .then(|| "an entry cannot hold library references".to_owned())
    }

    /// The entry as an uncompressed `mxGraphModel`. A `data` entry becomes
    /// the image cell draw.io's sidebar adds for it.
    pub fn graph_model(&self) -> Result<String> {
        match &self.content {
            Content::Model(xml) if xml.trim_start().starts_with('<') => Ok(xml.clone()),
            Content::Model(xml) => {
                let bytes = STANDARD
                    .decode(xml.trim())
                    .context("invalid library entry Base64")?;
                let inflated =
                    inflate(&bytes, false).context("invalid compressed library entry")?;
                diagram::percent_decode(diagram::utf8(&inflated)?)
            }
            Content::Image { aspect, .. } => {
                let data = self.data_image().expect("image content");
                let aspect = if *aspect { "aspect=fixed;" } else { "" };
                Ok(format!(
                    r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="" style="shape=image;verticalLabelPosition=bottom;verticalAlign=top;imageAspect=0;{aspect}image={}" vertex="1" parent="1"><mxGeometry width="{}" height="{}" as="geometry"/></mxCell></root></mxGraphModel>"#,
                    codec::attribute(&data?),
                    codec::number(self.width),
                    codec::number(self.height)
                ))
            }
        }
    }

    /// The image value of a `data` entry as draw.io's sidebar converts it
    /// (`convertDataUri`): a style value cannot contain ';', so the text from
    /// the first ';' to the next ',' goes. `None` for model entries.
    fn data_image(&self) -> Option<Result<String>> {
        let Content::Image { data, .. } = &self.content else {
            return None;
        };
        Some((|| {
            let converted = if data.starts_with("data:") {
                match (data.find(';'), data.find(',')) {
                    (Some(semicolon), Some(comma)) if semicolon < comma => {
                        format!("{}{}", &data[..semicolon], &data[comma..])
                    }
                    _ => data.clone(),
                }
            } else {
                data.clone()
            };
            // Libraries may also link images; rendering those needs --allow-network.
            let source = converted.starts_with("data:image/")
                || converted.starts_with("https://")
                || converted.starts_with("http://");
            ensure!(
                source && !converted.contains(';'),
                "unsupported library image data"
            );
            Ok(converted)
        })())
    }

    /// The kind of the entry; `Invalid` when its data cannot be read.
    pub fn kind(&self) -> EntryKind {
        self.try_kind().unwrap_or(EntryKind::Invalid)
    }

    fn try_kind(&self) -> Result<EntryKind> {
        if let Some(data) = self.data_image() {
            let data = data?;
            return Ok(if data.starts_with("data:") {
                EntryKind::Image
            } else {
                EntryKind::Shape
            });
        }
        let model = self.graph_model()?;
        let source = diagram::parse(&model).context("invalid library entry")?;
        let cells = Cells::of(source.root_element())?;
        let top = cells.top();
        Ok(match top.as_slice() {
            [] => bail!("library entry has no cells"),
            [_, _, ..] => EntryKind::Set,
            [cell] if inner(*cell).attribute("edge") == Some("1") => EntryKind::Edge,
            [_] if cells.content.len() > 1 => EntryKind::Group,
            [cell] => {
                let style = inner(*cell).attribute("style").unwrap_or("");
                if style_value(style, "image").is_some_and(|v| v.starts_with("data:")) {
                    EntryKind::Image
                } else {
                    EntryKind::Shape
                }
            }
        })
    }

    /// The image value an `image` entry shows: the `image` style value of its
    /// cell, or the converted data URI of a `data` entry.
    pub fn image(&self) -> Result<Option<String>> {
        if let Some(data) = self.data_image() {
            let data = data?;
            return Ok(data.starts_with("data:").then_some(data));
        }
        if self.kind() != EntryKind::Image {
            return Ok(None);
        }
        Ok(self
            .cell_style()?
            .and_then(|style| style_value(&style, "image").map(str::to_owned)))
    }

    /// Whether the entry is a single vertex that a preview can shrink, and the
    /// size of its top-level cells' bounding box (vertices and edge points, as
    /// references align them), never less than the declared size of a multi-cell entry.
    pub fn extent(&self) -> Result<(bool, f64, f64)> {
        if matches!(self.content, Content::Image { .. }) {
            return Ok((true, self.width, self.height));
        }
        let model = self.graph_model()?;
        let source = diagram::parse(&model).context("invalid library entry")?;
        let cells = Cells::of(source.root_element())?;
        let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
        let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut include = |x: f64, y: f64, width: f64, height: f64| {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + width);
            bottom = bottom.max(y + height);
        };
        let mut vertices = 0;
        for cell in cells.top() {
            let Some(geometry) = geometry(cell) else {
                continue;
            };
            if inner(cell).attribute("edge") == Some("1") {
                for point in geometry
                    .descendants()
                    .filter(|n| n.has_tag_name("mxPoint") && n.attribute("as") != Some("offset"))
                {
                    include(number(point, "x"), number(point, "y"), 0.0, 0.0);
                }
            } else if geometry.attribute("relative") != Some("1") {
                vertices += 1;
                let (x, y) = (number(geometry, "x"), number(geometry, "y"));
                include(x, y, number(geometry, "width"), number(geometry, "height"));
            }
        }
        if right < left {
            return Ok((false, self.width, self.height));
        }
        let (width, height) = (right - left, bottom - top);
        Ok(if cells.content.len() == 1 && vertices == 1 {
            (true, width, height)
        } else {
            (false, width.max(self.width), height.max(self.height))
        })
    }

    /// The style of an entry made of one cell, image data included; `None`
    /// for multi-cell entries and `data` entries.
    pub fn cell_style(&self) -> Result<Option<String>> {
        if matches!(self.content, Content::Image { .. }) {
            return Ok(None);
        }
        let model = self.graph_model()?;
        let source = diagram::parse(&model).context("invalid library entry")?;
        let cells = Cells::of(source.root_element())?;
        Ok(match cells.content.as_slice() {
            [cell] => Some(inner(*cell).attribute("style").unwrap_or("").to_owned()),
            _ => None,
        })
    }
}

/// One cell an expansion creates, as `library show` lists it. IDs are
/// patterns: `<ID>` is the placeholder's ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellInfo {
    pub id: String,
    pub edge: bool,
    pub label: String,
    pub parent: Option<String>,
}

/// What `library show` tells about an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description {
    /// The label of the single top-level cell.
    pub label: Option<String>,
    /// The style of the single top-level cell.
    pub style: Option<String>,
    /// The custom data on the wrapper of the single top-level cell.
    pub data: Vec<(String, String)>,
    pub cells: Vec<CellInfo>,
}

/// An image value's media type and decoded size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub media_type: String,
    pub bytes: usize,
}

impl ImageInfo {
    /// Reads `data:<type>,<payload>`; the payload is Base64 after draw.io's
    /// conversion, or percent-encoded text.
    pub fn of(data: &str) -> Option<Self> {
        let (head, payload) = data.strip_prefix("data:")?.split_once(',')?;
        let media_type = head.split(';').next().unwrap_or("").to_owned();
        let bytes = STANDARD
            .decode(payload)
            .map(|b| b.len())
            .or_else(|_| diagram::percent_decode(payload).map(|t| t.len()))
            .unwrap_or(payload.len());
        Some(Self { media_type, bytes })
    }
}

impl Entry {
    /// The cells a reference to this entry creates, with the IDs they get.
    pub fn describe(&self) -> Result<Description> {
        let model = self.graph_model()?;
        let source = diagram::parse(&model).context("invalid library entry")?;
        let cells = Cells::of(source.root_element())?;
        let top = cells.top();
        let set = top.len() > 1;
        let mut ids = std::collections::HashMap::new();
        let mut next = 1;
        for cell in &cells.content {
            let id = if !set && *cell == top[0] {
                "<ID>".to_owned()
            } else {
                next += 1;
                format!("<ID>-{}", next - 1)
            };
            ids.insert(cells.id(*cell), id);
        }
        let label_of = |cell: Node<'_, '_>| {
            if cell.has_tag_name("mxCell") {
                cell.attribute("value")
            } else {
                cell.attribute("label")
            }
            .unwrap_or("")
            .to_owned()
        };
        let infos = cells
            .content
            .iter()
            .map(|cell| CellInfo {
                id: ids[&cells.id(*cell)].clone(),
                edge: inner(*cell).attribute("edge") == Some("1"),
                label: label_of(*cell),
                parent: inner(*cell)
                    .attribute("parent")
                    .and_then(|p| ids.get(p))
                    .cloned(),
            })
            .collect();
        let single = (!set).then(|| top[0]);
        Ok(Description {
            label: single.map(label_of),
            style: single.map(|c| inner(c).attribute("style").unwrap_or("").to_owned()),
            data: single
                .filter(|c| !c.has_tag_name("mxCell"))
                .map(|c| {
                    c.attributes()
                        .filter(|a| a.name() != "id" && a.name() != "label")
                        .map(|a| (a.name().to_owned(), a.value().to_owned()))
                        .collect()
                })
                .unwrap_or_default(),
            cells: infos,
        })
    }
}

/// A library: its name, the optional `<mxlibrary title>` that draw.io shows,
/// and its entries.
#[derive(Clone, Debug)]
pub struct Library {
    pub name: LibraryName,
    pub title: Option<String>,
    pub entries: Vec<Entry>,
}

impl Library {
    pub fn entry(&self, id: &EntryId) -> Option<&Entry> {
        self.entries.iter().find(|e| &e.id == id)
    }
}

pub use crate::domain::library_file::parse;

/// `base`, or `base-2`, `base-3`… whichever is unused first; marks it used.
pub fn unique(base: EntryId, used: &mut HashSet<String>) -> EntryId {
    if used.insert(base.to_string()) {
        return base;
    }
    (2..)
        .map(|n| base.numbered(n))
        .find(|id| used.insert(id.to_string()))
        .expect("an unused number")
}

/// The value of `key` in a draw.io style; the last one wins, as in draw.io.
pub fn style_value<'s>(style: &'s str, key: &str) -> Option<&'s str> {
    style
        .split(';')
        .filter_map(|item| item.split_once('='))
        .filter(|(k, _)| *k == key)
        .map(|(_, v)| v)
        .next_back()
}

/// A graph cell: an `mxCell` outside a wrapper, or an `object`/`UserObject`
/// wrapper holding one.
pub(crate) fn is_cell(node: Node<'_, '_>) -> bool {
    node.has_tag_name("mxCell")
        && !node
            .parent_element()
            .is_some_and(|p| p.has_tag_name("object") || p.has_tag_name("UserObject"))
        || node.has_tag_name("object")
        || node.has_tag_name("UserObject")
}

/// The mxCell carrying parent/source/target: the node or its child.
pub(crate) fn inner<'a, 'i>(node: Node<'a, 'i>) -> Node<'a, 'i> {
    if node.has_tag_name("mxCell") {
        node
    } else {
        node.children()
            .find(|n| n.has_tag_name("mxCell"))
            .unwrap_or(node)
    }
}

pub(crate) fn geometry<'a, 'i>(cell: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    inner(cell)
        .children()
        .find(|n| n.has_tag_name("mxGeometry"))
}

pub(crate) fn number(node: Node<'_, '_>, name: &str) -> f64 {
    node.attribute(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// A library entry's cells, excluding its root cell and layers.
pub(crate) struct Cells<'a, 'i> {
    pub(crate) content: Vec<Node<'a, 'i>>,
    layers: HashSet<String>,
}

impl<'a, 'i> Cells<'a, 'i> {
    pub(crate) fn of(model: Node<'a, 'i>) -> Result<Self> {
        ensure!(
            model.has_tag_name("mxGraphModel"),
            "library entry root must be <mxGraphModel>"
        );
        let root = model
            .children()
            .find(|n| n.has_tag_name("root"))
            .context("library entry has no <root>")?;
        let all: Vec<_> = root.descendants().filter(|n| is_cell(*n)).collect();
        let roots: HashSet<String> = all
            .iter()
            .filter(|c| inner(**c).attribute("parent").is_none())
            .map(|c| c.attribute("id").unwrap_or("").to_owned())
            .collect();
        let layers: HashSet<String> = all
            .iter()
            .filter(|c| {
                inner(**c)
                    .attribute("parent")
                    .is_some_and(|p| roots.contains(p))
            })
            .map(|c| c.attribute("id").unwrap_or("").to_owned())
            .collect();
        let content = all
            .into_iter()
            .filter(|c| {
                let id = c.attribute("id").unwrap_or("");
                !roots.contains(id) && !layers.contains(id)
            })
            .collect();
        Ok(Self { content, layers })
    }

    pub(crate) fn id(&self, cell: Node<'_, '_>) -> String {
        cell.attribute("id").unwrap_or("").to_owned()
    }

    pub(crate) fn is_top(&self, cell: Node<'_, '_>) -> bool {
        inner(cell)
            .attribute("parent")
            .is_none_or(|p| self.layers.contains(p))
    }

    pub(crate) fn top(&self) -> Vec<Node<'a, 'i>> {
        self.content
            .iter()
            .copied()
            .filter(|c| self.is_top(*c))
            .collect()
    }

    /// The top-left corner of the top-level cells' geometry: vertex positions
    /// and edge points.
    pub(crate) fn origin(&self) -> (f64, f64) {
        let mut x = f64::INFINITY;
        let mut y = f64::INFINITY;
        for cell in self.top() {
            let Some(geometry) = geometry(cell) else {
                continue;
            };
            let points: Vec<_> = if inner(cell).attribute("edge") == Some("1") {
                geometry
                    .descendants()
                    .filter(|n| n.has_tag_name("mxPoint") && n.attribute("as") != Some("offset"))
                    .map(|p| (number(p, "x"), number(p, "y")))
                    .collect()
            } else {
                vec![(number(geometry, "x"), number(geometry, "y"))]
            };
            for (px, py) in points {
                x = x.min(px);
                y = y.min(py);
            }
        }
        (
            if x.is_finite() { x } else { 0.0 },
            if y.is_finite() { y } else { 0.0 },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(json: &str) -> Library {
        let xml = format!("<mxlibrary title=\"T\">{}</mxlibrary>", codec::text(json));
        parse(LibraryName::parse("lib").unwrap(), xml.as_bytes()).unwrap()
    }

    #[test]
    fn ids_come_from_the_id_field_or_the_title() {
        let lib = library(
            r#"[{"id":"keep","title":"A","xml":"<mxGraphModel/>"},
                {"title":"Keep","xml":"<mxGraphModel/>"},
                {"id":"Bad ID","title":"Keep","xml":"<mxGraphModel/>"},
                {"id":"keep","title":"","xml":"<mxGraphModel/>"}]"#,
        );
        let ids: Vec<_> = lib.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["keep", "keep-2", "keep-3", "entry"]);
        assert_eq!(lib.title.as_deref(), Some("T"));
    }

    #[test]
    fn kinds_follow_the_cells() {
        let cell = |extra: &str| {
            format!(
                r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{extra}</root></mxGraphModel>"#
            )
        };
        let entries = serde_json::json!([
            {"title": "image", "xml": cell(r#"<mxCell id="2" style="shape=image;image=data:image/png,AA" vertex="1" parent="1"><mxGeometry width="4" height="4" as="geometry"/></mxCell>"#)},
            {"title": "shape", "xml": cell(r#"<mxCell id="2" style="rounded=1;" vertex="1" parent="1"><mxGeometry width="4" height="4" as="geometry"/></mxCell>"#)},
            {"title": "edge", "xml": cell(r#"<mxCell id="2" edge="1" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"#)},
            {"title": "group", "xml": cell(r#"<mxCell id="2" vertex="1" parent="1"><mxGeometry as="geometry"/></mxCell><mxCell id="3" vertex="1" parent="2"><mxGeometry as="geometry"/></mxCell>"#)},
            {"title": "set", "xml": cell(r#"<mxCell id="2" vertex="1" parent="1"/><mxCell id="3" vertex="1" parent="1"/>"#)},
            {"title": "data", "data": "data:image/svg+xml;base64,PHN2Zy8+", "w": 8, "h": 4},
            {"title": "linked", "data": "https://example.com/a.svg"},
            {"title": "broken", "xml": "%%%"},
        ]);
        let lib = library(&entries.to_string());
        let kinds: Vec<_> = lib.entries.iter().map(|e| e.kind().as_str()).collect();
        assert_eq!(
            kinds,
            [
                "image", "shape", "edge", "group", "set", "image", "shape", "invalid"
            ]
        );
        assert_eq!(
            lib.entries[5].image().unwrap().as_deref(),
            Some("data:image/svg+xml,PHN2Zy8+")
        );
        assert_eq!(
            lib.entries[0].image().unwrap().as_deref(),
            Some("data:image/png,AA")
        );
        assert_eq!(lib.entries[1].image().unwrap(), None);
    }
}
