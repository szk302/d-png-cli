//! Expansion of library references in a diagram (spec 4.3, 4.4): placeholder
//! cells with `libref` become the entry's cells, and `image=lib:` style values
//! become the image data. The libraries are already resolved by the caller;
//! this module only edits the diagram.
use roxmltree::{Document, Node};
use std::collections::{HashMap, HashSet};

use crate::domain::{
    codec,
    diagnostic::{Code, Diagnostic, Diagnostics, Location},
    diagram,
    element::{Child, Element},
    library::{self, Cells, Entry, EntryKind, inner, is_cell},
    reference::{ImageRef, Ref},
};

/// The attribute marking a placeholder cell.
pub const LIBREF: &str = "libref";

/// Attributes of a placeholder's mxCell that the expansion itself uses.
const PLACEHOLDER_KEYS: &[&str] = &[
    "id", "value", "style", "parent", "vertex", "edge", "source", "target", LIBREF,
];

/// One reference found in a diagram, with where it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found<T> {
    pub value: T,
    pub location: Location,
}

/// Every reference a diagram makes.
#[derive(Clone, Debug, Default)]
pub struct References {
    pub cells: Vec<Found<Ref>>,
    pub images: Vec<Found<ImageRef>>,
}

/// What the caller looked up for the references: entries for `libref`, and
/// the image value for each `image=lib:` value (the text after `lib:`).
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    pub entries: HashMap<Ref, Entry>,
    pub images: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expanded {
    /// A placeholder became these cells, the top-level ones first.
    Cells(Vec<String>),
    /// An image reference became image data.
    Image,
}

/// One expanded reference, for reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedRef {
    pub location: Location,
    pub reference: String,
    pub result: Expanded,
}

#[derive(Clone, Debug)]
pub struct Expansion {
    pub xml: String,
    pub expanded: Vec<ExpandedRef>,
}

/// A page's graph model with its 1-based number and name.
pub(crate) struct Page<'a, 'i> {
    pub(crate) number: usize,
    pub(crate) name: Option<String>,
    pub(crate) model: Node<'a, 'i>,
}

pub(crate) fn pages<'a, 'i>(doc: &'a Document<'i>) -> Vec<Page<'a, 'i>> {
    let root = doc.root_element();
    if root.has_tag_name("mxGraphModel") {
        return vec![Page {
            number: 1,
            name: None,
            model: root,
        }];
    }
    root.children()
        .filter(|n| n.has_tag_name("diagram"))
        .enumerate()
        .filter_map(|(index, page)| {
            Some(Page {
                number: index + 1,
                name: page.attribute("name").map(str::to_owned),
                model: page.children().find(|n| n.has_tag_name("mxGraphModel"))?,
            })
        })
        .collect()
}

/// The cells of a page in document order: mxCells and wrappers.
pub(crate) fn cells<'a, 'i>(model: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    model
        .children()
        .filter(|n| n.has_tag_name("root"))
        .flat_map(|root| root.descendants().filter(|n| is_cell(*n)))
        .collect()
}

impl Page<'_, '_> {
    pub(crate) fn at(&self, cell: Option<&str>) -> Location {
        Location {
            page: Some(self.number),
            page_name: self.name.clone(),
            cell: cell.map(str::to_owned),
        }
    }
}

/// The `image=lib:` values in a style, in order.
pub fn image_refs(style: &str) -> impl Iterator<Item = &str> {
    style
        .split(';')
        .filter_map(|item| item.strip_prefix("image="))
        .filter_map(|value| value.strip_prefix(ImageRef::PREFIX))
}

/// Replaces `image=lib:` values with resolved image data. Returns `None` when
/// the style has no image reference.
pub fn substitute_images(style: &str, images: &HashMap<String, String>) -> Option<String> {
    let mut changed = false;
    let items: Vec<String> = style
        .split(';')
        .map(|item| {
            if let Some(value) = item
                .strip_prefix("image=")
                .and_then(|v| v.strip_prefix(ImageRef::PREFIX))
                && let Some(data) = images.get(value)
            {
                changed = true;
                return format!("image={data}");
            }
            item.to_owned()
        })
        .collect();
    changed.then(|| items.join(";"))
}

