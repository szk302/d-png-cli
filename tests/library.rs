//! Library commands (spec 9.7-9.10) over a project area.
mod support;

use d_png_cli::domain::{
    diagram, library, preview,
    reference::{LibraryName, Ref},
};
use std::{fs, path::Path};
use support::{cli, compress, library_file, stderr, stdout};
use tempfile::tempdir;

const ICON: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" style="shape=image;image=data:image/svg+xml,PHN2Zy8+" vertex="1" parent="1"><mxGeometry width="144" height="72" as="geometry"/></mxCell></root></mxGraphModel>"#;
const CARD: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><object id="g" label="Card" owner="team"><mxCell style="group;" vertex="1" parent="1"><mxGeometry width="100" height="40" as="geometry"/></mxCell></object><mxCell id="a" value="Title" vertex="1" parent="g"><mxGeometry width="40" height="40" as="geometry"/></mxCell></root></mxGraphModel>"#;

/// A project with one library, `icons`.
fn project() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    cli().arg("init").arg(dir.path()).assert().success();
    fs::write(
        dir.path().join(".d-png/libraries/icons.xml"),
        library_file(serde_json::json!([
            {"xml": compress(ICON), "w": 144, "h": 72, "title": "Icon A"},
            {"xml": CARD, "w": 100, "h": 40, "title": "Card"},
            {"data": "data:image/png;base64,iVBORw0KGgo=", "w": 16, "h": 8, "title": "Raw"},
            {"xml": compress(ICON), "w": 10, "h": 10, "title": "Dup"},
            {"xml": compress(ICON), "w": 20, "h": 20, "title": "Dup"},
        ]))
        .replacen("<mxlibrary>", "<mxlibrary title=\"Icons\">", 1),
    )
    .unwrap();
    // Not library files: other extensions and invalid names.
    fs::write(dir.path().join(".d-png/libraries/notes.txt"), "x").unwrap();
    fs::write(dir.path().join(".d-png/libraries/Bad Name.xml"), "x").unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    stdout(cli().current_dir(dir).args(args))
}

#[test]
fn init_creates_the_project_area_once() {
    let dir = tempdir().unwrap();
    let out = stdout(cli().arg("init").arg(dir.path()));
    assert!(out.starts_with("created "), "{out}");
    let area = dir.path().join(".d-png");
    assert!(area.join("libraries").is_dir());
    assert!(area.join("config.toml").is_file());
    assert_eq!(
        fs::read_to_string(area.join(".gitignore")).unwrap(),
        "# Libraries installed with `library install`; restored from lock.json\n/libraries/@*/\n"
    );
    fs::write(area.join("config.toml"), "# mine\n").unwrap();
    let again = stdout(cli().args(["init", "--json"]).arg(dir.path()));
    assert!(again.contains("\"created\":false"), "{again}");
    assert_eq!(
        fs::read_to_string(area.join("config.toml")).unwrap(),
        "# mine\n"
    );
}

#[test]
fn list_shows_the_project_libraries() {
    let dir = project();
    let list = run(dir.path(), &["library", "list"]);
    assert_eq!(list, "icons\t-\t5\tIcons\n");
    let json: serde_json::Value =
        serde_json::from_str(&run(dir.path(), &["library", "list", "--json"])).unwrap();
    assert_eq!(json[0]["name"], "icons");
    assert_eq!(json[0]["origin"], "project");
    assert_eq!(json[0]["file"], ".d-png/libraries/icons.xml");
    assert_eq!(run(dir.path(), &["library", "list", "--scope", "acme"]), "");
    // A subdirectory finds the project above it; --project names it.
    let sub = dir.path().join("docs/deep");
    fs::create_dir_all(&sub).unwrap();
    assert!(run(&sub, &["library", "list"]).starts_with("icons\t"));
    let elsewhere = tempdir().unwrap();
    assert!(
        stdout(
            cli()
                .current_dir(elsewhere.path())
                .args(["library", "list", "--project"])
                .arg(dir.path())
        )
        .starts_with("icons\t")
    );
    // Without a project, there are no libraries.
    assert_eq!(run(elsewhere.path(), &["library", "list"]), "");
    let missing = stderr(
        cli()
            .current_dir(elsewhere.path())
            .env("D_PNG_PROJECT", elsewhere.path())
            .args(["library", "list"]),
        1,
    );
    assert!(missing.contains("PROJECT_NOT_FOUND"), "{missing}");
}

