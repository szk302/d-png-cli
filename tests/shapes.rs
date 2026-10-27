//! draw.io's built-in shapes, offered as the drawio/* libraries.
use assert_cmd::{Command, cargo::cargo_bin_cmd};
use drawio_png_cli::{
    adapters::builtin as shapes,
    domain::{diagram as document, drawio_png as png_data, library, preview},
};
use std::fs;
use tempfile::tempdir;

const MODEL: &str = include_str!("fixtures/model.xml");

fn cli() -> Command {
    let mut command = cargo_bin_cmd!("drawio-png-cli");
    for name in [
        "DRAWIO_PNG_DRAWIO_PATH",
        "DRAWIO_PNG_DRAWIO_ARGS",
        "DRAWIO_PNG_CHROME_PATH",
        "CHROME_PATH",
        "DRAWIO_PNG_CHROME_ARGS",
        "DRAWIO_PNG_CHROMIUM_MODE",
        "DRAWIO_PNG_DRAWIO_WEB_PATH",
        "DRAWIO_PNG_LIBRARY_PATH",
    ] {
        command.env_remove(name);
    }
    command
}

fn placement() -> library::Placement<'static> {
    library::Placement {
        page: 1,
        x: 0.0,
        y: 0.0,
        width: None,
        height: None,
        id: None,
        label: None,
    }
}

fn stdout(command: &mut Command) -> String {
    String::from_utf8(command.assert().success().get_output().stdout.clone()).unwrap()
}

fn stderr(command: &mut Command, code: i32) -> String {
    String::from_utf8(command.assert().code(code).get_output().stderr.clone()).unwrap()
}

#[test]
fn catalog_holds_cells_and_styles_but_no_images() {
    let catalog = include_bytes!("../assets/shapes/catalog.json.gz");
    let mut text = String::new();
    std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&catalog[..]), &mut text)
        .unwrap();
    // The catalog only names shapes and image paths; icon artwork is never redistributed.
    assert!(!text.contains("data:image"));
    assert!(text.contains("96a916a337d13fc8bf622c8a67d422bd284eabe5"));
    let libraries = shapes::libraries().unwrap();
    assert!(libraries.len() > 300);
    // childLayout ran as when draw.io adds the cells: the List's items are stacked.
    let general = libraries
        .iter()
        .find(|l| l.name == "drawio/general")
        .unwrap();
    let list = &general.entries[30];
    assert_eq!(list.title, "List");
    let (model, _) = library::insert(MODEL, list, &placement()).unwrap();
    let doc = roxmltree::Document::parse(&model).unwrap();
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
    assert!(libraries.iter().all(shapes::is_builtin));
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
        let name = format!("drawio/{name}");
        assert!(libraries.iter().any(|l| l.name == name), "{name}");
    }
}

#[test]
fn built_in_libraries_are_listed_and_searched_without_styles() {
    let list = stdout(cli().args(["library", "list"]));
    assert!(
        list.contains("drawio/aws4-compute\t139\tAWS / Compute\n"),
        "{list}"
    );
    let show = stdout(cli().args(["library", "show", "aws4-compute"]));
    assert_eq!(
        show,
        "name: drawio/aws4-compute\ntitle: AWS / Compute\nentries: 139\n"
    );

    // Every library is searched, legacy palettes included.
    let found = stdout(cli().args(["library", "search", "LAMBDA"]));
    assert!(
        found.contains("drawio/aws4-compute\t16\tLambda\t78x78\n"),
        "{found}"
    );
    assert!(found.contains("drawio/aws3-compute\t"), "{found}");
    assert!(!found.contains("style") && !found.contains("mxgraph"));
    // Entries without a sidebar title take their label, like the AWS groups.
    let vpc = stdout(cli().args(["library", "search", "vpc", "--library", "aws4-groups"]));
    assert_eq!(vpc, "drawio/aws4-groups\t7\tVPC\t130x130\n");
    assert_eq!(
        stdout(cli().args(["library", "search", "biohazard"])),
        "drawio/signs-safety\t1\tBiohazard\t106x97\n"
    );
    // The drawio/ prefix may be left out.
    for library in ["drawio/aws4-compute", "aws4-compute"] {
        assert_eq!(
            stdout(cli().args(["library", "search", "lambda", "--library", library])),
            "drawio/aws4-compute\t16\tLambda\t78x78\ndrawio/aws4-compute\t88\tLambda Function\t48x48\n"
        );
    }
    let missing = stderr(cli().args(["library", "search", "--library", "nope"]), 1);
    assert!(
        missing.contains(
            "no library named \"nope\"; available: drawio/* (see `drawio-png-cli library list`)"
        ),
        "{missing}"
    );
    // --no-builtin leaves nothing without DRAWIO_PNG_LIBRARY_PATH.
    let none = stderr(cli().args(["library", "search", "--no-builtin"]), 1);
    assert!(none.contains("set DRAWIO_PNG_LIBRARY_PATH"), "{none}");
}