/// Finds the references of a normalized diagram. Malformed ones are reported
/// together.
pub fn references(xml: &str) -> Result<References, Diagnostics> {
    let doc = diagram::parse(xml)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::XmlSyntax, &e)))?;
    let mut found = References::default();
    let mut problems = Diagnostics::default();
    for page in pages(&doc) {
        for cell in cells(page.model) {
            let id = cell.attribute("id");
            let at = page.at(id);
            let inner = inner(cell);
            match (
                cell.attribute(LIBREF),
                (inner != cell).then(|| inner.attribute(LIBREF)).flatten(),
            ) {
                (Some(_), Some(_)) => problems.push(
                    Diagnostic::new(
                        Code::RefSyntax,
                        "write libref on the wrapper or on its mxCell, not both",
                    )
                    .at(at.clone()),
                ),
                (Some(text), None) | (None, Some(text)) => match Ref::parse(text) {
                    Ok(value) => found.cells.push(Found {
                        value,
                        location: at.clone(),
                    }),
                    Err(d) => problems.push(d.at(at.clone())),
                },
                (None, None) => {}
            }
            for text in image_refs(inner.attribute("style").unwrap_or("")) {
                match ImageRef::parse(text) {
                    Ok(value) => found.images.push(Found {
                        value,
                        location: at.clone(),
                    }),
                    Err(d) => problems.push(d.at(at.clone())),
                }
            }
        }
    }
    problems.into_result(found)
}

