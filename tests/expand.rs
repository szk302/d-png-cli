//! Expanding `libref` placeholders and `image=lib:` values (spec 4.3, 4.4).
mod support;

use drawio_png_cli::{
    app::{diagram, libraries::Libraries},
    domain::{
        diagnostic::{Code, Diagnostics},
        library::{self, Origin},
        reference::{LibraryName, fingerprint},
    },
};
use support::{Memory, cell, compress, geometry, library};

const ICON: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" style="shape=image;image=data:image/svg+xml,PHN2Zy8+" vertex="1" parent="1"><mxGeometry width="144" height="72" as="geometry"/></mxCell></root></mxGraphModel>"#;

// A group with a child, a labelled object and an edge between the two vertices.
const GROUP: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="g" value="" style="group" vertex="1" connectable="0" parent="1"><mxGeometry x="10" y="20" width="100" height="40" as="geometry"/></mxCell><mxCell id="c" value="child" style="rounded=1;" vertex="1" parent="g"><mxGeometry x="5" y="5" width="30" height="30" as="geometry"/></mxCell><object id="o" label="object" note="kept"><mxCell style="ellipse;" vertex="1" parent="1"><mxGeometry x="150" y="20" width="40" height="40" as="geometry"/></mxCell></object><mxCell id="e" style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1" source="g" target="o"><mxGeometry relative="1" as="geometry"><mxPoint x="12" y="80" as="sourcePoint"/><Array as="points"><mxPoint x="120" y="90"/></Array><mxPoint x="5" y="6" as="offset"/></mxGeometry></mxCell></root></mxGraphModel>"#;

// One top-level group whose children keep their own labels.
const CARD: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="g" value="Group" style="group" vertex="1" connectable="0" parent="1"><mxGeometry width="100" height="40" as="geometry"/></mxCell><mxCell id="a" value="Child A" vertex="1" parent="g"><mxGeometry width="40" height="40" as="geometry"/></mxCell><object id="b" label="Child B"><mxCell vertex="1" parent="g"><mxGeometry x="50" width="40" height="40" as="geometry"/></mxCell></object><mxCell id="c" vertex="1" parent="g"><mxGeometry x="90" width="10" height="10" as="geometry"/></mxCell></root></mxGraphModel>"#;

const PORTS: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" vertex="1" parent="1"><mxGeometry width="100" height="40" as="geometry"/></mxCell><mxCell id="p" style="port" vertex="1" parent="v"><mxGeometry x="1" y="0.5" width="4" height="4" relative="1" as="geometry"/></mxCell><mxCell id="w" vertex="1" parent="1"><mxGeometry x="200" width="100" height="40" as="geometry"/></mxCell><mxCell id="q" style="port" vertex="1" parent="w"><mxGeometry y="0.5" width="4" height="4" relative="1" as="geometry"/></mxCell><mxCell id="e" style="sourcePort=p;targetPort=q;endArrow=none;entryX=0;" edge="1" parent="1" source="v" target="w"><mxGeometry relative="1" as="geometry"/></mxCell></root></mxGraphModel>"#;

const LINE: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="e" value="" style="endArrow=classic;" edge="1" parent="1"><mxGeometry width="50" height="50" relative="1" as="geometry"><mxPoint x="10" y="60" as="sourcePoint"/><mxPoint x="60" y="10" as="targetPoint"/></mxGeometry></mxCell></root></mxGraphModel>"#;

const OBJECT: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><object id="2" label="Service" owner="library" note="keep"><mxCell style="rounded=1;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell></object></root></mxGraphModel>"#;

fn project() -> Memory {
    Memory {
        origin: Origin::Project,
        libraries: vec![library(
            "lib",
            serde_json::json!([
                {"id": "icon", "xml": compress(ICON), "w": 144, "h": 72, "title": "Icon A"},
                {"id": "group", "xml": GROUP, "w": 200, "h": 80, "title": "Group\nB"},
                {"id": "raw", "data": "data:image/png;base64,iVBORw0KGgo=", "w": 16, "h": 8, "title": "Raw", "aspect": "fixed"},
                {"id": "linked", "data": "https://example.com/icon.svg", "w": 32, "h": 32, "title": "Linked"},
                {"id": "card", "xml": CARD, "w": 100, "h": 40, "title": "Card"},
                {"id": "ports", "xml": PORTS, "w": 300, "h": 40, "title": "Ports"},
                {"id": "line", "xml": LINE, "w": 50, "h": 50, "title": "Line"},
                {"id": "object", "xml": OBJECT, "w": 120, "h": 60, "title": "Object"},
                {"id": "broken", "xml": "not base64!", "title": "Broken"},
            ]),
        )],
    }
}