#[test]
fn style_prints_one_line_for_single_cells_only() {
    // Styles with JSON values come escaped, ready for a style="..." attribute.
    let quoted = stdout(cli().args(["library", "style", "drawio/advanced", "39"]));
    assert!(
        quoted.contains("newEdgeStyle={&quot;edgeStyle&quot;:"),
        "{quoted}"
    );
    let pasted = MODEL.replace(
        "vertex=\"1\"",
        &format!("vertex=\"1\" style=\"{}\"", quoted.trim_end()),
    );
    document::validate(&pasted).unwrap();
    let style = stdout(cli().args(["library", "style", "drawio/aws4-compute", "16"]));
    assert!(style.ends_with("shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.lambda;\n"));
    assert_eq!(style.lines().count(), 1);
    assert_eq!(
        stdout(cli().args(["library", "style", "aws4-compute", "16"])),
        style
    );
    let cards = stderr(
        cli().args(["library", "style", "drawio/gcp2-product-cards", "1"]),
        1,
    );
    assert!(
        cards.contains("`drawio-png-cli insert --library drawio/gcp2-product-cards --index 1`"),
        "{cards}"
    );
    for index in ["0", "79"] {
        let range = stderr(cli().args(["library", "style", "kubernetes", index]), 1);
        assert!(
            range.contains("library \"drawio/kubernetes\" has 78 entries"),
            "{range}"
        );
    }

    // Image entries of custom libraries carry their data only through insert.
    let directory = tempdir().unwrap();
    let file = directory.path().join("icons.xml");
    fs::write(
        &file,
        r#"<mxlibrary>[{"data":"data:image/png;base64,iVBORw0KGgo=","w":16,"h":16,"title":"Dot"}]</mxlibrary>"#,
    )
    .unwrap();
    let image = stderr(
        cli()
            .env("DRAWIO_PNG_LIBRARY_PATH", &file)
            .args(["library", "style", "icons", "1"]),
        1,
    );
    assert!(
        image.contains("`drawio-png-cli insert --library icons --index 1`"),
        "{image}"
    );
    assert!(!image.contains("data:"));
}