/// Expands every reference of a normalized diagram with resolved libraries.
/// Every reference must be in `resolved`; problems with how a reference uses
/// its entry are reported together.
pub fn expand(xml: &str, resolved: &Resolved) -> Result<Expansion, Diagnostics> {
    let doc = diagram::parse(xml)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::XmlSyntax, &e)))?;
    let mut problems = Diagnostics::default();
    let mut replacements: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut expanded = Vec::new();
    for page in pages(&doc) {
        let cells = cells(page.model);
        let mut used: HashSet<String> = cells
            .iter()
            .filter_map(|c| c.attribute("id"))
            .map(str::to_owned)
            .collect();
        for cell in &cells {
            let inner = inner(*cell);
            let text = cell
                .attribute(LIBREF)
                .or_else(|| (inner != *cell).then(|| inner.attribute(LIBREF)).flatten());
            let Some(text) = text else {
                // An ordinary cell: only image references change.
                if let Some(style) = inner.attribute("style")
                    && let Some(new) = substitute_images(style, &resolved.images)
                {
                    let attr = inner
                        .attributes()
                        .find(|a| a.name() == "style" && a.namespace().is_none())
                        .expect("style attribute");
                    replacements.push((
                        attr.range(),
                        format!("style=\"{}\"", codec::attribute(&new)),
                    ));
                    for reference in image_refs(style) {
                        expanded.push(ExpandedRef {
                            location: page.at(cell.attribute("id")),
                            reference: reference.to_owned(),
                            result: Expanded::Image,
                        });
                    }
                }
                continue;
            };
            let at = page.at(cell.attribute("id"));
            let Ok(reference) = Ref::parse(text) else {
                continue; // reported by `references`
            };
            let Some(entry) = resolved.entries.get(&reference) else {
                problems.push(
                    Diagnostic::new(Code::RefNotFound, format!("{reference} is not resolved"))
                        .at(at)
                        .reference(text),
                );
                continue;
            };
            match Placeholder::read(*cell)
                .and_then(|placeholder| placeholder.expand(entry, &cells, &mut used, resolved))
            {
                Ok((xml, ids)) => {
                    replacements.push((cell.range(), xml));
                    expanded.push(ExpandedRef {
                        location: at,
                        reference: text.to_owned(),
                        result: Expanded::Cells(ids),
                    });
                }
                Err(found) => {
                    for d in found {
                        let d = if d.location == Location::default() {
                            d.at(at.clone())
                        } else {
                            d
                        };
                        problems.push(d.reference(text));
                    }
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    replacements.sort_by_key(|(range, _)| range.start);
    let mut output = xml.to_owned();
    for (range, replacement) in replacements.into_iter().rev() {
        output.replace_range(range, &replacement);
    }
    Ok(Expansion {
        xml: output,
        expanded,
    })
}

/// A placeholder cell: what it asks of the entry it references.
struct Placeholder {
    id: String,
    parent: String,
    /// The wrapper element's name and its attributes besides id, label and libref.
    wrapper: Option<(String, Vec<(String, String)>)>,
    label: Option<String>,
    style: Vec<(String, String)>,
    /// Other mxCell attributes, such as connectable or visible.
    extra: Vec<(String, String)>,
    vertex: bool,
    edge: bool,
    source: Option<String>,
    target: Option<String>,
    x: f64,
    y: f64,
    width: Option<f64>,
    height: Option<f64>,
    geometry: Option<Element>,
}

type Problems = Vec<Diagnostic>;

fn problem(code: Code, message: impl Into<String>) -> Problems {
    vec![Diagnostic::new(code, message)]
}

impl Placeholder {
    fn read(cell: Node<'_, '_>) -> Result<Self, Problems> {
        let inner = inner(cell);
        let wrapped = inner != cell;
        let id = cell.attribute("id").unwrap_or("").to_owned();
        if id.is_empty() {
            return Err(problem(Code::XmlStructure, "a placeholder needs an id"));
        }
        let parent = inner
            .attribute("parent")
            .ok_or_else(|| problem(Code::XmlStructure, "a placeholder needs a parent"))?
            .to_owned();
        let wrapper = wrapped.then(|| {
            (
                cell.tag_name().name().to_owned(),
                cell.attributes()
                    .filter(|a| !["id", "label", LIBREF].contains(&a.name()))
                    .map(|a| (a.name().to_owned(), a.value().to_owned()))
                    .collect(),
            )
        });
        let label = if wrapped {
            cell.attribute("label")
        } else {
            inner.attribute("value")
        }
        .map(str::to_owned);
        let mut style = Vec::new();
        for item in inner
            .attribute("style")
            .unwrap_or("")
            .split(';')
            .filter(|i| !i.is_empty())
        {
            match item.split_once('=') {
                Some((k, v)) => style.push((k.to_owned(), v.to_owned())),
                None => {
                    return Err(problem(
                        Code::RefOverride,
                        format!("a placeholder cannot add the named style {item:?}"),
                    ));
                }
            }
        }
        let extra = inner
            .attributes()
            .filter(|a| !PLACEHOLDER_KEYS.contains(&a.name()))
            .map(|a| (a.name().to_owned(), a.value().to_owned()))
            .collect();
        if let Some(child) = inner
            .children()
            .find(|n| n.is_element() && !n.has_tag_name("mxGeometry"))
        {
            return Err(problem(
                Code::RefOverride,
                format!(
                    "a placeholder cannot hold <{}>; edit the cell after `expand`",
                    child.tag_name().name()
                ),
            ));
        }
        let geometry = inner.children().find(|n| n.has_tag_name("mxGeometry"));
        let number = |key: &str| -> Result<Option<f64>, Problems> {
            let Some(text) = geometry.and_then(|g| g.attribute(key)) else {
                return Ok(None);
            };
            match text.parse::<f64>() {
                Ok(v) if v.is_finite() => Ok(Some(v)),
                _ => Err(problem(
                    Code::XmlStructure,
                    format!("mxGeometry {key}={text:?} is not a finite number"),
                )),
            }
        };
        let (width, height) = (number("width")?, number("height")?);
        for (key, value) in [("width", width), ("height", height)] {
            if value.is_some_and(|v| v <= 0.0) {
                return Err(problem(
                    Code::RefOverride,
                    format!("{key} must be positive"),
                ));
            }
        }
        Ok(Self {
            id,
            parent,
            wrapper,
            label,
            style,
            extra,
            vertex: inner.attribute("vertex") == Some("1"),
            edge: inner.attribute("edge") == Some("1"),
            source: inner.attribute("source").map(str::to_owned),
            target: inner.attribute("target").map(str::to_owned),
            x: number("x")?.unwrap_or(0.0),
            y: number("y")?.unwrap_or(0.0),
            width,
            height,
            geometry: geometry.map(Element::from_node),
        })
    }

    /// The overrides that only apply to one top-level cell.
    fn overrides(&self) -> Vec<&'static str> {
        let mut found = Vec::new();
        if self.label.is_some() {
            found.push("a label");
        }
        if !self.style.is_empty() {
            found.push("a style");
        }
        if !self.extra.is_empty() {
            found.push("cell attributes");
        }
        if self.wrapper.as_ref().is_some_and(|(_, a)| !a.is_empty()) {
            found.push("custom data");
        }
        found
    }

    fn check(&self, kind: EntryKind) -> Result<(), Problems> {
        let mut problems = Vec::new();
        let mut push = |code, message: String| problems.push(Diagnostic::new(code, message));
        match kind {
            EntryKind::Invalid => push(
                Code::RefKind,
                "the entry cannot be read; see `library show`".into(),
            ),
            EntryKind::Set => {
                if self.vertex || self.edge {
                    push(
                        Code::RefKind,
                        "a set holds several cells; omit vertex and edge".into(),
                    );
                }
                for what in self.overrides() {
                    push(
                        Code::RefOverride,
                        format!("a set has several top-level cells and takes no {what}"),
                    );
                }
            }
            EntryKind::Edge => {
                if self.vertex {
                    push(Code::RefKind, "the entry is an edge, not a vertex".into());
                }
            }
            _ => {
                if self.edge {
                    push(Code::RefKind, format!("the entry is a {kind}, not an edge"));
                }
            }
        }
        if kind != EntryKind::Edge && (self.source.is_some() || self.target.is_some()) {
            push(
                Code::RefOverride,
                format!("source and target need an edge entry, not a {kind}"),
            );
        }
        if !kind.resizable() && (self.width.is_some() || self.height.is_some()) {
            push(
                Code::RefOverride,
                format!("width and height need an image or shape entry, not a {kind}"),
            );
        }
        if kind != EntryKind::Edge
            && let Some(geometry) = &self.geometry
        {
            if let Some(child) = geometry.elements().next() {
                push(
                    Code::RefOverride,
                    format!(
                        "the placeholder mxGeometry of a {kind} cannot hold <{}>",
                        child.name
                    ),
                );
            }
            for (key, _) in &geometry.attributes {
                if !["x", "y", "width", "height", "as"].contains(&key.as_str()) {
                    push(
                        Code::RefOverride,
                        format!("the placeholder mxGeometry of a {kind} cannot set {key}"),
                    );
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }

    /// The entry's cells as XML for this placeholder, and their IDs, the
    /// top-level cells first.
    fn expand(
        &self,
        entry: &Entry,
        page: &[Node<'_, '_>],
        used: &mut HashSet<String>,
        resolved: &Resolved,
    ) -> Result<(String, Vec<String>), Problems> {
        let kind = entry.kind();
        self.check(kind)?;
        let invalid = |e: anyhow::Error| problem(Code::RefKind, format!("invalid entry: {e:#}"));
        let model = entry.graph_model().map_err(invalid)?;
        let source = diagram::parse(&model).map_err(invalid)?;
        let entry_cells = Cells::of(source.root_element()).map_err(invalid)?;
        let top = entry_cells.top();
        let set = kind == EntryKind::Set;

        // IDs: the top-level cell keeps the placeholder's; the others are
        // numbered after it. A set loses the placeholder ID entirely.
        let mut ids = HashMap::new();
        let mut next = 1;
        let mut conflicts = Vec::new();
        if !set {
            ids.insert(entry_cells.id(top[0]), self.id.clone());
        }
        for cell in &entry_cells.content {
            let key = entry_cells.id(*cell);
            if ids.contains_key(&key) {
                continue;
            }
            let id = format!("{}-{next}", self.id);
            next += 1;
            if !used.insert(id.clone()) {
                conflicts.push(Diagnostic::new(
                    Code::RefIdConflict,
                    format!("the expansion creates {id:?}, which is already on the page"),
                ));
            }
            ids.insert(key, id);
        }
        let mut order: Vec<String> = top
            .iter()
            .map(|c| ids[&entry_cells.id(*c)].clone())
            .collect();
        for cell in &entry_cells.content {
            let id = &ids[&entry_cells.id(*cell)];
            if !order.contains(id) {
                order.push(id.clone());
            }
        }
        if set {
            for other in page {
                let inner = inner(*other);
                let style = inner.attribute("style").unwrap_or("");
                let points_here = ["parent", "source", "target"]
                    .iter()
                    .any(|k| inner.attribute(*k) == Some(self.id.as_str()))
                    || ["sourcePort", "targetPort"]
                        .iter()
                        .any(|k| library::style_value(style, k) == Some(self.id.as_str()));
                if points_here {
                    conflicts.push(Diagnostic::new(
                        Code::RefIdConflict,
                        format!(
                            "cell {:?} refers to {:?}, which a set expansion removes; use the IDs from `library show` ({}-1, …)",
                            other.attribute("id").unwrap_or(""),
                            self.id,
                            self.id
                        ),
                    ));
                }
            }
        }
        if !conflicts.is_empty() {
            return Err(conflicts);
        }

        let (origin_x, origin_y) = entry_cells.origin();
        let (dx, dy) = (self.x - origin_x, self.y - origin_y);
        let mut out = String::new();
        for cell in &entry_cells.content {
            let is_top = entry_cells.is_top(*cell);
            let mut element = Element::from_node(*cell);
            // Cells are siblings in draw.io XML; a wrapper's only cell is its mxCell.
            element.retain_elements(|e| !matches!(e.name.as_str(), "object" | "UserObject"));
            remap(&mut element, &ids, &self.parent);
            if is_top {
                move_geometry(&mut element, dx, dy);
            }
            if is_top && !set {
                element = self.apply(element, kind)?;
            }
            if let Some(inner) = inner_mut(&mut element)
                && let Some(style) = inner.get("style")
                && let Some(new) = substitute_images(style, &resolved.images)
            {
                inner.set("style", new);
            }
            element.write(&mut out);
        }
        Ok((out, order))
    }

    /// Applies the placeholder's overrides to the entry's top-level cell.
    fn apply(&self, mut cell: Element, kind: EntryKind) -> Result<Element, Problems> {
        let entry_wrapped = cell.name != "mxCell";
        let entry_label = if entry_wrapped {
            cell.get("label").map(str::to_owned)
        } else {
            cell.get("value").map(str::to_owned)
        };
        let label = self.label.clone().or(entry_label);

        // The cell itself.
        let mut inner = if entry_wrapped {
            let mut wrapper = cell.clone();
            let index = wrapper
                .children
                .iter()
                .position(|c| matches!(c, Child::Element(e) if e.name == "mxCell"))
                .ok_or_else(|| problem(Code::RefKind, "the entry's wrapper has no mxCell"))?;
            let Child::Element(inner) = wrapper.children.remove(index) else {
                unreachable!()
            };
            cell = wrapper;
            inner
        } else {
            cell.clone()
        };
        if !self.style.is_empty() {
            let merged = merge_style(inner.get("style").unwrap_or(""), &self.style);
            inner.set("style", merged);
        }
        for (key, value) in &self.extra {
            inner.set(key, value.clone());
        }
        if kind == EntryKind::Edge {
            self.connect(&mut inner);
        } else if let Some(geometry) = inner.child_mut("mxGeometry") {
            resize(geometry, self.width, self.height);
        } else if self.width.is_some() || self.height.is_some() {
            let mut geometry = Element::new("mxGeometry");
            resize(&mut geometry, self.width, self.height);
            geometry.set("as", "geometry");
            inner.children.push(Child::Element(geometry));
        }

        // The wrapper: the placeholder's element name if it has one, else the
        // entry's; custom data merged, the placeholder's winning.
        let wrapper_name = self
            .wrapper
            .as_ref()
            .map(|(name, _)| name.clone())
            .or_else(|| entry_wrapped.then(|| cell.name.clone()));
        let Some(wrapper_name) = wrapper_name else {
            inner.set("id", self.id.clone());
            if let Some(label) = label {
                inner.set("value", label);
            }
            move_first(&mut inner.attributes, &["id", "value"]);
            return Ok(inner);
        };
        let mut wrapper = Element::new(&wrapper_name);
        wrapper.set("id", self.id.clone());
        if let Some(label) = label {
            wrapper.set("label", label);
        }
        if entry_wrapped {
            for (key, value) in &cell.attributes {
                if key != "id" && key != "label" {
                    wrapper.set(key, value.clone());
                }
            }
        }
        if let Some((_, attributes)) = &self.wrapper {
            for (key, value) in attributes {
                wrapper.set(key, value.clone());
            }
        }
        inner.remove("id");
        inner.remove("value");
        wrapper.children.push(Child::Element(inner));
        Ok(wrapper)
    }

    /// Connects an edge entry's top-level cell (spec 4.3, edges).
    fn connect(&self, inner: &mut Element) {
        if let Some(source) = &self.source {
            inner.set("source", source.clone());
        }
        if let Some(target) = &self.target {
            inner.set("target", target.clone());
        }
        match (&self.source, &self.target) {
            (Some(_), Some(_)) => {
                let geometry = self.geometry.clone().unwrap_or_else(|| {
                    let mut g = Element::new("mxGeometry");
                    g.set("relative", "1");
                    g.set("as", "geometry");
                    g
                });
                inner.retain_elements(|e| e.name != "mxGeometry");
                inner.children.push(Child::Element(geometry));
            }
            (source, target) => {
                let points = self
                    .geometry
                    .as_ref()
                    .and_then(|g| {
                        g.elements()
                            .find(|e| e.name == "Array" && e.get("as") == Some("points"))
                    })
                    .cloned();
                if let Some(geometry) = inner.child_mut("mxGeometry") {
                    geometry.retain_elements(|e| {
                        let end = e.get("as");
                        !(source.is_some() && end == Some("sourcePoint")
                            || target.is_some() && end == Some("targetPoint")
                            || points.is_some() && e.name == "Array" && end == Some("points"))
                    });
                    if let Some(points) = points {
                        geometry.children.push(Child::Element(points));
                    }
                }
            }
        }
    }
}

fn inner_mut(cell: &mut Element) -> Option<&mut Element> {
    if cell.name == "mxCell" {
        Some(cell)
    } else {
        cell.child_mut("mxCell")
    }
}

/// Gives the entry's cells their new IDs and references; top-level cells go
/// under `parent`.
fn remap(cell: &mut Element, ids: &HashMap<String, String>, parent: &str) {
    if let Some(id) = cell.get("id").and_then(|id| ids.get(id)).cloned() {
        cell.set("id", id);
    }
    let Some(inner) = inner_mut(cell) else {
        return;
    };
    let mapped = |value: Option<&str>| value.and_then(|v| ids.get(v)).cloned();
    match mapped(inner.get("parent")) {
        Some(id) => inner.set("parent", id),
        None => inner.set("parent", parent.to_owned()),
    }
    for key in ["source", "target"] {
        match mapped(inner.get(key)) {
            Some(id) => inner.set(key, id),
            None => {
                inner.remove(key);
            }
        }
    }
    if let Some(style) = inner.get("style")
        && let Some(new) = remap_ports(style, ids)
    {
        inner.set("style", new);
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

/// Moves a top-level cell: a vertex's position, or an edge's points (a label
/// offset is relative and stays).
fn move_geometry(cell: &mut Element, dx: f64, dy: f64) {
    let Some(inner) = inner_mut(cell) else {
        return;
    };
    let edge = inner.get("edge") == Some("1");
    let Some(geometry) = inner.child_mut("mxGeometry") else {
        return;
    };
    let shift = |element: &mut Element| {
        for (key, delta) in [("x", dx), ("y", dy)] {
            let value = element.get(key).and_then(|v| v.parse::<f64>().ok());
            if value.is_some() || delta != 0.0 {
                element.set(key, codec::number(value.unwrap_or(0.0) + delta));
            }
        }
    };
    if edge {
        geometry.walk_mut(&mut |element| {
            if element.name == "mxPoint" && element.get("as") != Some("offset") {
                shift(element);
            }
        });
    } else {
        shift(geometry);
        move_first(&mut geometry.attributes, &["x", "y"]);
    }
}

/// Resizes a single cell; with one dimension, the other keeps the aspect ratio.
fn resize(geometry: &mut Element, width: Option<f64>, height: Option<f64>) {
    let current = |key: &str| {
        geometry
            .get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let (w0, h0) = (current("width"), current("height"));
    let (w, h) = match (width, height) {
        (None, None) => return,
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, if w0 > 0.0 { h0 * w / w0 } else { h0 }),
        (None, Some(h)) => (if h0 > 0.0 { w0 * h / h0 } else { w0 }, h),
    };
    geometry.set("width", codec::number(w));
    geometry.set("height", codec::number(h));
}

/// Overlays style keys on an entry's style: existing keys change in place,
/// new ones are appended.
pub fn merge_style(style: &str, overrides: &[(String, String)]) -> String {
    let mut items: Vec<String> = style
        .split(';')
        .filter(|i| !i.is_empty())
        .map(str::to_owned)
        .collect();
    for (key, value) in overrides {
        let item = format!("{key}={value}");
        match items
            .iter_mut()
            .filter(|i| i.split_once('=').is_some_and(|(k, _)| k == key))
            .last()
        {
            Some(slot) => *slot = item,
            None => items.push(item),
        }
    }
    let mut merged = items.join(";");
    merged.push(';');
    merged
}

/// Orders the given keys first, in that order, keeping the rest.
fn move_first(attributes: &mut Vec<(String, String)>, keys: &[&str]) {
    let mut first = Vec::new();
    for key in keys {
        if let Some(index) = attributes.iter().position(|(k, _)| k == key) {
            first.push(attributes.remove(index));
        }
    }
    first.append(attributes);
    *attributes = first;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_merge_by_key() {
        assert_eq!(
            merge_style("rounded=1;", &[("fillColor".into(), "#dae8fc".into())]),
            "rounded=1;fillColor=#dae8fc;"
        );
        assert_eq!(
            merge_style(
                "group;fillColor=red",
                &[("fillColor".into(), "blue".into())]
            ),
            "group;fillColor=blue;"
        );
    }

    #[test]
    fn image_references_are_found_and_substituted() {
        let style = "shape=image;image=lib:a/b@37483e4b7f30;aspect=fixed;";
        assert_eq!(image_refs(style).collect::<Vec<_>>(), ["a/b@37483e4b7f30"]);
        let images = HashMap::from([(
            "a/b@37483e4b7f30".to_owned(),
            "data:image/png,AA".to_owned(),
        )]);
        assert_eq!(
            substitute_images(style, &images).unwrap(),
            "shape=image;image=data:image/png,AA;aspect=fixed;"
        );
        assert!(substitute_images("shape=image;", &images).is_none());
    }
}