fn page(cells: &str) -> String {
    format!(
        r#"<mxfile><diagram name="P" id="p"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{cells}</root></mxGraphModel></diagram></mxfile>"#
    )
}

fn expand(cells: &str) -> Result<String, Diagnostics> {
    let project = project();
    let libraries = Libraries::new(vec![&project]);
    diagram::expand(page(cells).as_bytes(), None, &libraries).map(|e| e.xml)
}

#[track_caller]
fn codes(cells: &str) -> Vec<Code> {
    expand(cells)
        .expect_err("expected problems")
        .0
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn a_single_cell_is_placed_resized_and_relabelled() {
    let xml = expand(
        r#"<mxCell id="before" vertex="1" parent="1"/><mxCell id="icon" value="Label &amp; &lt;b&gt;" parent="1" libref="lib/icon"><mxGeometry x="200" y="30" width="48" as="geometry"/></mxCell>"#,
    )
    .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let icon = cell(&doc, "icon");
    assert_eq!(icon.attribute("value"), Some("Label & <b>"));
    assert_eq!(icon.attribute("parent"), Some("1"));
    assert_eq!(icon.attribute("vertex"), Some("1"));
    assert_eq!(icon.attribute("libref"), None);
    assert!(
        icon.attribute("style")
            .unwrap()
            .contains("image=data:image/svg+xml,PHN2Zy8+")
    );
    // The aspect ratio is kept when only the width is given.
    let g = geometry(icon);
    let size = |n: &str| g.attribute(n).unwrap();
    assert_eq!(
        (size("x"), size("y"), size("width"), size("height")),
        ("200", "30", "48", "24")
    );
    // Other cells are untouched.
    assert!(xml.contains(r#"<mxCell id="before" vertex="1" parent="1"/>"#));
    // Negative positions work.
    let xml = expand(
        r#"<mxCell id="neg" parent="1" libref="lib/icon"><mxGeometry x="-10" y="-20.5" as="geometry"/></mxCell>"#,
    )
    .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let g = geometry(cell(&doc, "neg"));
    assert_eq!(
        (g.attribute("x"), g.attribute("y")),
        (Some("-10"), Some("-20.5"))
    );
}

#[test]
fn a_set_gets_numbered_ids_with_parents_and_edges_remapped() {
    let project = project();
    let libraries = Libraries::new(vec![&project]);
    let expanded = diagram::expand(
        page(r#"<mxCell id="grp" parent="1" libref="lib/group"><mxGeometry x="100" y="200" as="geometry"/></mxCell>"#)
            .as_bytes(),
        None,
        &libraries,
    )
    .unwrap();
    // Top-level cells first: the group, the object and the edge.
    match &expanded.expanded[0].result {
        drawio_png_cli::domain::expand::Expanded::Cells(ids) => {
            assert_eq!(ids, &["grp-1", "grp-3", "grp-4", "grp-2"])
        }
        other => panic!("{other:?}"),
    }
    let doc = roxmltree::Document::parse(&expanded.xml).unwrap();
    assert!(doc.descendants().all(|n| n.attribute("id") != Some("grp")));
    let group = cell(&doc, "grp-1");
    let child = cell(&doc, "grp-2");
    let object = cell(&doc, "grp-3");
    let edge = cell(&doc, "grp-4");
    assert_eq!(child.attribute("parent"), Some("grp-1"));
    assert_eq!(group.attribute("parent"), Some("1"));
    assert_eq!(object.attribute("label"), Some("object"));
    assert_eq!(object.attribute("note"), Some("kept"));
    assert_eq!(
        geometry(object).parent().unwrap().attribute("parent"),
        Some("1")
    );
    assert_eq!(edge.attribute("source"), Some("grp-1"));
    assert_eq!(edge.attribute("target"), Some("grp-3"));
    // The entry's top-left corner (10, 20) moves to (100, 200); child
    // coordinates are relative to the group and stay.
    let xy = |n: roxmltree::Node<'_, '_>| {
        (
            n.attribute("x").unwrap().to_owned(),
            n.attribute("y").unwrap().to_owned(),
        )
    };
    assert_eq!(xy(geometry(group)), ("100".into(), "200".into()));
    assert_eq!(xy(geometry(object)), ("240".into(), "200".into()));
    assert_eq!(xy(geometry(child)), ("5".into(), "5".into()));
    let points: Vec<_> = geometry(edge)
        .descendants()
        .filter(|n| n.has_tag_name("mxPoint"))
        .map(xy)
        .collect();
    assert_eq!(
        points,
        [
            ("102".into(), "260".into()),
            ("210".into(), "270".into()),
            ("5".into(), "6".into()) // the label offset is relative
        ]
    );
    // A set takes no label, style, custom data or size.
    for placeholder in [
        r#"<mxCell id="s" value="x" parent="1" libref="lib/group"/>"#,
        r#"<mxCell id="s" style="fillColor=red;" parent="1" libref="lib/group"/>"#,
        r#"<object id="s" owner="me" libref="lib/group"><mxCell parent="1"/></object>"#,
        r#"<mxCell id="s" parent="1" libref="lib/group"><mxGeometry width="10" as="geometry"/></mxCell>"#,
    ] {
        assert!(
            codes(placeholder).contains(&Code::RefOverride),
            "{placeholder}"
        );
    }
}

#[test]
fn expansion_ids_never_collide() {
    // A generated ID already on the page.
    assert_eq!(
        codes(
            r#"<mxCell id="card-1" vertex="1" parent="1"/><mxCell id="card" parent="1" libref="lib/card"/>"#
        ),
        [Code::RefIdConflict]
    );
    // A set loses its placeholder ID, so nothing may refer to it.
    assert_eq!(
        codes(
            r#"<mxCell id="grp" parent="1" libref="lib/group"/><mxCell id="x" edge="1" parent="1" source="grp"/>"#
        ),
        [Code::RefIdConflict]
    );
    // The same entry twice gets distinct IDs from distinct placeholders.
    let xml = expand(
        r#"<mxCell id="one" parent="1" libref="lib/card"/><mxCell id="two" parent="1" libref="lib/card"><mxGeometry x="200" as="geometry"/></mxCell>"#,
    )
    .unwrap();
    for id in ["one", "one-1", "one-3", "two", "two-1", "two-3"] {
        assert!(xml.contains(&format!("id=\"{id}\"")), "{id}");
    }
    // Generated IDs can be connected to.
    expand(
        r#"<mxCell id="card" parent="1" libref="lib/card"/><mxCell id="e" edge="1" parent="1" source="card-1" target="card"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
    )
    .unwrap();
}

#[test]
fn a_label_changes_only_the_top_level_cell() {
    let xml =
        expand(r#"<mxCell id="new" value="New Group" parent="1" libref="lib/card"/>"#).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    assert_eq!(cell(&doc, "new").attribute("value"), Some("New Group"));
    assert_eq!(cell(&doc, "new-1").attribute("value"), Some("Child A"));
    assert_eq!(cell(&doc, "new-2").attribute("label"), Some("Child B"));
    // A child without a label does not get one.
    assert_eq!(cell(&doc, "new-3").attribute("value"), None);
}

#[test]
fn image_entries_become_image_cells() {
    let xml = expand(
        r#"<mxCell id="raw" parent="1" libref="lib/raw"><mxGeometry x="1" y="2" as="geometry"/></mxCell><mxCell id="linked" parent="1" libref="lib/linked"/>"#,
    )
    .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    // A style value cannot contain ';', so ";base64" is dropped like draw.io does.
    let style = cell(&doc, "raw").attribute("style").unwrap();
    assert!(
        style.contains("image=data:image/png,iVBORw0KGgo="),
        "{style}"
    );
    assert!(style.contains("aspect=fixed"));
    let g = geometry(cell(&doc, "raw"));
    assert_eq!(
        (g.attribute("width"), g.attribute("height")),
        (Some("16"), Some("8"))
    );
    // Linked images are kept as URLs; rendering them needs --allow-network.
    let style = cell(&doc, "linked").attribute("style").unwrap();
    assert!(
        style.contains("image=https://example.com/icon.svg"),
        "{style}"
    );
}

#[test]
fn image_references_replace_only_the_image() {
    let xml = expand(
        r#"<mxCell id="a" value="A" style="shape=image;image=lib:lib/raw;aspect=fixed;" vertex="1" parent="1"><mxGeometry width="20" height="10" as="geometry"/></mxCell>"#,
    )
    .unwrap();
    assert!(
        xml.contains(r#"style="shape=image;image=data:image/png,iVBORw0KGgo=;aspect=fixed;""#),
        "{xml}"
    );
    // A fingerprint finds the image even under another ID; a changed image is reported.
    let print = fingerprint("data:image/png,iVBORw0KGgo=");
    expand(&format!(
        r#"<mxCell id="a" style="image=lib:lib/renamed@{print};" vertex="1" parent="1"/>"#
    ))
    .unwrap();
    assert_eq!(
        codes(r#"<mxCell id="a" style="image=lib:lib/raw@000000000000;" vertex="1" parent="1"/>"#),
        [Code::ImageChanged]
    );
    // Only image entries hold images.
    assert_eq!(
        codes(r#"<mxCell id="a" style="image=lib:lib/card;" vertex="1" parent="1"/>"#),
        [Code::RefKind]
    );
}

#[test]
fn edge_ports_follow_the_new_cell_ids() {
    let xml = expand(r#"<mxCell id="pp" parent="1" libref="lib/ports"/>"#).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    // v, p, w, q, e in document order.
    let edge = cell(&doc, "pp-5");
    assert_eq!(
        edge.attribute("style"),
        Some("sourcePort=pp-2;targetPort=pp-4;endArrow=none;entryX=0;")
    );
    assert_eq!(edge.attribute("source"), Some("pp-1"));
    assert_eq!(edge.attribute("target"), Some("pp-3"));
}

#[test]
fn edges_connect_to_the_placeholder_ends() {
    // Both ends: the placeholder geometry replaces the entry's points.
    let xml = expand(
        r#"<mxCell id="a" vertex="1" parent="1"/><mxCell id="b" vertex="1" parent="1"/><mxCell id="l" value="uses" parent="1" source="a" target="b" libref="lib/line"><mxGeometry relative="1" as="geometry"><Array as="points"><mxPoint x="5" y="5"/></Array></mxGeometry></mxCell>"#,
    )
    .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let line = cell(&doc, "l");
    assert_eq!(
        (
            line.attribute("source"),
            line.attribute("target"),
            line.attribute("value")
        ),
        (Some("a"), Some("b"), Some("uses"))
    );
    let points: Vec<_> = geometry(line)
        .descendants()
        .filter(|n| n.has_tag_name("mxPoint"))
        .map(|n| n.attribute("as"))
        .collect();
    assert_eq!(points, [None]);
    // One end: the other end keeps the entry's point, moved into place.
    let xml = expand(
        r#"<mxCell id="a" vertex="1" parent="1"/><mxCell id="l" parent="1" source="a" libref="lib/line"><mxGeometry x="100" y="100" as="geometry"/></mxCell>"#,
    )
    .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let ends: Vec<_> = geometry(cell(&doc, "l"))
        .children()
        .filter(|n| n.has_tag_name("mxPoint"))
        .map(|n| (n.attribute("as").unwrap(), n.attribute("x").unwrap()))
        .collect();
    assert_eq!(ends, [("targetPoint", "150")]);
    // Ends need an edge entry.
    assert_eq!(
        codes(
            r#"<mxCell id="a" vertex="1" parent="1"/><mxCell id="x" parent="1" source="a" libref="lib/card"/>"#
        ),
        [Code::RefOverride]
    );
}

#[test]
fn wrappers_and_custom_data_merge() {
    // The placeholder's custom data wins; the entry's stays; no double wrapper.
    let xml = expand(
        r#"<object id="s" label="Payments" owner="payments" libref="lib/object"><mxCell style="fillColor=#dae8fc;" parent="1"><mxGeometry x="200" y="40" as="geometry"/></mxCell></object>"#,
    )
    .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let object = cell(&doc, "s");
    assert_eq!(object.tag_name().name(), "object");
    assert_eq!(object.attribute("label"), Some("Payments"));
    assert_eq!(object.attribute("owner"), Some("payments"));
    assert_eq!(object.attribute("note"), Some("keep"));
    assert_eq!(
        object
            .descendants()
            .filter(|n| n.has_tag_name("object"))
            .count(),
        1
    );
    let inner = object.first_element_child().unwrap();
    assert_eq!(
        inner.attribute("style"),
        Some("rounded=1;fillColor=#dae8fc;")
    );
    assert_eq!(inner.attribute("id"), None);
    // A plain placeholder relabels the entry's wrapper.
    let xml = expand(r#"<mxCell id="s" value="Renamed" parent="1" libref="lib/object"/>"#).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    assert_eq!(cell(&doc, "s").attribute("label"), Some("Renamed"));
    // A wrapper around a plain entry carries its label.
    let xml = expand(r#"<UserObject id="u" label="Tagged" tag="x" libref="lib/icon"><mxCell parent="1"/></UserObject>"#)
        .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let wrapper = cell(&doc, "u");
    assert_eq!(wrapper.tag_name().name(), "UserObject");
    assert_eq!(wrapper.attribute("label"), Some("Tagged"));
    assert_eq!(wrapper.attribute("tag"), Some("x"));
    assert_eq!(
        wrapper.first_element_child().unwrap().attribute("value"),
        None
    );
}

#[test]
fn misuse_of_an_entry_is_reported_with_codes() {
    assert_eq!(
        codes(r#"<mxCell id="x" vertex="1" parent="1" libref="lib/line"/>"#),
        [Code::RefKind]
    );
    assert_eq!(
        codes(r#"<mxCell id="x" edge="1" parent="1" libref="lib/icon"/>"#),
        [Code::RefKind]
    );
    assert_eq!(
        codes(
            r#"<mxCell id="x" parent="1" libref="lib/card"><mxGeometry width="10" as="geometry"/></mxCell>"#
        ),
        [Code::RefOverride]
    );
    assert_eq!(
        codes(r#"<mxCell id="x" style="ellipse" parent="1" libref="lib/icon"/>"#),
        [Code::RefOverride]
    );
    assert_eq!(
        codes(r#"<mxCell id="x" parent="1" libref="lib/icon"><mxPoint as="extra"/></mxCell>"#),
        [Code::RefOverride]
    );
    assert_eq!(
        codes(r#"<mxCell id="x" parent="1" libref="lib/broken"/>"#),
        [Code::RefKind]
    );
    assert_eq!(
        codes(r#"<mxCell id="x" parent="1" libref="Lib/icon"/>"#),
        [Code::RefSyntax]
    );
    // Every missing reference is reported at once, with where it is.
    let problems = expand(
        r#"<mxCell id="x" parent="1" libref="lib/missing"/><mxCell id="y" parent="1" libref="other/thing"/>"#,
    )
    .unwrap_err();
    assert_eq!(problems.0.len(), 2);
    assert!(problems.0.iter().all(|d| d.code == Code::RefNotFound));
    assert_eq!(problems.0[0].location.cell.as_deref(), Some("x"));
    assert_eq!(problems.0[0].location.page_name.as_deref(), Some("P"));
}

#[test]
fn placeholders_on_any_page_expand_there() {
    let two = r#"<mxfile><diagram name="A"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram><diagram name="B"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="i" parent="1" libref="lib/icon"/></root></mxGraphModel></diagram></mxfile>"#;
    let project = project();
    let libraries = Libraries::new(vec![&project]);
    let expanded = diagram::expand(two.as_bytes(), None, &libraries).unwrap();
    assert_eq!(expanded.pages, 2);
    assert_eq!(expanded.expanded[0].location.page, Some(2));
    let doc = roxmltree::Document::parse(&expanded.xml).unwrap();
    let pages: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("diagram"))
        .collect();
    assert!(
        !pages[0]
            .descendants()
            .any(|n| n.attribute("id") == Some("i"))
    );
    assert!(
        pages[1]
            .descendants()
            .any(|n| n.attribute("id") == Some("i"))
    );
}

#[test]
fn libraries_parse_in_every_form_draw_io_writes() {
    let name = || LibraryName::parse("lib").unwrap();
    for text in [
        "<mxlibrary>not json</mxlibrary>",
        "<other>[]</other>",
        r#"<mxlibrary>[{"title":"no content"}]</mxlibrary>"#,
    ] {
        assert!(library::parse(name(), text.as_bytes()).is_err(), "{text}");
    }
    let json = r#"[{"title":"A","xml":"<mxGraphModel/>"}]"#;
    for text in [
        format!("<mxlibrary><![CDATA[{json}]]></mxlibrary>"),
        format!(
            "<mxlibrary>{}</mxlibrary>",
            json.replace('<', "&lt;").replace('>', "&gt;")
        ),
        format!(
            "<mxlibrary>{}</mxlibrary>",
            json.replace('<', r"\u003c").replace('>', r"\u003e")
        ),
    ] {
        let parsed = library::parse(name(), text.as_bytes()).unwrap();
        assert_eq!(parsed.entries[0].id.as_str(), "a", "{text}");
        assert!(parsed.entries[0].graph_model().unwrap().starts_with('<'));
    }
}
