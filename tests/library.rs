use assert_cmd::{Command, cargo::cargo_bin_cmd};
use base64::{Engine, engine::general_purpose::STANDARD};
use drawio_png_cli::domain::{diagram as document, library};
use std::{fs, io::Write};
use tempfile::tempdir;

const MODEL: &str = include_str!("fixtures/model.xml");
const MIXED: &str = include_str!("fixtures/mixed-pages.xml");

fn cli() -> Command {
    cargo_bin_cmd!("drawio-png-cli")
}

/// Compresses a model the way draw.io stores library entries.
fn compress(xml: &str) -> String {
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(urlencoding::encode(xml).as_bytes())
        .unwrap();
    STANDARD.encode(encoder.finish().unwrap())
}

const ICON: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" style="shape=image;image=data:image/svg+xml,PHN2Zy8+" vertex="1" parent="1"><mxGeometry width="144" height="72" as="geometry"/></mxCell></root></mxGraphModel>"#;

// A group with a child, a labelled object and an edge between the two vertices.
const GROUP: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="g" value="" style="group" vertex="1" connectable="0" parent="1"><mxGeometry x="10" y="20" width="100" height="40" as="geometry"/></mxCell><mxCell id="c" value="child" style="rounded=1;" vertex="1" parent="g"><mxGeometry x="5" y="5" width="30" height="30" as="geometry"/></mxCell><object id="o" label="object" note="kept"><mxCell style="ellipse;" vertex="1" parent="1"><mxGeometry x="150" y="20" width="40" height="40" as="geometry"/></mxCell></object><mxCell id="e" style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1" source="g" target="o"><mxGeometry relative="1" as="geometry"><mxPoint x="12" y="80" as="sourcePoint"/><Array as="points"><mxPoint x="120" y="90"/></Array><mxPoint x="5" y="6" as="offset"/></mxGeometry></mxCell></root></mxGraphModel>"#;

fn library_xml() -> String {
    let entries = serde_json::json!([
        {"xml": compress(ICON), "w": 144, "h": 72, "title": "Icon A"},
        {"xml": GROUP, "w": 200, "h": 80, "title": "Group\nB"},
        {"data": "data:image/png;base64,iVBORw0KGgo=", "w": 16, "h": 8, "title": "Raw", "aspect": "fixed"},
        {"data": "https://example.com/icon.svg", "w": 32, "h": 32, "title": "Linked"},
        {"xml": compress(ICON), "w": 10, "h": 10, "title": "Dup"},
        {"xml": compress(ICON), "w": 20, "h": 20, "title": "Dup"},
    ]);
    // draw.io escapes the JSON as element text.
    let text = entries
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<mxlibrary title=\"test\">{text}</mxlibrary>")
}

/// A library of the given entries, escaped as draw.io writes it.
fn library_of(entries: serde_json::Value) -> String {
    let text = entries
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<mxlibrary>{text}</mxlibrary>")
}

fn library_named(name: &str, entries: serde_json::Value) -> Vec<library::Library> {
    vec![library::Library {
        name: name.into(),
        title: None,
        entries: library::parse(library_of(entries).as_bytes()).unwrap(),
    }]
}

fn entries() -> Vec<library::Library> {
    vec![library::Library {
        name: "test".into(),
        title: Some("test".into()),
        entries: library::parse(library_xml().as_bytes()).unwrap(),
    }]
}

fn placement(x: f64, y: f64) -> library::Placement<'static> {
    library::Placement {
        page: 1,
        x,
        y,
        width: None,
        height: None,
        id: None,
        label: None,
    }
}

fn cell<'a>(doc: &'a roxmltree::Document<'a>, id: &str) -> roxmltree::Node<'a, 'a> {
    doc.descendants()
        .find(|n| n.attribute("id") == Some(id))
        .unwrap_or_else(|| panic!("no cell {id}"))
}

