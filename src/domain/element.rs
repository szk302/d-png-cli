//! A small owned XML element tree for building cells: copied from parsed
//! input, edited, and written back. Comments and processing instructions
//! inside a cell are not kept.
use roxmltree::Node;

use crate::domain::codec;

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Child>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Child {
    Element(Element),
    Text(String),
}

impl Element {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Copies a parsed element and its element and text descendants.
    pub fn from_node(node: Node<'_, '_>) -> Self {
        Self {
            name: node.tag_name().name().to_owned(),
            attributes: node
                .attributes()
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect(),
            children: node
                .children()
                .filter_map(|child| {
                    if child.is_element() {
                        Some(Child::Element(Self::from_node(child)))
                    } else if child.is_text() {
                        child.text().map(|t| Child::Text(t.to_owned()))
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Sets an attribute, in place if present, otherwise last.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if let Some(slot) = self.attributes.iter_mut().find(|(k, _)| k == key) {
            slot.1 = value;
        } else {
            self.attributes.push((key.to_owned(), value));
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        let index = self.attributes.iter().position(|(k, _)| k == key)?;
        Some(self.attributes.remove(index).1)
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Child::Element(e) => Some(e),
            Child::Text(_) => None,
        })
    }

    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.children.iter_mut().filter_map(|c| match c {
            Child::Element(e) => Some(e),
            Child::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.elements_mut().find(|e| e.name == name)
    }

    /// Removes the child elements for which `remove` is true.
    pub fn retain_elements(&mut self, mut keep: impl FnMut(&Element) -> bool) {
        self.children.retain(|c| match c {
            Child::Element(e) => keep(e),
            Child::Text(_) => true,
        });
    }

    /// Visits this element and every descendant element.
    pub fn walk_mut(&mut self, visit: &mut impl FnMut(&mut Element)) {
        visit(self);
        for child in self.elements_mut() {
            child.walk_mut(visit);
        }
    }

    /// Writes the element; one without elements or non-blank text is self-closing.
    pub fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (key, value) in &self.attributes {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            out.push_str(&codec::attribute(value));
            out.push('"');
        }
        let empty = self.children.iter().all(|c| match c {
            Child::Element(_) => false,
            Child::Text(t) => t.trim().is_empty(),
        });
        if empty {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for child in &self.children {
            match child {
                Child::Element(e) => e.write(out),
                Child::Text(t) => out.push_str(&codec::text(t)),
            }
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }
}