#[test]
fn insert_adds_built_in_shapes_by_title_or_index() {
    let directory = tempdir().unwrap();
    let input = directory.path().join("in.xml");
    let output = directory.path().join("out.xml");
    fs::write(&input, MODEL).unwrap();
    let insert = |args: &[&str]| {
        let mut command = cli();
        command
            .arg("insert")
            .args(args)
            .arg("-i")
            .arg(&input)
            .arg("-o")
            .arg(&output);
        command
    };

    let ids = stdout(&mut insert(&[
        "--library",
        "aws4-compute",
        "--name",
        "lambda",
        "--id",
        "fn",
        "--x",
        "40",
        "--y",
        "60",
        "--label",
        "Handler",
    ]));
    assert_eq!(ids, "fn\n");
    let xml = fs::read_to_string(&output).unwrap();
    document::validate(&xml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let cell = doc
        .descendants()
        .find(|n| n.attribute("id") == Some("fn"))
        .unwrap();
    assert!(
        cell.attribute("style")
            .unwrap()
            .contains("shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.lambda;")
    );
    assert_eq!(cell.attribute("value"), Some("Handler"));
    let geometry = cell
        .children()
        .find(|n| n.has_tag_name("mxGeometry"))
        .unwrap();
    assert_eq!(geometry.attribute("x"), Some("40"));
    assert_eq!(geometry.attribute("width"), Some("78"));

    // Multi-cell entries get fresh IDs, as other library entries do.
    let ids = stdout(&mut insert(&[
        "--library",
        "drawio/gcp2-product-cards",
        "--index",
        "1",
    ]));
    assert!(ids.lines().count() > 1, "{ids}");
    document::validate(&fs::read_to_string(&output).unwrap()).unwrap();

    let ambiguous = stderr(&mut insert(&["--name", "Lambda"]), 1);
    assert!(
        ambiguous.contains("\"Lambda\" is in libraries ")
            && ambiguous.contains("\"drawio/aws4-compute\""),
        "{ambiguous}"
    );
    assert!(ambiguous.ends_with("; use --library\n"), "{ambiguous}");
    let hidden = stderr(
        &mut insert(&[
            "--name",
            "Lambda",
            "--library",
            "aws4-compute",
            "--no-builtin",
        ]),
        1,
    );
    assert!(hidden.contains("set DRAWIO_PNG_LIBRARY_PATH"), "{hidden}");
}

#[test]
fn custom_libraries_keep_their_names_and_titles_next_to_built_in_ones() {
    let directory = tempdir().unwrap();
    // Named like drawio/kubernetes, with a title the built-in libraries also use.
    let file = directory.path().join("kubernetes.xml");
    fs::write(
        &file,
        r#"<mxlibrary>[{"data":"data:image/png;base64,iVBORw0KGgo=","w":16,"h":16,"title":"Pod"}]</mxlibrary>"#,
    )
    .unwrap();
    let run = |args: &[&str]| {
        let mut command = cli();
        command.env("DRAWIO_PNG_LIBRARY_PATH", &file).args(args);
        command
    };
    // An exact name is the custom library; drawio/ selects the built-in one.
    assert_eq!(
        stdout(&mut run(&[
            "library",
            "search",
            "pod",
            "--library",
            "kubernetes"
        ])),
        "kubernetes\t1\tPod\t16x16\n"
    );
    assert!(
        stdout(&mut run(&[
            "library",
            "search",
            "pod",
            "--library",
            "drawio/kubernetes"
        ]))
        .starts_with("drawio/kubernetes\t")
    );
    assert!(stdout(&mut run(&["library", "show", "kubernetes"])).starts_with("name: kubernetes\n"));
    assert!(
        stdout(&mut run(&["library", "show", "drawio/kubernetes"]))
            .starts_with("name: drawio/kubernetes\n")
    );
    let image = stderr(&mut run(&["library", "style", "kubernetes", "1"]), 1);
    assert!(
        image.contains("`drawio-png-cli insert --library kubernetes --index 1`"),
        "{image}"
    );
    assert_eq!(
        stdout(&mut run(&["library", "style", "drawio/kubernetes", "24"]))
            .lines()
            .count(),
        1
    );

    // Without --library, a title on DRAWIO_PNG_LIBRARY_PATH wins over the built-in ones.
    let output = directory.path().join("out.xml");
    fs::write(&output, MODEL).unwrap();
    let insert = |args: &[&str]| {
        let mut command = run(&["insert"]);
        command
            .args(args)
            .arg("-i")
            .arg(&output)
            .arg("-o")
            .arg(&output);
        command
    };
    assert_eq!(
        stdout(&mut insert(&["--name", "pod", "--id", "mine"])),
        "mine\n"
    );
    let xml = fs::read_to_string(&output).unwrap();
    assert!(xml.contains("image=data:image/png,iVBORw0KGgo="), "{xml}");
    // Titles only the built-in libraries have are still found there.
    assert_eq!(
        stdout(&mut insert(&["--name", "Private subnet", "--id", "subnet"])),
        "subnet\n"
    );
}

/// Checks that each previewed entry's top-level vertices, which come before
/// its label, overlap neither another entry nor any label.
fn assert_separate(xml: &str, name: &str) {
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
        let value = node
            .attribute("value")
            .or(node.attribute("label"))
            .unwrap_or("");
        if value.starts_with(name) {
            slots.push((current.take(), rect, value.replace('\n', " ")));
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
    let libraries = shapes::libraries().unwrap();
    let kubernetes = libraries
        .iter()
        .find(|l| l.name == "drawio/kubernetes")
        .unwrap();
    let chosen: Vec<_> = (0..12).map(|i| (kubernetes, i)).collect();
    let xml = preview::preview(&chosen).unwrap();
    document::validate(&xml).unwrap();
    assert!(xml.contains("drawio/kubernetes #1&#xa;"));
    assert!(xml.contains("drawio/kubernetes #12&#xa;"));
    // Groups, pools and connectors keep their size and must not reach a neighbour.
    for name in [
        "drawio/advanced",
        "drawio/bootstrap",
        "drawio/sysml-activities",
        "drawio/gcp2-product-cards",
        "drawio/mockup-misc",
    ] {
        let library = libraries.iter().find(|l| l.name == name).unwrap();
        let all: Vec<_> = (0..library.entries.len()).map(|i| (library, i)).collect();
        for chunk in all.chunks(preview::PREVIEW_LIMIT) {
            assert_separate(&preview::preview(chunk).unwrap(), name);
        }
    }
    // A rotated custom image keeps its turn although its style is not printed.
    let rotated = library::Library {
        name: "rotated".into(),
        title: None,
        entries: library::parse(
            br#"<mxlibrary>[{"title":"Rotated Image","w":350,"h":30,"xml":"&lt;mxGraphModel&gt;&lt;root&gt;&lt;mxCell id=\"0\"/&gt;&lt;mxCell id=\"1\" parent=\"0\"/&gt;&lt;mxCell id=\"2\" style=\"shape=image;image=data:image/png,iVBORw0KGgo=;rotation=-90;\" vertex=\"1\" parent=\"1\"&gt;&lt;mxGeometry width=\"350\" height=\"30\" as=\"geometry\"/&gt;&lt;/mxCell&gt;&lt;/root&gt;&lt;/mxGraphModel&gt;"}]</mxlibrary>"#,
        )
        .unwrap(),
    };
    assert_separate(&preview::preview(&[(&rotated, 0)]).unwrap(), "rotated");

    let directory = tempdir().unwrap();
    let output = directory.path().join("preview.png");
    let preview = |args: &[&str]| {
        let mut command = cli();
        command
            .args(["library", "preview"])
            .args(args)
            .arg("-o")
            .arg(&output);
        command
    };
    let many = stderr(&mut preview(&["--library", "aws4-compute"]), 1);
    assert!(many.contains("139 entries match"), "{many}");
    let none = stderr(&mut preview(&["no such shape"]), 1);
    assert!(none.contains("no entries match"), "{none}");
    stderr(&mut preview(&[]), 2);
    assert!(!output.exists());
}

#[test]
#[ignore = "requires Chromium and draw.io 26.0.2 web assets; set DRAWIO_PNG_TEST_CHROME_PATH and DRAWIO_PNG_TEST_DRAWIO_WEB_PATH"]
fn preview_renders_built_in_shapes_with_web_assets() {
    let chrome =
        std::env::var_os("DRAWIO_PNG_TEST_CHROME_PATH").expect("set DRAWIO_PNG_TEST_CHROME_PATH");
    let web = std::env::var_os("DRAWIO_PNG_TEST_DRAWIO_WEB_PATH")
        .expect("set DRAWIO_PNG_TEST_DRAWIO_WEB_PATH");
    let directory = tempdir().unwrap();
    let output = directory.path().join("preview.png");
    cli()
        .env("DRAWIO_PNG_CHROME_PATH", chrome)
        .env(
            "DRAWIO_PNG_CHROME_ARGS",
            std::env::var("DRAWIO_PNG_TEST_CHROME_ARGS").unwrap_or_default(),
        )
        .env("DRAWIO_PNG_DRAWIO_WEB_PATH", web)
        .args(["library", "preview", "lambda", "-o"])
        .arg(&output)
        .assert()
        .success();
    let png = fs::read(&output).unwrap();
    assert!(png.starts_with(png_data::SIGNATURE));
}
