//! Library commands (spec 9.7-9.10) over a project area and the built-in
//! `@drawio` libraries.
mod support;

use drawio_png_cli::{
    adapters::builtin::BuiltinLibraries,
    app::{libraries::Libraries, ports::LibraryRepository},
    domain::{
        diagram, library, preview,
        reference::{LibraryName, Ref},
    },
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
        dir.path().join(".drawio-png/libraries/icons.xml"),
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
    fs::write(dir.path().join(".drawio-png/libraries/notes.txt"), "x").unwrap();
    fs::write(dir.path().join(".drawio-png/libraries/Bad Name.xml"), "x").unwrap();
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
    let area = dir.path().join(".drawio-png");
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
fn list_shows_project_libraries_before_built_in_ones() {
    let dir = project();
    let list = run(dir.path(), &["library", "list"]);
    assert!(list.starts_with("icons\t-\t5\tIcons\n"), "{list}");
    assert!(list.contains("@drawio/aws4-compute\t-\t139\tAWS / Compute\n"));
    assert!(!list.contains("Bad Name") && !list.contains("notes"));
    let json: serde_json::Value =
        serde_json::from_str(&run(dir.path(), &["library", "list", "--json"])).unwrap();
    assert_eq!(json[0]["name"], "icons");
    assert_eq!(json[0]["origin"], "project");
    assert_eq!(json[0]["file"], ".drawio-png/libraries/icons.xml");
    assert_eq!(json[1]["origin"], "builtin");
    let builtin = run(dir.path(), &["library", "list", "--scope", "drawio"]);
    assert!(
        builtin.lines().all(|l| l.starts_with("@drawio/")),
        "{builtin}"
    );
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
    // Without a project, only the built-in libraries are there.
    assert!(run(elsewhere.path(), &["library", "list"]).starts_with("@drawio/"));
    let missing = stderr(
        cli()
            .current_dir(elsewhere.path())
            .env("DRAWIO_PNG_PROJECT", elsewhere.path())
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
    let found = run(
        dir.path(),
        &["library", "search", "lambda", "--scope", "drawio"],
    );
    assert!(
        found.starts_with("@drawio/aws3-compute/lambda\t"),
        "{found}"
    );
    assert!(run(dir.path(), &["library", "search", "icon", "a"]).starts_with("icons/icon-a\t"));
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
        r#"<mxCell id="ID" parent="1" libref="icons/card"><mxGeometry x="0" y="0" as="geometry"/></mxCell>"#
    );
    let raw: serde_json::Value = serde_json::from_str(&run(
        dir.path(),
        &["library", "show", "icons/raw", "--json"],
    ))
    .unwrap();
    assert_eq!(raw["image"]["mediaType"], "image/png");
    assert_eq!(raw["image"]["bytes"], 8);
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
    let lambda = run(
        dir.path(),
        &["library", "show", "@drawio/aws4-compute/lambda"],
    );
    assert!(lambda.contains("kind: shape"), "{lambda}");
    assert!(lambda.contains("resIcon=mxgraph.aws4.lambda"), "{lambda}");
    // Close references are suggested for mistakes.
    let error = stderr(
        cli()
            .current_dir(dir.path())
            .args(["library", "show", "@drawio/aws4-compute/lamda"]),
        1,
    );
    assert!(error.contains("error[REF_NOT_FOUND]"), "{error}");
    assert!(
        error.contains(
            "candidates: @drawio/aws4-compute/lambda, @drawio/aws4-compute/lambda-function"
        ),
        "{error}"
    );
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
fn diagrams_use_project_and_built_in_libraries() {
    let dir = project();
    let xml = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="i" parent="1" libref="icons/icon-a"><mxGeometry width="48" as="geometry"/></mxCell><mxCell id="fn" parent="1" libref="@drawio/aws4-compute/lambda"/></root></mxGraphModel>"#;
    let out = stdout(cli().current_dir(dir.path()).arg("expand").write_stdin(xml));
    diagram::validate(&out).unwrap();
    assert!(out.contains("resIcon=mxgraph.aws4.lambda"));
    assert!(!out.contains("libref"));
    // Project references need a project.
    let elsewhere = tempdir().unwrap();
    let error = stderr(
        cli()
            .current_dir(elsewhere.path())
            .arg("validate")
            .write_stdin(xml),
        1,
    );
    assert!(error.contains("no project (.drawio-png/) here"), "{error}");
}

#[test]
fn built_in_catalog_holds_cells_and_styles_but_no_images() {
    let catalog = include_bytes!("../assets/shapes/catalog.json.gz");
    let mut text = String::new();
    std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&catalog[..]), &mut text)
        .unwrap();
    // The catalog only names shapes and image paths; icon artwork is never redistributed.
    assert!(!text.contains("data:image"));
    assert!(text.contains("96a916a337d13fc8bf622c8a67d422bd284eabe5"));
    let builtin = BuiltinLibraries::default();
    let infos = builtin.list().unwrap();
    assert!(infos.len() > 300);
    assert!(infos.iter().all(|i| i.name.is_builtin()));
    // Palettes initPalettes passes arguments to or registers itself.
    for name in [
        "aws4-compute",
        "aws4-groups",
        "azure2-compute",
        "gcp2-zones",
        "kubernetes",
        "general",
        "arrows",
        "cisco-routers",
        "signs-safety",
        "rack-cisco",
        "pid-pumps",
    ] {
        let name = LibraryName::parse(&format!("@drawio/{name}")).unwrap();
        assert!(infos.iter().any(|i| i.name == name), "{name}");
    }
    // childLayout ran as when draw.io adds the cells: the List's items are stacked.
    let libraries = Libraries::new(vec![&builtin]);
    let xml = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="list" parent="1" libref="@drawio/general/list"/></root></mxGraphModel>"#;
    let expanded = drawio_png_cli::app::diagram::expand(xml.as_bytes(), None, &libraries).unwrap();
    let doc = roxmltree::Document::parse(&expanded.xml).unwrap();
    let rows: Vec<_> = doc
        .descendants()
        .filter(|n| n.attribute("value").is_some_and(|v| v.starts_with("Item ")))
        .map(|n| {
            n.first_element_child()
                .unwrap()
                .attribute("y")
                .unwrap_or("0")
                .to_owned()
        })
        .collect();
    assert_eq!(rows, ["30", "60", "90"]);
    // Entry IDs are unique within each library.
    for info in infos {
        let library = builtin.load(&info.name).unwrap().unwrap();
        let mut ids: Vec<_> = library.entries.iter().map(|e| e.id.as_str()).collect();
        ids.sort();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "{}", info.name);
    }
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

