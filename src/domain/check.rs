//! Checks of an expanded diagram (spec 4.5): cell IDs, the cells that
//! parent, source and target name, and references left over. Every problem
//! on every page is reported.
use std::collections::{HashMap, HashSet};

use crate::domain::{
    diagnostic::{Code, Diagnostic, Diagnostics},
    diagram,
    expand::{REF_ATTRIBUTE, cells, image_refs, pages},
    library::inner,
};

/// What the diagram holds, for reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub pages: usize,
    pub cells: usize,
}

/// Checks a normalized, expanded diagram.
pub fn check(xml: &str) -> Result<Summary, Diagnostics> {
    diagram::validate(xml)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::XmlStructure, &e)))?;
    let doc = diagram::parse(xml)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::XmlSyntax, &e)))?;
    let mut problems = Diagnostics::default();
    let mut summary = Summary::default();
    for page in pages(&doc) {
        summary.pages += 1;
        let cells = cells(page.model);
        summary.cells += cells.len();
        let mut ids = HashSet::new();
        let mut parents = HashMap::new();
        for cell in &cells {
            let id = cell.attribute("id").unwrap_or("");
            if id.is_empty() {
                problems.push(
                    Diagnostic::new(Code::XmlStructure, "a cell has no id").at(page.at(None)),
                );
            } else if !ids.insert(id) {
                problems.push(
                    Diagnostic::new(Code::IdDuplicate, format!("cell ID {id:?} is used twice"))
                        .at(page.at(Some(id))),
                );
            }
            if let Some(parent) = inner(*cell).attribute("parent") {
                parents.insert(id, parent);
            }
        }
        for cell in &cells {
            let id = cell.attribute("id");
            let inner = inner(*cell);
            for key in ["parent", "source", "target"] {
                if let Some(other) = inner.attribute(key)
                    && !ids.contains(other)
                {
                    problems.push(
                        Diagnostic::new(
                            Code::IdDangling,
                            format!("{key} {other:?} is not a cell on this page"),
                        )
                        .at(page.at(id)),
                    );
                }
            }
            if cell.attribute(REF_ATTRIBUTE).is_some() || inner.attribute(REF_ATTRIBUTE).is_some() {
                problems.push(
                    Diagnostic::new(Code::RefSyntax, "a dpng-ref was left after expansion")
                        .at(page.at(id)),
                );
            }
            if image_refs(inner.attribute("style").unwrap_or(""))
                .next()
                .is_some()
            {
                problems.push(
                    Diagnostic::new(Code::RefSyntax, "an image=lib: was left after expansion")
                        .at(page.at(id)),
                );
            }
        }
        // A parent chain must end at a cell without a parent.
        let mut reported = HashSet::new();
        for start in parents.keys() {
            let mut seen = HashSet::new();
            let mut current = *start;
            while let Some(parent) = parents.get(current) {
                if !seen.insert(current) {
                    if reported.insert(current) {
                        problems.push(
                            Diagnostic::new(
                                Code::XmlStructure,
                                format!("cell {current:?} is its own ancestor"),
                            )
                            .at(page.at(Some(current))),
                        );
                    }
                    break;
                }
                current = parent;
            }
        }
    }
    problems.into_result(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(cells: &str) -> String {
        format!(
            r#"<mxfile><diagram name="P"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{cells}</root></mxGraphModel></diagram></mxfile>"#
        )
    }

    #[test]
    fn valid_diagrams_pass() {
        let summary = check(&page(
            r#"<mxCell id="a" vertex="1" parent="1"/><object id="b" label="x"><mxCell vertex="1" parent="1"/></object><mxCell id="e" edge="1" parent="1" source="a" target="b"/>"#,
        ))
        .unwrap();
        assert_eq!(summary, Summary { pages: 1, cells: 5 });
    }

    #[test]
    fn every_problem_is_reported() {
        let problems = check(&page(
            r#"<mxCell id="a" vertex="1" parent="1"/><mxCell id="a" vertex="1" parent="1"/><mxCell id="e" edge="1" parent="1" source="x" target="a"/><mxCell id="c" vertex="1" parent="d"/><mxCell id="d" vertex="1" parent="c"/><mxCell id="i" style="image=lib:a/b" vertex="1" parent="1"/>"#,
        ))
        .unwrap_err();
        let codes: Vec<_> = problems.0.iter().map(|d| d.code).collect();
        assert!(codes.contains(&Code::IdDuplicate), "{problems}");
        assert!(codes.contains(&Code::IdDangling), "{problems}");
        assert!(codes.contains(&Code::XmlStructure), "{problems}");
        assert!(codes.contains(&Code::RefSyntax), "{problems}");
        assert_eq!(problems.0[0].location.page, Some(1));
        assert_eq!(problems.0[0].location.page_name.as_deref(), Some("P"));
    }
}
