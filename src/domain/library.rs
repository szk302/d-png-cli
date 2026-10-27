//! draw.io custom libraries (`<mxlibrary>` files): list their entries and add
//! one to a diagram without exposing its payload, such as an embedded image.
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use roxmltree::{Document, Node};
use std::collections::{HashMap, HashSet};

use crate::domain::{codec::inflate, diagram};

/// One library entry, as the editor's sidebar shows it.
pub struct Entry {
    pub title: String,
    pub width: f64,
    pub height: f64,
    content: Content,
}

impl Entry {
    /// An entry holding an uncompressed `mxGraphModel`.
    pub fn model(title: String, width: f64, height: f64, xml: String) -> Self {
        Entry {
            title,
            width,
            height,
            content: Content::Model(xml),
        }
    }

    /// Whether the entry is a single vertex, which `insert` can resize, and the
    /// size of its top-level cells' bounding box (vertices and edge points, as
    /// `insert` aligns them), never less than the declared size of a multi-cell entry.
    pub fn extent(&self) -> Result<(bool, f64, f64)> {
        if matches!(self.content, Content::Image { .. }) {
            return Ok((true, self.width, self.height));
        }
        let model = entry_model(self)?;
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
        for &cell in cells.content.iter().filter(|c| cells.is_top(**c)) {
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

    /// The style of an entry made of one cell, to copy into hand-written XML;
    /// `None` for multi-cell entries and images, whose data belongs in `insert`.
    pub fn style(&self) -> Result<Option<String>> {
        Ok(self.cell_style()?.filter(|style| !style.contains("data:")))
    }

    /// The style of an entry made of one cell, image data included; `None`
    /// for multi-cell entries and images.
    pub fn cell_style(&self) -> Result<Option<String>> {
        if matches!(self.content, Content::Image { .. }) {
            return Ok(None);
        }
        let model = entry_model(self)?;
        let source = diagram::parse(&model).context("invalid library entry")?;
        let cells = Cells::of(source.root_element())?;
        Ok(match cells.content.as_slice() {
            [cell] => Some(inner(*cell).attribute("style").unwrap_or("").to_owned()),
            _ => None,
        })
    }
}

enum Content {
    /// An `mxGraphModel`, possibly compressed as draw.io stores it.
    Model(String),
    /// An image data URI or URL.
    Image { data: String, aspect: bool },
}

/// Parses an `<mxlibrary>` file.
pub fn parse(bytes: &[u8]) -> Result<Vec<Entry>> {
    Ok(parse_library(bytes)?.1)
}

/// Parses an `<mxlibrary>` file into its optional title and its entries.
pub fn parse_library(bytes: &[u8]) -> Result<(Option<String>, Vec<Entry>)> {
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
    let items: Vec<serde_json::Value> =
        serde_json::from_str(json.trim()).context("invalid library JSON")?;
    let entries = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let number = |key: &str| item[key].as_f64().unwrap_or(0.0);
            let content = if let Some(xml) = item["xml"].as_str() {
                Content::Model(xml.to_owned())
            } else if let Some(data) = item["data"].as_str() {
                Content::Image {
                    data: data.to_owned(),
                    aspect: item["aspect"].as_str() != Some("variable"),
                }
            } else {
                bail!("library entry {} has neither xml nor data", index + 1);
            };
            Ok(Entry {
                title: item["title"].as_str().unwrap_or("").to_owned(),
                width: number("w"),
                height: number("h"),
                content,
            })
        })
        .collect::<Result<_>>()?;
    Ok((title, entries))
}

/// A library: its name, the optional `<mxlibrary title>` that draw.io shows,
/// and its entries.
pub struct Library {
    pub name: String,
    pub title: Option<String>,
    pub entries: Vec<Entry>,
}

