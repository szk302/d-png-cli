//! What `read` does to images (spec 7): image data that a library entry also
//! holds becomes a fingerprinted reference, so an agent never has to carry
//! the data; `expand` turns it back.
use std::collections::HashMap;

use crate::domain::{
    codec,
    diagnostic::{Code, Diagnostic, Diagnostics, Location},
    diagram,
    expand::{cells, pages},
    library::inner,
    reference::{ImageRef, Ref, fingerprint},
};

/// An image replaced by a reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compacted {
    pub location: Location,
    /// `<ref>@<fingerprint>`, as written after `lib:`.
    pub reference: String,
    /// Characters of image data the reference replaced.
    pub replaced_chars: usize,
}

/// Image data no library holds, left in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptImage {
    pub location: Location,
    pub chars: usize,
}

#[derive(Clone, Debug)]
pub struct Compaction {
    pub xml: String,
    pub compacted: Vec<Compacted>,
    pub kept: Vec<KeptImage>,
}

/// Replaces the image data of a normalized diagram that `index` (fingerprint
/// to reference) knows. References already in the diagram stay as they are.
pub fn compact(xml: &str, index: &HashMap<String, Ref>) -> Result<Compaction, Diagnostics> {
    let doc = diagram::parse(xml)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::XmlSyntax, &e)))?;
    let mut replacements = Vec::new();
    let mut compacted = Vec::new();
    let mut kept = Vec::new();
    for page in pages(&doc) {
        for cell in cells(page.model) {
            let inner = inner(cell);
            let Some(attribute) = inner
                .attributes()
                .find(|a| a.name() == "style" && a.namespace().is_none())
            else {
                continue;
            };
            let mut changed = false;
            let items: Vec<String> = attribute
                .value()
                .split(';')
                .map(|item| {
                    let Some(value) = item
                        .strip_prefix("image=")
                        .filter(|v| v.starts_with("data:"))
                    else {
                        return item.to_owned();
                    };
                    let location = page.at(cell.attribute("id"));
                    let print = fingerprint(value);
                    match index.get(&print) {
                        Some(reference) => {
                            let reference = ImageRef {
                                reference: reference.clone(),
                                fingerprint: Some(print),
                            }
                            .to_string();
                            changed = true;
                            compacted.push(Compacted {
                                location,
                                reference: reference.clone(),
                                replaced_chars: value.chars().count(),
                            });
                            format!("image={}{reference}", ImageRef::PREFIX)
                        }
                        None => {
                            kept.push(KeptImage {
                                location,
                                chars: value.chars().count(),
                            });
                            item.to_owned()
                        }
                    }
                })
                .collect();
            if changed {
                replacements.push((
                    attribute.range(),
                    format!("style=\"{}\"", codec::attribute(&items.join(";"))),
                ));
            }
        }
    }
    let mut output = xml.to_owned();
    for (range, replacement) in replacements.into_iter().rev() {
        output.replace_range(range, &replacement);
    }
    Ok(Compaction {
        xml: output,
        compacted,
        kept,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_images_become_references() {
        let known = "data:image/png,AAAA";
        let xml = format!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="a" style="shape=image;image={known};aspect=fixed;" vertex="1" parent="1"/><mxCell id="b" style="image=data:image/png,BBBB" vertex="1" parent="1"/></root></mxGraphModel>"#
        );
        let index = HashMap::from([(fingerprint(known), Ref::parse("icons/a").unwrap())]);
        let result = compact(&xml, &index).unwrap();
        let reference = format!("icons/a@{}", fingerprint(known));
        assert!(
            result.xml.contains(&format!(
                "style=\"shape=image;image=lib:{reference};aspect=fixed;\""
            )),
            "{}",
            result.xml
        );
        assert!(result.xml.contains("image=data:image/png,BBBB"));
        assert_eq!(result.compacted[0].reference, reference);
        assert_eq!(result.compacted[0].replaced_chars, known.len());
        assert_eq!(result.kept[0].location.cell.as_deref(), Some("b"));
        // Idempotent.
        let again = compact(&result.xml, &index).unwrap();
        assert_eq!(again.xml, result.xml);
    }
}