#[test]
fn search_prints_references_without_content() {
    let dir = project();
    let all = run(dir.path(), &["library", "search", "--library", "icons"]);
    assert_eq!(
        all,
        "icons/icon-a\timage\t144x72\tIcon A\nicons/card\tgroup\t100x40\tCard\nicons/raw\timage\t16x8\tRaw\nicons/dup\timage\t10x10\tDup\nicons/dup-2\timage\t20x20\tDup\n"
    );
    assert!(!all.contains("data:"));
    // Exact matches first, then prefixes, then the rest; every word must match.
    fs::write(
        dir.path().join(".d-png/libraries/more.xml"),
        library_file(serde_json::json!([
            {"xml": CARD, "w": 100, "h": 40, "title": "Scorecard"},
            {"xml": CARD, "w": 100, "h": 40, "title": "Cards"},
        ])),
    )
    .unwrap();
    let found: Vec<String> = run(dir.path(), &["library", "search", "card"])
        .lines()
        .map(|l| l.split('\t').next().unwrap().to_owned())
        .collect();
    assert_eq!(found, ["icons/card", "more/cards", "more/scorecard"]);
    assert!(run(dir.path(), &["library", "search", "icon", "a"]).starts_with("icons/icon-a\t"));
    assert_eq!(
        run(
            dir.path(),
            &["library", "search", "card", "--scope", "acme"]
        ),
        ""
    );
    let groups = run(
        dir.path(),
        &["library", "search", "--library", "icons", "--kind", "group"],
    );
    assert_eq!(groups, "icons/card\tgroup\t100x40\tCard\n");
    // The limit is reported on stderr.
    let limited = cli()
        .current_dir(dir.path())
        .args(["library", "search", "--limit", "2"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(String::from_utf8_lossy(&limited.stdout).lines().count(), 2);
    assert!(String::from_utf8_lossy(&limited.stderr).contains("2 of "));
    let json: serde_json::Value = serde_json::from_str(&run(
        dir.path(),
        &["library", "search", "raw", "--library", "icons", "--json"],
    ))
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!([{"ref": "icons/raw", "library": "icons", "id": "raw", "title": "Raw", "kind": "image", "width": 16.0, "height": 8.0, "origin": "project"}])
    );
}

#[test]
fn show_describes_libraries_and_entries() {
    let dir = project();
    let card: serde_json::Value = serde_json::from_str(&run(
        dir.path(),
        &["library", "show", "icons/card", "--json"],
    ))
    .unwrap();
    assert_eq!(card["kind"], "group");
    assert_eq!(card["label"], "Card");
    assert_eq!(card["style"], "group;");
    assert_eq!(card["data"], serde_json::json!({"owner": "team"}));
    assert_eq!(
        card["cells"],
        serde_json::json!([
            {"id": "<ID>", "kind": "vertex", "label": "Card", "parent": null},
            {"id": "<ID>-1", "kind": "vertex", "label": "Title", "parent": "<ID>"}
        ])
    );
    assert_eq!(
        card["example"],
        r#"<mxCell id="ID" parent="1" dpng-ref="icons/card"><mxGeometry x="0" y="0" as="geometry"/></mxCell>"#
    );
    let raw: serde_json::Value = serde_json::from_str(&run(
        dir.path(),
        &["library", "show", "icons/raw", "--json"],
    ))
    .unwrap();
    assert_eq!(raw["image"]["mediaType"], "image/png");
    assert_eq!(raw["image"]["bytes"], 8);
    // The style names the image instead of carrying it, and works as written.
    let style = raw["style"].as_str().unwrap();
    assert!(style.contains("image=lib:icons/raw;"), "{style}");
    assert!(!style.contains("data:"), "{style}");
    assert!(!run(dir.path(), &["library", "show", "icons/raw"]).contains("data:"));
    let xml = format!(
        r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="r" style="{style}" vertex="1" parent="1"><mxGeometry width="16" height="8" as="geometry"/></mxCell></root></mxGraphModel>"#
    );
    let expanded = stdout(cli().current_dir(dir.path()).arg("expand").write_stdin(xml));
    assert!(expanded.contains("image=data:image/png,"), "{expanded}");
    assert!(
        raw["example"]
            .as_str()
            .unwrap()
            .contains("width=\"16\" height=\"8\"")
    );
    let text = run(dir.path(), &["library", "show", "icons"]);
    assert!(
        text.contains("name: icons\ntitle: Icons\norigin: project\nentries: 5\n"),
        "{text}"
    );
    let icon = run(dir.path(), &["library", "show", "icons/icon-a"]);
    assert!(icon.contains("kind: image"), "{icon}");
    // Close references are suggested for mistakes.
    let error = stderr(
        cli()
            .current_dir(dir.path())
            .args(["library", "show", "icons/cart"]),
        1,
    );
    assert!(error.contains("error[REF_NOT_FOUND]"), "{error}");
    assert!(error.contains("candidates: icons/card\n"), "{error}");
    let json = stderr(
        cli()
            .current_dir(dir.path())
            .args(["library", "show", "icons/dupe", "--json"]),
        1,
    );
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["errors"][0]["code"], "REF_NOT_FOUND");
    assert_eq!(value["errors"][0]["candidates"][0], "icons/dup");
}