/// Selects an entry by exact title, then by a unique case-insensitive title,
/// across the given libraries.
pub fn find<'a>(libraries: &'a [Library], name: &str) -> Result<(&'a Library, &'a Entry)> {
    let matching = |same: &dyn Fn(&str) -> bool| -> Vec<(&'a Library, &'a Entry)> {
        libraries
            .iter()
            .flat_map(|l| l.entries.iter().map(move |e| (l, e)))
            .filter(|(_, e)| same(&e.title))
            .collect()
    };
    let exact = matching(&|title| title == name);
    let matches = if exact.is_empty() {
        let lower = name.to_lowercase();
        matching(&|title| title.to_lowercase() == lower)
    } else {
        exact
    };
    let mut names: Vec<&str> = matches.iter().map(|(l, _)| l.name.as_str()).collect();
    names.dedup();
    match (matches.as_slice(), names.as_slice()) {
        ([found], _) => Ok(*found),
        ([], _) => bail!("no library entry named {name:?}; see `drawio-png-cli library search`"),
        (_, [library]) => bail!(
            "{} entries in library {library:?} are named {name:?}; use --index",
            matches.len()
        ),
        _ => bail!(
            "{name:?} is in libraries {}; use --library",
            names
                .iter()
                .map(|n| format!("{n:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Where and how to add an entry.
pub struct Placement<'a> {
    /// 1-based page number.
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub id: Option<&'a str>,
    pub label: Option<&'a str>,
}

/// Inserts `entry` into a page of `xml`. Returns the expanded document and the new
/// cell IDs, the top-level cells first.
pub fn insert(
    xml: &str,
    entry: &Entry,
    placement: &Placement<'_>,
) -> Result<(String, Vec<String>)> {
    let xml = diagram::normalize(xml)?;
    let model = entry_model(entry)?;
    let source = diagram::parse(&model).context("invalid library entry")?;
    let cells = Cells::of(source.root_element())?;
    ensure!(!cells.content.is_empty(), "library entry has no cells");

    let doc = diagram::parse(&xml)?;
    let (graph_root, layer, existing) = target(&doc, placement.page)?;
    let ids = cells.new_ids(placement.id, &existing)?;
    let top: Vec<_> = cells
        .content
        .iter()
        .filter(|c| cells.is_top(**c))
        .copied()
        .collect();
    let single = (top.len() == 1).then(|| top[0]);
    ensure!(
        placement.label.is_none() || single.is_some(),
        "--label needs an entry with one top-level cell"
    );
    let resize = placement.width.is_some() || placement.height.is_some();
    ensure!(
        !resize || (single.is_some() && cells.content.len() == 1),
        "--width and --height need an entry with a single cell"
    );

    let (min_x, min_y) = cells.origin(&top);
    let edit = Edit {
        cells: &cells,
        ids: &ids,
        layer: &layer,
        dx: placement.x - min_x,
        dy: placement.y - min_y,
        placement,
    };
    let mut added = String::new();
    for &cell in &cells.content {
        edit.write(cell, &mut added);
    }
    // The page's <root> element always ends with its closing tag.
    let insert = graph_root.range().start
        + xml[graph_root.range()]
            .rfind("</")
            .context("page <root> has no closing tag")?;
    let mut output = xml.clone();
    output.insert_str(insert, &added);
    diagram::validate(&output).context("diagram is invalid after adding the entry")?;
    let mut order: Vec<String> = top.iter().map(|c| ids[&cells.id(*c)].clone()).collect();
    for &cell in &cells.content {
        let id = &ids[&cells.id(cell)];
        if !order.contains(id) {
            order.push(id.clone());
        }
    }
    Ok((output, order))
}

/// The entry as an uncompressed mxGraphModel.
fn entry_model(entry: &Entry) -> Result<String> {
    match &entry.content {
        Content::Model(xml) if xml.trim_start().starts_with('<') => Ok(xml.clone()),
        Content::Model(xml) => {
            let bytes = STANDARD
                .decode(xml.trim())
                .context("invalid library entry Base64")?;
            let inflated = inflate(&bytes, false).context("invalid compressed library entry")?;
            diagram::percent_decode(diagram::utf8(&inflated)?)
        }
        Content::Image { data, aspect } => {
            // A style value cannot contain ';', so draw.io drops ";base64".
            let data = data.replacen(";base64,", ",", 1);
            // Libraries may also link images; rendering those needs --allow-network.
            let source = data.starts_with("data:image/")
                || data.starts_with("https://")
                || data.starts_with("http://");
            ensure!(
                source && !data.contains(';'),
                "unsupported library image data"
            );
            let aspect = if *aspect { "aspect=fixed;" } else { "" };
            Ok(format!(
                r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="" style="shape=image;verticalLabelPosition=bottom;verticalAlign=top;imageAspect=0;{aspect}image={}" vertex="1" parent="1"><mxGeometry width="{}" height="{}" as="geometry"/></mxCell></root></mxGraphModel>"#,
                attribute(&data),
                entry.width,
                entry.height
            ))
        }
    }
}

/// Finds the page's <root>, the layer to add to, and the IDs in use.
fn target<'a, 'i>(
    doc: &'a Document<'i>,
    page: usize,
) -> Result<(Node<'a, 'i>, String, HashSet<String>)> {
    ensure!(page >= 1, "--page starts at 1");
    let root = doc.root_element();
    let model = if root.has_tag_name("mxGraphModel") {
        ensure!(page == 1, "the diagram has one page");
        root
    } else {
        let pages: Vec<_> = root
            .children()
            .filter(|n| n.has_tag_name("diagram"))
            .collect();
        let diagram = pages
            .get(page - 1)
            .with_context(|| format!("the diagram has {} page(s)", pages.len()))?;
        diagram
            .children()
            .find(|n| n.has_tag_name("mxGraphModel"))
            .context("page has no <mxGraphModel>")?
    };
    let graph_root = model
        .children()
        .find(|n| n.has_tag_name("root"))
        .context("page has no <root>")?;
    let mut existing = HashSet::new();
    let mut parents = HashMap::new();
    let mut order = Vec::new();
    for node in graph_root.descendants().filter(|n| is_cell(*n)) {
        let id = node.attribute("id").unwrap_or("").to_owned();
        parents.insert(
            id.clone(),
            inner(node).attribute("parent").map(str::to_owned),
        );
        existing.insert(id.clone());
        order.push(id);
    }
    // Layers are the children of the cell without a parent; use the first.
    let top = order
        .iter()
        .find(|id| parents[*id].is_none())
        .context("page has no root cell")?;
    let layer = order
        .iter()
        .find(|id| parents[*id].as_deref() == Some(top.as_str()))
        .context("page has no layer")?
        .clone();
    Ok((graph_root, layer, existing))
}

fn is_cell(node: Node<'_, '_>) -> bool {
    node.has_tag_name("mxCell")
        && !node
            .parent_element()
            .is_some_and(|p| p.has_tag_name("object") || p.has_tag_name("UserObject"))
        || node.has_tag_name("object")
        || node.has_tag_name("UserObject")
}

/// The mxCell carrying parent/source/target: the node or its child.
fn inner<'a, 'i>(node: Node<'a, 'i>) -> Node<'a, 'i> {
    if node.has_tag_name("mxCell") {
        node
    } else {
        node.children()
            .find(|n| n.has_tag_name("mxCell"))
            .unwrap_or(node)
    }
}

/// The library entry's cells, excluding its root cell and layers.
struct Cells<'a, 'i> {
    content: Vec<Node<'a, 'i>>,
    layers: HashSet<String>,
}

impl<'a, 'i> Cells<'a, 'i> {
    fn of(model: Node<'a, 'i>) -> Result<Self> {
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

    fn id(&self, cell: Node<'_, '_>) -> String {
        cell.attribute("id").unwrap_or("").to_owned()
    }

    fn is_top(&self, cell: Node<'_, '_>) -> bool {
        inner(cell)
            .attribute("parent")
            .is_none_or(|p| self.layers.contains(p))
    }

    /// Maps the entry's IDs to IDs unused on the target page.
    fn new_ids(
        &self,
        requested: Option<&str>,
        existing: &HashSet<String>,
    ) -> Result<HashMap<String, String>> {
        let mut used = existing.clone();
        let mut ids = HashMap::new();
        let mut next = 1;
        let mut fresh = |prefix: &str, used: &mut HashSet<String>| loop {
            let id = format!("{prefix}-{next}");
            next += 1;
            if used.insert(id.clone()) {
                break id;
            }
        };
        let top: Vec<_> = self.content.iter().filter(|c| self.is_top(**c)).collect();
        if let Some(id) = requested {
            ensure!(!id.is_empty(), "--id must not be empty");
            ensure!(
                top.len() == 1,
                "--id needs an entry with one top-level cell"
            );
            ensure!(
                used.insert(id.to_owned()),
                "cell ID {id:?} already exists on the page"
            );
            ids.insert(self.id(*top[0]), id.to_owned());
        }
        let prefix = requested.unwrap_or("lib");
        for cell in &self.content {
            let key = self.id(*cell);
            if let std::collections::hash_map::Entry::Vacant(slot) = ids.entry(key) {
                slot.insert(fresh(prefix, &mut used));
            }
        }
        Ok(ids)
    }

    /// The top-left corner of the top-level cells' geometry.
    fn origin(&self, top: &[Node<'_, '_>]) -> (f64, f64) {
        let mut x = f64::INFINITY;
        let mut y = f64::INFINITY;
        for cell in top {
            let Some(geometry) = geometry(*cell) else {
                continue;
            };
            let points: Vec<_> = if inner(*cell).attribute("edge") == Some("1") {
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

fn geometry<'a, 'i>(cell: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    inner(cell)
        .children()
        .find(|n| n.has_tag_name("mxGeometry"))
}

fn number(node: Node<'_, '_>, name: &str) -> f64 {
    node.attribute(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

struct Edit<'e, 'a, 'i, 'p> {
    cells: &'e Cells<'a, 'i>,
    ids: &'e HashMap<String, String>,
    layer: &'e str,
    dx: f64,
    dy: f64,
    placement: &'e Placement<'p>,
}

impl Edit<'_, '_, '_, '_> {
    /// Writes a cell with new IDs and references, moved into place.
    fn write(&self, cell: Node<'_, '_>, out: &mut String) {
        let top = self.cells.is_top(cell);
        let edge = inner(cell).attribute("edge") == Some("1");
        self.element(
            cell,
            out,
            &|node, name, value| {
                let is_cell = node == cell;
                let is_inner = node == inner(cell);
                match name {
                    "id" if is_cell => Some(self.ids[value].clone()),
                    "parent" if is_inner => Some(
                        self.ids
                            .get(value)
                            .cloned()
                            .unwrap_or_else(|| self.layer.to_owned()),
                    ),
                    "source" | "target" if is_inner => {
                        Some(self.ids.get(value).cloned().unwrap_or_default())
                    }
                    // Only the top-level cell is relabelled; a group's
                    // children keep their own labels.
                    "value" if top && is_cell && cell.has_tag_name("mxCell") => {
                        self.placement.label.map(str::to_owned)
                    }
                    "label" if top && is_cell && !cell.has_tag_name("mxCell") => {
                        self.placement.label.map(str::to_owned)
                    }
                    // Edges name the port cells they attach to in their style.
                    "style" if is_inner => remap_ports(value, self.ids),
                    _ => None,
                }
            },
            top,
            edge,
        );
    }

    fn element(
        &self,
        node: Node<'_, '_>,
        out: &mut String,
        rename: &dyn Fn(Node<'_, '_>, &str, &str) -> Option<String>,
        top: bool,
        edge: bool,
    ) {
        let name = node.tag_name().name();
        out.push('<');
        out.push_str(name);
        let geometry = name == "mxGeometry" && node.attribute("as") == Some("geometry");
        // Edge points move with the edge; a label offset is relative.
        let point = name == "mxPoint" && node.attribute("as") != Some("offset");
        let mut seen = HashSet::new();
        for attr in node.attributes() {
            let key = attr.name();
            seen.insert(key);
            let mut value =
                rename(node, key, attr.value()).unwrap_or_else(|| attr.value().to_owned());
            if top && ((geometry && !edge) || (point && edge)) {
                if key == "x" {
                    value = format_number(number(node, "x") + self.dx);
                } else if key == "y" {
                    value = format_number(number(node, "y") + self.dy);
                }
            }
            if geometry
                && top
                && let Some(size) = self.size(node, key)
            {
                value = size;
            }
            push_attribute(out, key, &value);
        }
        // Add coordinates and labels the source omitted.
        if top && ((geometry && !edge) || (point && edge)) {
            for (key, delta) in [("x", self.dx), ("y", self.dy)] {
                if !seen.contains(key) && delta != 0.0 {
                    push_attribute(out, key, &format_number(delta));
                }
            }
        }
        if let Some(label) = self.placement.label {
            let wants = if node.has_tag_name("mxCell") {
                "value"
            } else {
                "label"
            };
            if rename(node, wants, "").is_some() && !seen.contains(wants) {
                push_attribute(out, wants, label);
            }
        }
        let children: Vec<_> = node.children().collect();
        if children
            .iter()
            .all(|c| !c.is_element() && c.text().is_none_or(|t| t.trim().is_empty()))
        {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for child in children {
            if child.is_element() {
                if is_cell(child) {
                    continue;
                }
                self.element(child, out, rename, top, edge);
            } else if let Some(text) = child.text() {
                out.push_str(&text_escape(text));
            }
        }
        out.push_str("</");
        out.push_str(name);
        out.push('>');
    }

    /// The new width or height of a resized single cell.
    fn size(&self, geometry: Node<'_, '_>, key: &str) -> Option<String> {
        let (width, height) = (number(geometry, "width"), number(geometry, "height"));
        let (w, h) = match (self.placement.width, self.placement.height) {
            (None, None) => return None,
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (
                w,
                if width > 0.0 {
                    height * w / width
                } else {
                    height
                },
            ),
            (None, Some(h)) => (
                if height > 0.0 {
                    width * h / height
                } else {
                    width
                },
                h,
            ),
        };
        match key {
            "width" => Some(format_number(w)),
            "height" => Some(format_number(h)),
            _ => None,
        }
    }
}

/// Rewrites `sourcePort`/`targetPort` style values that name cells of the entry.
fn remap_ports(style: &str, ids: &HashMap<String, String>) -> Option<String> {
    let mut changed = false;
    let items: Vec<String> = style
        .split(';')
        .map(|item| {
            if let Some((key @ ("sourcePort" | "targetPort"), port)) = item.split_once('=')
                && let Some(id) = ids.get(port)
            {
                changed = true;
                return format!("{key}={id}");
            }
            item.to_owned()
        })
        .collect();
    changed.then(|| items.join(";"))
}

fn format_number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

fn push_attribute(out: &mut String, key: &str, value: &str) {
    out.push(' ');
    out.push_str(key);
    out.push_str("=\"");
    out.push_str(&attribute(value));
    out.push('"');
}

fn attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "&#xa;")
        .replace('\r', "&#xd;")
        .replace('\t', "&#x9;")
}

fn text_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