#[test]
fn preview_lays_out_labelled_entries_and_limits_their_number() {
    let builtin = BuiltinLibraries::default();
    let load = |name: &str| {
        builtin
            .load(&LibraryName::parse(name).unwrap())
            .unwrap()
            .unwrap()
    };
    let kubernetes = load("@drawio/kubernetes");
    let chosen: Vec<_> = kubernetes.entries[..12]
        .iter()
        .map(|e| {
            (
                Ref {
                    library: kubernetes.name.clone(),
                    entry: e.id.clone(),
                },
                e,
            )
        })
        .collect();
    let xml = preview::preview(&chosen).unwrap();
    diagram::validate(&xml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let first = format!("@drawio/kubernetes/{}\n", kubernetes.entries[0].id);
    assert!(
        doc.descendants()
            .any(|n| n.attribute("value").is_some_and(|v| v.starts_with(&first)))
    );
    // Groups, pools and connectors keep their size and must not reach a neighbour.
    for name in [
        "@drawio/advanced",
        "@drawio/bootstrap",
        "@drawio/sysml-activities",
        "@drawio/gcp2-product-cards",
        "@drawio/mockup-misc",
    ] {
        let library = load(name);
        let all: Vec<_> = library
            .entries
            .iter()
            .map(|e| {
                (
                    Ref {
                        library: library.name.clone(),
                        entry: e.id.clone(),
                    },
                    e,
                )
            })
            .collect();
        for chunk in all.chunks(preview::PREVIEW_LIMIT) {
            assert_separate(&preview::preview(chunk).unwrap());
        }
    }
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

    let directory = tempdir().unwrap();
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
    let many = stderr(&mut preview(&["--library", "@drawio/aws4-compute"]), 2);
    assert!(many.contains("139 entries match"), "{many}");
    let none = stderr(&mut preview(&["no such shape"]), 1);
    assert!(none.contains("no entries match"), "{none}");
    stderr(&mut preview(&[]), 2);
    assert!(!output.exists());
}

#[test]
#[ignore = "requires Chromium and draw.io 26.0.2 web assets; set DRAWIO_PNG_TEST_CHROME_PATH and DRAWIO_PNG_TEST_DRAWIO_WEB_PATH"]
fn preview_and_write_render_built_in_shapes_with_web_assets() {
    let chrome =
        std::env::var_os("DRAWIO_PNG_TEST_CHROME_PATH").expect("set DRAWIO_PNG_TEST_CHROME_PATH");
    let web = std::env::var_os("DRAWIO_PNG_TEST_DRAWIO_WEB_PATH")
        .expect("set DRAWIO_PNG_TEST_DRAWIO_WEB_PATH");
    let directory = tempdir().unwrap();
    let render = |args: &[&str], output: &Path, input: Option<&str>| {
        let mut command = cli();
        command
            .current_dir(directory.path())
            .env("DRAWIO_PNG_CHROME_PATH", &chrome)
            .env(
                "DRAWIO_PNG_CHROME_ARGS",
                std::env::var("DRAWIO_PNG_TEST_CHROME_ARGS").unwrap_or_default(),
            )
            .env("DRAWIO_PNG_DRAWIO_WEB_PATH", &web)
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
        &["library", "preview", "lambda", "--scope", "drawio"],
        &directory.path().join("preview.png"),
        None,
    );
    assert!(preview.starts_with(drawio_png_cli::domain::drawio_png::SIGNATURE));
    // A built-in shape written by reference, on the second page.
    let two = r#"<mxfile><diagram name="A"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram><diagram name="B"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="fn" parent="1" libref="@drawio/aws4-compute/lambda"/></root></mxGraphModel></diagram></mxfile>"#;
    let png = render(
        &["write", "--page", "2"],
        &directory.path().join("lambda.png"),
        Some(two),
    );
    let (width, height) = drawio_png_cli::domain::drawio_png::dimensions(&png).unwrap();
    assert_eq!((width, height), (79, 79));
    let xml = drawio_png_cli::domain::drawio_png::extract(&png).unwrap();
    assert!(xml.contains("resIcon=mxgraph.aws4.lambda") && !xml.contains("libref"));
}