#[test]
fn diagrams_use_project_libraries() {
    let dir = project();
    let xml = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="i" parent="1" dpng-ref="icons/icon-a"><mxGeometry width="48" as="geometry"/></mxCell><mxCell id="c" parent="1" dpng-ref="icons/card"/></root></mxGraphModel>"#;
    let out = stdout(cli().current_dir(dir.path()).arg("expand").write_stdin(xml));
    diagram::validate(&out).unwrap();
    assert!(out.contains("owner=\"team\""));
    assert!(!out.contains("dpng-ref"));
    // Project references need a project.
    let elsewhere = tempdir().unwrap();
    let error = stderr(
        cli()
            .current_dir(elsewhere.path())
            .arg("validate")
            .write_stdin(xml),
        1,
    );
    assert!(error.contains("no project (.d-png/) here"), "{error}");
}

/// Fails when an entry of a preview overlaps a label or another entry.
fn assert_separate(xml: &str) {
    type Rect = (f64, f64, f64, f64);
    let doc = roxmltree::Document::parse(xml).unwrap();
    let mut slots: Vec<(Option<Rect>, Rect, String)> = Vec::new();
    let mut current: Option<Rect> = None;
    for node in doc.descendants() {
        let cell = match node.tag_name().name() {
            "mxCell"
                if !node
                    .parent_element()
                    .is_some_and(|p| p.tag_name().name() != "root") =>
            {
                node
            }
            "object" | "UserObject" => match node.children().find(|n| n.has_tag_name("mxCell")) {
                Some(cell) => cell,
                None => continue,
            },
            _ => continue,
        };
        if cell.attribute("parent") != Some("1") || cell.attribute("vertex") != Some("1") {
            continue;
        }
        let Some(geometry) = cell.children().find(|n| n.has_tag_name("mxGeometry")) else {
            continue;
        };
        let number = |key| {
            geometry
                .attribute(key)
                .map_or(0.0, |v: &str| v.parse().unwrap())
        };
        let (x, y, width, height) = (number("x"), number("y"), number("width"), number("height"));
        // A rotated vertex covers its bounds turned about the centre.
        let turn = cell
            .attribute("style")
            .unwrap_or("")
            .split(';')
            .filter_map(|p| p.strip_prefix("rotation="))
            .next_back()
            .map_or(0.0, |v| v.parse::<f64>().unwrap().to_radians());
        let (cos, sin) = (turn.cos().abs(), turn.sin().abs());
        let (turned_width, turned_height) =
            (width * cos + height * sin, width * sin + height * cos);
        let (cx, cy) = (x + width / 2.0, y + height / 2.0);
        let rect = (
            cx - turned_width / 2.0,
            cy - turned_height / 2.0,
            cx + turned_width / 2.0,
            cy + turned_height / 2.0,
        );
        let id = node.attribute("id").unwrap_or("");
        if id.starts_with("label") {
            slots.push((current.take(), rect, id.to_owned()));
        } else {
            current = Some(current.map_or(rect, |c| {
                (
                    c.0.min(rect.0),
                    c.1.min(rect.1),
                    c.2.max(rect.2),
                    c.3.max(rect.3),
                )
            }));
        }
    }
    let hit =
        |a: Rect, b: Rect| a.0 < b.2 - 0.5 && b.0 < a.2 - 0.5 && a.1 < b.3 - 0.5 && b.1 < a.3 - 0.5;
    // Each entry comes just before its label.
    for (i, (entry, _, title)) in slots.iter().enumerate() {
        let Some(entry) = entry else { continue };
        for (j, (other, label, other_title)) in slots.iter().enumerate() {
            // Every label, its own included, and every other entry.
            let other = other.filter(|_| i != j);
            assert!(
                !hit(*entry, *label) && !other.is_some_and(|o| hit(*entry, o)),
                "{title} overlaps {other_title}"
            );
        }
    }
}