fn geometry<'a>(node: roxmltree::Node<'a, 'a>) -> roxmltree::Node<'a, 'a> {
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

#[test]
fn library_commands_print_libraries_details_and_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("lib.xml");
    fs::write(&path, library_xml()).unwrap();
    let run = |args: &[&str]| {
        let output = cli()
            .env_remove("DRAWIO_PNG_LIBRARY_PATH")
            .args(args)
            .arg("--library-file")
            .arg(&path)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(output).unwrap()
    };
    // Libraries: name, entry count and the <mxlibrary> title.
    assert_eq!(run(&["library", "list"]), "lib\t6\ttest\n");
    assert_eq!(run(&["library", "ls"]), "lib\t6\ttest\n");
    assert_eq!(
        run(&["library", "show", "lib"]),
        "name: lib\ntitle: test\nentries: 6\n"
    );
    // Entries: library, index, title and size; never the content.
    let all = run(&["library", "search"]);
    assert_eq!(
        all,
        "lib\t1\tIcon A\t144x72\nlib\t2\tGroup B\t200x80\nlib\t3\tRaw\t16x8\nlib\t4\tLinked\t32x32\nlib\t5\tDup\t10x10\nlib\t6\tDup\t20x20\n"
    );
    assert!(!all.contains("data:"));
    assert_eq!(
        run(&["library", "search", "ICON"]),
        "lib\t1\tIcon A\t144x72\n"
    );
}

#[test]
fn insert_places_resizes_and_labels_a_single_cell() {
    let entries = entries();
    let entry = library::find(&entries, "icon a").unwrap().1;
    let mut options = placement(200.0, 30.0);
    options.width = Some(48.0);
    options.id = Some("icon");
    options.label = Some("Label & <b>");
    let (xml, ids) = library::insert(MODEL, entry, &options).unwrap();
    assert_eq!(ids, ["icon"]);
    document::validate(&xml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let icon = cell(&doc, "icon");
    assert_eq!(icon.attribute("value"), Some("Label & <b>"));
    assert_eq!(icon.attribute("parent"), Some("1"));
    assert!(
        icon.attribute("style")
            .unwrap()
            .contains("image=data:image/svg+xml,PHN2Zy8+")
    );
    let g = geometry(icon);
    // The aspect ratio is kept when only the width is given.
    let size = |n: &str| g.attribute(n).unwrap();
    assert_eq!(
        (size("x"), size("y"), size("width"), size("height")),
        ("200", "30", "48", "24")
    );
    // Existing cells are untouched.
    assert!(xml.starts_with(&MODEL[..MODEL.find("</root>").unwrap()]));
}

#[test]
fn insert_remaps_ids_parents_and_edges_of_multi_cell_entries() {
    let entries = entries();
    let entry = library::find(&entries, "Group\nB").unwrap().1;
    let (xml, ids) = library::insert(MODEL, entry, &placement(100.0, 200.0)).unwrap();
    document::validate(&xml).unwrap();
    // Top-level cells first: the group, the object and the edge.
    assert_eq!(ids, ["lib-1", "lib-3", "lib-4", "lib-2"]);
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let group = cell(&doc, "lib-1");
    let child = cell(&doc, "lib-2");
    let object = cell(&doc, "lib-3");
    let edge = cell(&doc, "lib-4");
    assert_eq!(child.attribute("parent"), Some("lib-1"));
    assert_eq!(group.attribute("parent"), Some("1"));
    assert_eq!(object.attribute("label"), Some("object"));
    assert_eq!(object.attribute("note"), Some("kept"));
    assert_eq!(
        geometry(object).parent().unwrap().attribute("parent"),
        Some("1")
    );
    assert_eq!(edge.attribute("source"), Some("lib-1"));
    assert_eq!(edge.attribute("target"), Some("lib-3"));
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
    // A second add gets fresh IDs.
    let (again, ids) = library::insert(&xml, entry, &placement(0.0, 0.0)).unwrap();
    document::validate(&again).unwrap();
    assert_eq!(ids, ["lib-5", "lib-7", "lib-8", "lib-6"]);
    // Labels and sizes need a single cell.
    let mut options = placement(0.0, 0.0);
    options.label = Some("x");
    assert!(library::insert(MODEL, entry, &options).is_err());
    let mut options = placement(0.0, 0.0);
    options.width = Some(10.0);
    assert!(library::insert(MODEL, entry, &options).is_err());
}

#[test]
fn insert_turns_image_entries_into_image_cells() {
    let entries = entries();
    let entry = library::find(&entries, "Raw").unwrap().1;
    let (xml, ids) = library::insert(MODEL, entry, &placement(1.0, 2.0)).unwrap();
    document::validate(&xml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let image = cell(&doc, &ids[0]);
    // A style value cannot contain ';', so ";base64" is dropped like draw.io does.
    let style = image.attribute("style").unwrap();
    assert!(
        style.contains("image=data:image/png,iVBORw0KGgo="),
        "{style}"
    );
    assert!(style.contains("aspect=fixed"));
    let g = geometry(image);
    assert_eq!(
        (g.attribute("width"), g.attribute("height")),
        (Some("16"), Some("8"))
    );
    // Linked images are kept as URLs; rendering them needs --allow-network.
    let entry = library::find(&entries, "Linked").unwrap().1;
    let (xml, ids) = library::insert(MODEL, entry, &placement(0.0, 0.0)).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let style = cell(&doc, &ids[0]).attribute("style").unwrap();
    assert!(
        style.contains("image=https://example.com/icon.svg"),
        "{style}"
    );
}

#[test]
fn insert_targets_pages_and_expands_compressed_ones() {
    let entries = entries();
    let entry = library::find(&entries, "Icon A").unwrap().1;
    for page in [1, 2] {
        let mut options = placement(0.0, 0.0);
        options.page = page;
        let (xml, ids) = library::insert(MIXED, entry, &options).unwrap();
        document::validate(&xml).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let pages: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("diagram"))
            .collect();
        assert_eq!(pages.len(), 2);
        for (index, diagram) in pages.iter().enumerate() {
            let has = diagram
                .descendants()
                .any(|n| n.attribute("id") == Some(ids[0].as_str()));
            assert_eq!(has, index + 1 == page, "page {page}");
        }
    }
    let mut options = placement(0.0, 0.0);
    options.page = 3;
    assert!(library::insert(MIXED, entry, &options).is_err());
}

#[test]
fn name_selection_requires_a_unique_match() {
    let entries = entries();
    assert!(library::find(&entries, "Dup").is_err());
    assert!(library::find(&entries, "missing").is_err());
    assert_eq!(library::find(&entries, "ICON A").unwrap().1.title, "Icon A");
}

#[test]
fn cli_insert_prints_ids_and_keeps_output_on_errors() {
    let dir = tempdir().unwrap();
    let lib = dir.path().join("lib.xml");
    fs::write(&lib, library_xml()).unwrap();
    let output = dir.path().join("out.xml");
    let result = cli()
        .arg("insert")
        .arg("--library-file")
        .arg(&lib)
        .args(["--name", "Icon A", "--id", "icon", "--x", "5", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(String::from_utf8(result).unwrap(), "icon\n");
    let saved = fs::read(&output).unwrap();
    // Reusing the ID, an ambiguous name and a bad index all fail without writing.
    for args in [
        vec!["--name", "Icon A", "--id", "icon"],
        vec!["--name", "Dup"],
        vec!["--index", "9"],
        vec!["--index", "1", "--width", "0"],
    ] {
        cli()
            .arg("insert")
            .arg("--library-file")
            .arg(&lib)
            .args(&args)
            .arg("-i")
            .arg(&output)
            .arg("-o")
            .arg(&output)
            .assert()
            .code(1);
        assert_eq!(fs::read(&output).unwrap(), saved, "{args:?}");
    }
    cli()
        .arg("insert")
        .arg("--library-file")
        .arg(&lib)
        .args(["--name", "Icon A", "--index", "1", "-o"])
        .arg(&output)
        .assert()
        .code(2);
    // The entry content never reaches stdout.
    let list = cli()
        .arg("insert")
        .arg("--library-file")
        .arg(&lib)
        .args(["--index", "3", "-i"])
        .arg(&output)
        .arg("-o")
        .arg(&output)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!String::from_utf8(list).unwrap().contains("data:"));
    document::validate(&fs::read_to_string(&output).unwrap()).unwrap();
}

#[test]
fn invalid_libraries_are_rejected() {
    for text in [
        "<mxlibrary>not json</mxlibrary>",
        "<other>[]</other>",
        r#"<mxlibrary>[{"title":"no content"}]</mxlibrary>"#,
    ] {
        assert!(library::parse(text.as_bytes()).is_err(), "{text}");
    }
    let broken = r#"<mxlibrary>[{"xml":"not base64!","title":"x"}]</mxlibrary>"#;
    let entries = library::parse(broken.as_bytes()).unwrap();
    assert!(library::insert(MODEL, &entries[0], &placement(0.0, 0.0)).is_err());
}

#[test]
fn search_path_finds_libraries_and_selects_by_library_name() {
    let dir = tempdir().unwrap();
    let (icons, extra) = (dir.path().join("icons"), dir.path().join("extra"));
    fs::create_dir_all(&icons).unwrap();
    fs::create_dir_all(&extra).unwrap();
    fs::write(icons.join("main.xml"), library_xml()).unwrap();
    fs::write(icons.join("broken.xml"), "<mxlibrary>not json</mxlibrary>").unwrap();
    fs::write(icons.join("notes.txt"), "ignored").unwrap();
    let other = r#"<mxlibrary>[{"xml":"<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"2\" vertex=\"1\" parent=\"1\"><mxGeometry width=\"5\" height=\"5\" as=\"geometry\"/></mxCell></root></mxGraphModel>","w":5,"h":5,"title":"Icon A"}]</mxlibrary>"#;
    fs::write(
        extra.join("other.xml"),
        other
            .replace('<', "&lt;")
            .replacen("&lt;mxlibrary>", "<mxlibrary>", 1)
            .replace("&lt;/mxlibrary>", "</mxlibrary>"),
    )
    .unwrap();
    let search = std::env::join_paths([&icons, &extra.join("other.xml")]).unwrap();
    let run = |args: &[&str]| {
        let output = cli()
            .env("DRAWIO_PNG_LIBRARY_PATH", &search)
            .args(args)
            .output()
            .unwrap();
        (
            output.status.code(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    };
    // Directories contribute their *.xml files; broken ones are skipped with a warning.
    let (code, stdout, stderr) = run(&["library", "list", "--no-builtin"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "main\t6\ttest\nother\t1\t-\n");
    // Built-in libraries follow those on DRAWIO_PNG_LIBRARY_PATH.
    let (_, all, _) = run(&["library", "list"]);
    assert!(all.starts_with(&stdout) && all.contains("\ndrawio/aws4-compute\t"));
    let (code, stdout, stderr) = run(&["library", "search", "icon", "--no-builtin"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "main\t1\tIcon A\t144x72\nother\t1\tIcon A\t5x5\n");
    assert!(stderr.contains("warning: skipped invalid library") && stderr.contains("broken.xml"));
    let (_, stdout, _) = run(&["library", "search", "--library", "other"]);
    assert_eq!(stdout, "other\t1\tIcon A\t5x5\n");
    let (code, _, stderr) = run(&["library", "show", "missing"]);
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains("available: main, other, drawio/* (see `drawio-png-cli library list`)"),
        "{stderr}"
    );

    let output = dir.path().join("out.xml");
    fs::write(&output, MODEL).unwrap();
    let path = output.to_str().unwrap();
    // A title in several libraries needs --library; --index needs one library.
    let (code, _, stderr) = run(&["insert", "--name", "Icon A", "-i", path, "-o", path]);
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains(r#""Icon A" is in libraries "main", "other"; use --library"#),
        "{stderr}"
    );
    let (code, _, stderr) = run(&["insert", "--index", "1", "-i", path, "-o", path]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("--index needs one library"), "{stderr}");
    assert_eq!(fs::read_to_string(&output).unwrap(), MODEL);
    let (code, stdout, stderr) = run(&[
        "insert",
        "--library",
        "other",
        "--name",
        "icon a",
        "--id",
        "o",
        "-i",
        path,
        "-o",
        path,
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "o\n");
    let (code, stdout, _) = run(&[
        "insert",
        "--library",
        "main",
        "--index",
        "2",
        "-i",
        path,
        "-o",
        path,
    ]);
    assert_eq!(code, Some(0));
    assert_eq!(stdout.lines().count(), 4);
    document::validate(&fs::read_to_string(&output).unwrap()).unwrap();
}

#[test]
fn library_files_and_names_are_required() {
    // Without a file or the built-in libraries, DRAWIO_PNG_LIBRARY_PATH is needed;
    // a file excludes --library and --no-builtin.
    let output = cli()
        .env_remove("DRAWIO_PNG_LIBRARY_PATH")
        .args(["library", "list", "--no-builtin"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("set DRAWIO_PNG_LIBRARY_PATH"));
    cli()
        .args([
            "library",
            "search",
            "--library-file",
            "lib.xml",
            "--library",
            "lib",
        ])
        .assert()
        .code(2);
    cli()
        .args([
            "library",
            "list",
            "--library-file",
            "lib.xml",
            "--no-builtin",
        ])
        .assert()
        .code(2);
    cli()
        .env("DRAWIO_PNG_LIBRARY_PATH", "/nonexistent/library.xml")
        .args(["library", "list"])
        .assert()
        .code(1);
}

#[test]
fn label_changes_only_the_top_level_cell() {
    // One top-level group whose children keep their own labels.
    let group = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="g" value="Group" style="group" vertex="1" connectable="0" parent="1"><mxGeometry width="100" height="40" as="geometry"/></mxCell><mxCell id="a" value="Child A" vertex="1" parent="g"><mxGeometry width="40" height="40" as="geometry"/></mxCell><object id="b" label="Child B"><mxCell vertex="1" parent="g"><mxGeometry x="50" width="40" height="40" as="geometry"/></mxCell></object><mxCell id="c" vertex="1" parent="g"><mxGeometry x="90" width="10" height="10" as="geometry"/></mxCell></root></mxGraphModel>"#;
    let libraries = library_named(
        "g",
        serde_json::json!([{"xml": group, "w": 100, "h": 40, "title": "G"}]),
    );
    let mut options = placement(0.0, 0.0);
    options.id = Some("new-group");
    options.label = Some("New Group");
    let (xml, ids) =
        library::insert(MODEL, library::find(&libraries, "G").unwrap().1, &options).unwrap();
    document::validate(&xml).unwrap();
    assert_eq!(
        ids,
        ["new-group", "new-group-1", "new-group-2", "new-group-3"]
    );
    let doc = roxmltree::Document::parse(&xml).unwrap();
    assert_eq!(
        cell(&doc, "new-group").attribute("value"),
        Some("New Group")
    );
    assert_eq!(
        cell(&doc, "new-group-1").attribute("value"),
        Some("Child A")
    );
    assert_eq!(
        cell(&doc, "new-group-2").attribute("label"),
        Some("Child B")
    );
    // A child without a label does not get one.
    assert_eq!(cell(&doc, "new-group-3").attribute("value"), None);
}

#[test]
fn edge_ports_follow_the_new_cell_ids() {
    let ports = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" vertex="1" parent="1"><mxGeometry width="100" height="40" as="geometry"/></mxCell><mxCell id="p" style="port" vertex="1" parent="v"><mxGeometry x="1" y="0.5" width="4" height="4" relative="1" as="geometry"/></mxCell><mxCell id="w" vertex="1" parent="1"><mxGeometry x="200" width="100" height="40" as="geometry"/></mxCell><mxCell id="q" style="port" vertex="1" parent="w"><mxGeometry y="0.5" width="4" height="4" relative="1" as="geometry"/></mxCell><mxCell id="e" style="sourcePort=p;targetPort=q;endArrow=none;entryX=0;" edge="1" parent="1" source="v" target="w"><mxGeometry relative="1" as="geometry"/></mxCell></root></mxGraphModel>"#;
    let libraries = library_named(
        "p",
        serde_json::json!([{"xml": ports, "w": 300, "h": 40, "title": "P"}]),
    );
    let (xml, ids) = library::insert(
        MODEL,
        library::find(&libraries, "P").unwrap().1,
        &placement(0.0, 0.0),
    )
    .unwrap();
    document::validate(&xml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let id = |old: &str| {
        // Cells keep their order, so map by position in the entry.
        let order = ["v", "p", "w", "q", "e"];
        let mut sorted = ids.clone();
        sorted.sort_by_key(|i| i.trim_start_matches("lib-").parse::<u32>().unwrap());
        sorted[order.iter().position(|o| *o == old).unwrap()].clone()
    };
    let edge = cell(&doc, &id("e"));
    assert_eq!(
        edge.attribute("style"),
        Some(
            format!(
                "sourcePort={};targetPort={};endArrow=none;entryX=0;",
                id("p"),
                id("q")
            )
            .as_str()
        )
    );
    assert_eq!(edge.attribute("source"), Some(id("v").as_str()));
    // Styles without ports, and ports outside the entry, stay as they are.
    let other = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="e" style="sourcePort=elsewhere;rounded=1" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="1" y="2" as="sourcePoint"/><mxPoint x="3" y="4" as="targetPoint"/></mxGeometry></mxCell></root></mxGraphModel>"#;
    let libraries = library_named(
        "o",
        serde_json::json!([{"xml": other, "w": 2, "h": 2, "title": "O"}]),
    );
    let (xml, ids) = library::insert(
        MODEL,
        library::find(&libraries, "O").unwrap().1,
        &placement(0.0, 0.0),
    )
    .unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    assert_eq!(
        cell(&doc, &ids[0]).attribute("style"),
        Some("sourcePort=elsewhere;rounded=1")
    );
}

#[test]
fn library_names_come_from_unicode_file_names_and_positions_can_be_negative() {
    let dir = tempdir().unwrap();
    let text = library_of(
        serde_json::json!([{"xml": compress(ICON), "w": 144, "h": 72, "title": "Icon A"}]),
    );
    // A non-ASCII name without .xml must not be cut inside a character.
    for (file, name) in [
        ("図形集", "図形集"),
        ("図形集.xml", "図形集"),
        ("形.XML", "形"),
    ] {
        let path = dir.path().join(file);
        fs::write(&path, &text).unwrap();
        let output = cli()
            .args(["library", "list", "--library-file"])
            .arg(&path)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("{name}\t1\t-\n"),
            "{file}"
        );
    }
    // Negative coordinates work as separate values.
    let output = dir.path().join("out.xml");
    cli()
        .arg("insert")
        .arg("--library-file")
        .arg(dir.path().join("図形集.xml"))
        .args([
            "--name", "Icon A", "--id", "neg", "--x", "-10", "--y", "-20.5", "-o",
        ])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    let xml = fs::read_to_string(&output).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let g = geometry(cell(&doc, "neg"));
    assert_eq!(
        (g.attribute("x"), g.attribute("y")),
        (Some("-10"), Some("-20.5"))
    );
}