/// Entries of every kind and shape a preview lays out: wide, tall and
/// round shapes, a group, a pool with lanes, an edge and a set.
const VARIED: &[(&str, &str, f64, f64)] = &[
    (
        "Box",
        r#"<mxCell id="2" style="rounded=1;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell>"#,
        120.0,
        60.0,
    ),
    (
        "Wide Bar",
        r#"<mxCell id="2" value="Bar" style="" vertex="1" parent="1"><mxGeometry width="400" height="20" as="geometry"/></mxCell>"#,
        400.0,
        20.0,
    ),
    (
        "Tall Bar",
        r#"<mxCell id="2" style="" vertex="1" parent="1"><mxGeometry width="20" height="300" as="geometry"/></mxCell>"#,
        20.0,
        300.0,
    ),
    (
        "Circle",
        r#"<mxCell id="2" style="ellipse;" vertex="1" parent="1"><mxGeometry width="80" height="80" as="geometry"/></mxCell>"#,
        80.0,
        80.0,
    ),
    (
        "Card",
        r#"<object id="g" label="Card" owner="team"><mxCell style="group;" vertex="1" parent="1"><mxGeometry width="100" height="40" as="geometry"/></mxCell></object><mxCell id="a" value="Title" vertex="1" parent="g"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#,
        100.0,
        40.0,
    ),
    (
        "Pool",
        r#"<mxCell id="2" value="Pool" style="swimlane;horizontal=0;startSize=20;" vertex="1" parent="1"><mxGeometry width="320" height="240" as="geometry"/></mxCell><mxCell id="3" value="Lane 1" style="swimlane;horizontal=0;startSize=20;" vertex="1" parent="2"><mxGeometry x="20" width="300" height="120" as="geometry"/></mxCell><mxCell id="4" value="Lane 2" style="swimlane;horizontal=0;startSize=20;" vertex="1" parent="2"><mxGeometry x="20" y="120" width="300" height="120" as="geometry"/></mxCell>"#,
        320.0,
        240.0,
    ),
    (
        "Curve",
        r#"<mxCell id="2" style="curved=1;endArrow=classic;html=1;" edge="1" parent="1"><mxGeometry width="50" height="50" relative="1" as="geometry"><mxPoint y="50" as="sourcePoint"/><mxPoint x="50" as="targetPoint"/><Array as="points"><mxPoint x="50" y="50"/><mxPoint/></Array></mxGeometry></mxCell>"#,
        50.0,
        50.0,
    ),
    (
        "Labelled Connector",
        r#"<mxCell id="2" value="Label" style="endArrow=classic;html=1;" edge="1" parent="1"><mxGeometry width="160" relative="1" as="geometry"><mxPoint y="100" as="sourcePoint"/><mxPoint x="160" y="100" as="targetPoint"/></mxGeometry></mxCell>"#,
        160.0,
        0.0,
    ),
    (
        "Branch",
        r#"<mxCell id="2" value="A" style="rhombus;" vertex="1" parent="1"><mxGeometry width="80" height="60" as="geometry"/></mxCell><mxCell id="3" value="B" style="rounded=1;" vertex="1" parent="1"><mxGeometry x="140" width="120" height="60" as="geometry"/></mxCell><mxCell id="4" edge="1" parent="1" source="2" target="3"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
        260.0,
        60.0,
    ),
];

fn varied() -> library::Library {
    let entries: Vec<_> = VARIED
        .iter()
        .map(|(title, cells, w, h)| {
            let xml = format!(
                r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{cells}</root></mxGraphModel>"#
            );
            serde_json::json!({"xml": compress(&xml), "w": w, "h": h, "title": title})
        })
        .collect();
    library::parse(
        LibraryName::parse("varied").unwrap(),
        library_file(serde_json::Value::Array(entries)).as_bytes(),
    )
    .unwrap()
}

#[test]
fn preview_lays_out_labelled_entries_and_limits_their_number() {
    let varied = varied();
    let all: Vec<_> = varied
        .entries
        .iter()
        .map(|e| {
            (
                Ref {
                    library: varied.name.clone(),
                    entry: e.id.clone(),
                },
                e,
            )
        })
        .collect();
    let xml = preview::preview(&all).unwrap();
    diagram::validate(&xml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    assert!(doc.descendants().any(|n| {
        n.attribute("value")
            .is_some_and(|v| v.starts_with("varied/box\n"))
    }));
    // Groups, pools and connectors keep their size and must not reach a neighbour.
    assert_separate(&xml);
    // A rotated custom image keeps its turn although its style is not printed.
    let rotated = library::parse(
        LibraryName::parse("rotated").unwrap(),
        br#"<mxlibrary>[{"title":"Rotated Image","w":350,"h":30,"xml":"&lt;mxGraphModel&gt;&lt;root&gt;&lt;mxCell id=\"0\"/&gt;&lt;mxCell id=\"1\" parent=\"0\"/&gt;&lt;mxCell id=\"2\" style=\"shape=image;image=data:image/png,iVBORw0KGgo=;rotation=-90;\" vertex=\"1\" parent=\"1\"&gt;&lt;mxGeometry width=\"350\" height=\"30\" as=\"geometry\"/&gt;&lt;/mxCell&gt;&lt;/root&gt;&lt;/mxGraphModel&gt;"}]</mxlibrary>"#,
    )
    .unwrap();
    let entry = &rotated.entries[0];
    assert_separate(
        &preview::preview(&[(
            Ref {
                library: rotated.name.clone(),
                entry: entry.id.clone(),
            },
            entry,
        )])
        .unwrap(),
    );

    // More entries than one preview shows.
    let directory = project();
    let many: Vec<_> = (0..preview::PREVIEW_LIMIT + 1)
        .map(|i| serde_json::json!({"xml": CARD, "w": 100, "h": 40, "title": format!("Card {i}")}))
        .collect();
    fs::write(
        directory.path().join(".d-png/libraries/many.xml"),
        library_file(serde_json::Value::Array(many)),
    )
    .unwrap();
    let output = directory.path().join("preview.png");
    let preview = |args: &[&str]| {
        let mut command = cli();
        command
            .current_dir(directory.path())
            .args(["library", "preview"])
            .args(args)
            .arg("-o")
            .arg(&output);
        command
    };
    let many = stderr(&mut preview(&["--library", "many"]), 2);
    assert!(many.contains("61 entries match"), "{many}");
    let none = stderr(&mut preview(&["no such shape"]), 1);
    assert!(none.contains("no entries match"), "{none}");
    stderr(&mut preview(&[]), 2);
    // JSON and the image cannot share standard output.
    let both = stderr(
        cli()
            .current_dir(directory.path())
            .args(["library", "preview", "card", "--json", "-o", "-"]),
        2,
    );
    assert!(both.contains("give -o a file"), "{both}");
    assert!(!output.exists());
}

#[test]
#[ignore = "requires Chromium and draw.io 26.0.2 web assets; set D_PNG_TEST_CHROME_PATH and D_PNG_TEST_DRAWIO_WEB_PATH"]
fn preview_and_write_render_stencil_shapes_with_web_assets() {
    let chrome = std::env::var_os("D_PNG_TEST_CHROME_PATH").expect("set D_PNG_TEST_CHROME_PATH");
    let web =
        std::env::var_os("D_PNG_TEST_DRAWIO_WEB_PATH").expect("set D_PNG_TEST_DRAWIO_WEB_PATH");
    // A library of draw.io shapes that need the web assets' stencils.
    let directory = tempdir().unwrap();
    cli().arg("init").arg(directory.path()).assert().success();
    let database = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="" style="shape=mxgraph.flowchart.database;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry width="60" height="60" as="geometry"/></mxCell></root></mxGraphModel>"#;
    fs::write(
        directory.path().join(".d-png/libraries/flowchart.xml"),
        library_file(serde_json::json!([
            {"xml": compress(database), "w": 60, "h": 60, "title": "Database"},
        ])),
    )
    .unwrap();
    let render = |args: &[&str], output: &Path, input: Option<&str>| {
        let mut command = cli();
        command
            .current_dir(directory.path())
            .env("D_PNG_CHROME_PATH", &chrome)
            .env(
                "D_PNG_CHROME_ARGS",
                std::env::var("D_PNG_TEST_CHROME_ARGS").unwrap_or_default(),
            )
            .env("D_PNG_DRAWIO_WEB_PATH", &web)
            .args(args)
            .arg("-o")
            .arg(output);
        if let Some(input) = input {
            command.write_stdin(input);
        }
        command.assert().success();
        fs::read(output).unwrap()
    };
    let preview = render(
        &["library", "preview", "database"],
        &directory.path().join("preview.png"),
        None,
    );
    assert!(preview.starts_with(d_png_cli::domain::drawio_png::SIGNATURE));
    // A stencil shape written by reference, on the second page.
    let two = r#"<mxfile><diagram name="A"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram><diagram name="B"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="db" parent="1" dpng-ref="flowchart/database"/></root></mxGraphModel></diagram></mxfile>"#;
    let png = render(
        &["write", "--page", "2"],
        &directory.path().join("database.png"),
        Some(two),
    );
    let (width, height) = d_png_cli::domain::drawio_png::dimensions(&png).unwrap();
    assert_eq!((width, height), (61, 61));
    let xml = d_png_cli::domain::drawio_png::extract(&png).unwrap();
    assert!(xml.contains("shape=mxgraph.flowchart.database") && !xml.contains("dpng-ref"));
}
