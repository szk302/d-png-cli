use assert_cmd::cargo::cargo_bin_cmd;
use drawio_png_cli::{
    adapters::render::chromium,
    domain::{diagram as document, drawio_png as png_data},
};
use std::{fs, path::PathBuf, time::Duration};

const MODEL: &str = include_str!("fixtures/model.xml");
const MIXED: &str = include_str!("fixtures/mixed-pages.xml");

fn cli() -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("drawio-png-cli");
    for name in [
        "DRAWIO_PNG_DRAWIO_PATH",
        "DRAWIO_PNG_DRAWIO_ARGS",
        "DRAWIO_PNG_CHROME_PATH",
        "CHROME_PATH",
        "DRAWIO_PNG_CHROME_ARGS",
        "DRAWIO_PNG_RENDERER",
        "DRAWIO_PNG_DRAWIO_WEB_PATH",
        "DRAWIO_PNG_PROJECT",
    ] {
        command.env_remove(name);
    }
    // Neither the developer's settings nor their cache take part.
    command
        .env(
            "DRAWIO_PNG_HOME",
            std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-user-area"),
        )
        .env(
            "DRAWIO_PNG_CACHE",
            std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-cache"),
        );
    command
}

#[test]
fn rendering_flags_conflict_with_metadata_only() {
    for flags in [
        vec!["--renderer", "vscode"],
        vec!["--renderer", "raw"],
        vec!["--allow-network"],
    ] {
        cli()
            .args(["write", "--metadata-only", "-o", "unused.png"])
            .args(flags)
            .assert()
            .code(2);
    }
    // v0.1.0 names are gone.
    for name in ["unknown", "chromium", "auto", "desktop"] {
        cli()
            .args(["write", "--renderer", name, "-o", "unused.png"])
            .assert()
            .code(2);
    }
}

#[test]
fn metadata_only_and_xml_commands_ignore_browser_environment() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output.png");
    let mut command = cli();
    command
        .env("DRAWIO_PNG_CHROME_ARGS", "'unfinished")
        .env("DRAWIO_PNG_CHROME_PATH", "missing")
        .env("DRAWIO_PNG_DRAWIO_WEB_PATH", "missing")
        .env("DRAWIO_PNG_RENDERER", "invalid")
        .args(["write", "--metadata-only", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    for command in ["read", "expand", "validate"] {
        cli()
            .env("DRAWIO_PNG_CHROME_ARGS", "'unfinished")
            .env("DRAWIO_PNG_DRAWIO_WEB_PATH", "missing")
            .env("DRAWIO_PNG_RENDERER", "invalid")
            .arg(command)
            .arg(&output)
            .assert()
            .success();
    }
}

#[test]
fn invalid_browser_configuration_preserves_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output.png");
    fs::write(&output, b"original").unwrap();
    // A renderer name is a usage error; browser settings are not.
    for (name, value, code) in [
        ("DRAWIO_PNG_RENDERER", "unknown", 2),
        ("DRAWIO_PNG_RENDERER", "", 2),
        ("DRAWIO_PNG_RENDERER", "RAW", 2),
        ("DRAWIO_PNG_RENDERER", " raw ", 2),
        ("DRAWIO_PNG_RENDERER", "chromium", 2),
        ("DRAWIO_PNG_CHROME_PATH", "missing", 1),
        ("CHROME_PATH", "missing", 1),
        ("DRAWIO_PNG_CHROME_ARGS", "'unfinished", 1),
        ("DRAWIO_PNG_CHROME_ARGS", "--user-data-dir=/tmp/profile", 1),
    ] {
        cli()
            .env(name, value)
            .args(["write", "-o"])
            .arg(&output)
            .write_stdin(MODEL)
            .assert()
            .code(code);
        assert_eq!(fs::read(&output).unwrap(), b"original");
    }
}

#[test]
fn license_texts_are_available_without_a_renderer() {
    let result = cli()
        .arg("licenses")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(result).unwrap();
    assert!(text.contains(include_str!("../LICENSE")));
    for notice in [
        "Apache License",
        "Felix Gnass",
        "Preet Shihn",
        "Mark Adler",
        "Cure53",
        "Copyright 2011, 2012 The Chromium Authors",
        "https://github.com/hediet/vscode-drawio",
        "# Cargo dependency licenses",
        "anyhow",
        "Microsoft Corporation",
        "Unicode",
        // musl is statically linked into the Linux release binaries.
        "musl as a whole is licensed under the following standard MIT license",
        "Rich Felker",
    ] {
        assert!(text.contains(notice), "missing {notice}");
    }
}

#[test]
fn renderers_reject_invalid_combinations_and_preserve_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.png");
    fs::write(&output, b"original").unwrap();
    cli()
        .args(["write", "--renderer", "unknown", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(2);
    let result = cli()
        .env("DRAWIO_PNG_DRAWIO_PATH", "missing")
        .args([
            "write",
            "--renderer",
            "desktop-app",
            "--allow-network",
            "-o",
        ])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&result).contains("--allow-network needs a Chromium renderer"));
    assert_eq!(fs::read(output).unwrap(), b"original");
}

fn pixels(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    buffer.truncate(info.buffer_size());
    (info.width, info.height, buffer)
}
fn browser() -> (PathBuf, Vec<String>) {
    let program =
        std::env::var_os("DRAWIO_PNG_TEST_CHROME_PATH").expect("set DRAWIO_PNG_TEST_CHROME_PATH");
    let args =
        shell_words::split(&std::env::var("DRAWIO_PNG_TEST_CHROME_ARGS").unwrap_or_default())
            .unwrap();
    (PathBuf::from(program), args)
}
fn render(xml: &str, root: Option<PathBuf>, network: bool) -> anyhow::Result<Vec<u8>> {
    let (program, args) = browser();
    chromium::render_with(&program, xml, Duration::from_secs(20), &args, root, network)
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_renders_first_page_and_preserves_xml() {
    let (program, args) = browser();
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("日本語 diagram.png");
    cli()
        .env("DRAWIO_PNG_CHROME_PATH", program)
        .env("DRAWIO_PNG_CHROME_ARGS", shell_words::join(args))
        .env("DRAWIO_PNG_DRAWIO_PATH", "invalid-but-ignored")
        .args(["write", "--renderer", "vscode", "-o"])
        .arg(&output)
        .write_stdin(MIXED.replace("width=\"100\"", "width=\"300\""))
        .assert()
        .success();
    let png = fs::read(output).unwrap();
    let xml = png_data::extract(&png).unwrap();
    document::validate(&xml).unwrap();
    assert_eq!(xml.matches("<mxGraphModel").count(), 2);
    assert!(xml.contains("width=\"300\""));
    let actual = pixels(&png);
    assert!(actual.0 > 1 && actual.1 > 1);
    assert_eq!(actual, pixels(&render(MODEL, None, false).unwrap()));

    // The stable VS Code extension (1.9.0, draw.io 26.0.2) exports this rectangle at 101x41.
    let geometry = r##"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" vertex="1" parent="1" style="rounded=0;fillColor=#dae8fc;strokeColor=#6c8ebf;"><mxGeometry x="10" y="20" width="100" height="40" as="geometry"/></mxCell></root></mxGraphModel>"##;
    let actual = pixels(&render(geometry, None, false).unwrap());
    assert_eq!((actual.0, actual.1), (101, 41));
    assert_eq!(
        actual,
        pixels(include_bytes!("fixtures/geometry-vscode.png"))
    );
    let options =
        format!("<mxfile scale=\"2\" border=\"10\"><diagram>{geometry}</diagram></mxfile>");
    assert_eq!(
        pixels(&render(&options, None, false).unwrap()),
        pixels(include_bytes!("fixtures/geometry-options-vscode.png"))
    );
    // Defaults and absent file attributes must behave like xmlpng.
    let defaults = format!("<mxfile><diagram>{geometry}</diagram></mxfile>");
    assert_eq!(pixels(&render(&defaults, None, false).unwrap()), actual);
    // The SVG pipeline preserves transparency; explicit backgrounds are filled.
    // The plain export is fully covered by the rectangle, so check the border.
    let bordered = pixels(&render(&options, None, false).unwrap());
    assert!(bordered.2.as_chunks::<4>().0.iter().any(|p| p[3] == 0));
    let white = options.replace("<mxGraphModel>", "<mxGraphModel background=\"#ffffff\">");
    let white = pixels(&render(&white, None, false).unwrap());
    assert!(white.2.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_modes_select_distinct_outputs_and_preserve_xml() {
    use chromium::ChromiumMode;
    let (program, args) = browser();
    let dir = tempfile::tempdir().unwrap();
    let geometry = r##"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" vertex="1" parent="1" style="rounded=0;fillColor=#dae8fc;strokeColor=#6c8ebf;"><mxGeometry x="10" y="20" width="100" height="40" as="geometry"/></mxCell></root></mxGraphModel>"##;
    for (mode, size, reference) in [
        ("raw", (103, 43), None),
        (
            "desktop-compat",
            (104, 44),
            Some(&include_bytes!("fixtures/geometry-desktop.png")[..]),
        ),
        (
            "vscode",
            (101, 41),
            Some(&include_bytes!("fixtures/geometry-vscode.png")[..]),
        ),
    ] {
        for from_env in [false, true] {
            let output = dir.path().join(format!("{mode}-{from_env}.png"));
            let mut command = cli();
            command
                .env("DRAWIO_PNG_CHROME_PATH", &program)
                .env("DRAWIO_PNG_CHROME_ARGS", shell_words::join(&args))
                .env("DRAWIO_PNG_DRAWIO_PATH", "invalid-but-ignored")
                .env(
                    "DRAWIO_PNG_RENDERER",
                    if from_env {
                        mode
                    } else {
                        "invalid-but-overridden"
                    },
                )
                .args(["write", "-o"])
                .arg(&output);
            if !from_env {
                command.args(["--renderer", mode]);
            }
            command.write_stdin(geometry).assert().success();
            let png = fs::read(output).unwrap();
            assert_eq!(
                png_data::extract(&png).unwrap(),
                document::normalize(geometry).unwrap()
            );
            let actual = pixels(&png);
            assert_eq!((actual.0, actual.1), size);
            if let Some(reference) = reference {
                assert_eq!(actual, pixels(reference));
            }
        }
    }

    for mode in [ChromiumMode::Raw, ChromiumMode::Desktop] {
        let draw = |xml: &str| {
            chromium::render_with_mode(
                &program,
                xml,
                Duration::from_secs(20),
                &args,
                None,
                false,
                mode,
            )
        };
        assert_eq!(
            pixels(&draw(&document::normalize(MIXED).unwrap()).unwrap()),
            pixels(&draw(MODEL).unwrap())
        );
        // A 2x capture exceeds the limit even though the final pixels fit.
        let large = geometry
            .replace("width=\"100\"", "width=\"3000\"")
            .replace("height=\"40\"", "height=\"1500\"");
        if mode == ChromiumMode::Desktop {
            assert!(format!("{:#}", draw(&large).unwrap_err()).contains("2x Chromium capture"));
        }
        let huge = geometry
            .replace("width=\"100\"", "width=\"5000\"")
            .replace("height=\"40\"", "height=\"4000\"");
        assert!(draw(&huge).is_err());
    }
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_renders_embedded_stencils_in_all_modes() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::io::Write;
    let (program, args) = browser();
    let deflate = |text: &str| {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(urlencoding::encode(text).as_bytes())
            .unwrap();
        encoder.finish().unwrap()
    };
    let shape = r#"<shape w="100" h="40" aspect="variable" strokewidth="inherit"><background><rect x="0" y="0" w="100" h="40"/></background><foreground><fillstroke/></foreground></shape>"#;
    let cell = |shape: &str| {
        format!(
            r##"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" vertex="1" parent="1" style="{shape}fillColor=#dae8fc;strokeColor=#6c8ebf;"><mxGeometry x="10" y="20" width="100" height="40" as="geometry"/></mxCell></root></mxGraphModel>"##
        )
    };
    let stencil = |bytes: &[u8]| cell(&format!("shape=stencil({});", STANDARD.encode(bytes)));
    let render = |xml: &str, mode| {
        chromium::render_with_mode(
            &program,
            xml,
            Duration::from_secs(60),
            &args,
            None,
            false,
            mode,
        )
    };
    // The stencil draws the same rectangle as the built-in shape, offline.
    for mode in [
        chromium::ChromiumMode::Raw,
        chromium::ChromiumMode::Desktop,
        chromium::ChromiumMode::Vscode,
    ] {
        assert_eq!(
            pixels(&render(&stencil(&deflate(shape)), mode).unwrap()),
            pixels(&render(&cell(""), mode).unwrap()),
            "{mode:?}"
        );
    }
    let truncated = deflate(shape);
    for broken in [
        stencil(b"not deflate"),
        stencil(&truncated[..truncated.len() / 2]),
        stencil(&deflate("<notshape/>")),
        cell("shape=stencil(!!!);"),
    ] {
        let error = format!(
            "{:#}",
            render(&broken, chromium::ChromiumMode::Vscode).unwrap_err()
        );
        assert!(error.contains("Invalid embedded stencil"), "{error}");
    }
    // Upstream would silently omit an unresolved include-shape and save a blank image.
    let missing = stencil(&deflate(
        r#"<shape w="100" h="40"><foreground><include-shape name="mxgraph.missing.shape" x="0" y="0" w="100" h="40"/></foreground></shape>"#,
    ));
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.png");
    let original = include_bytes!("fixtures/text.drawio.png");
    for mode in ["raw", "desktop-compat", "vscode"] {
        fs::write(&output, original).unwrap();
        let result = cli()
            .env("DRAWIO_PNG_CHROME_PATH", &program)
            .env("DRAWIO_PNG_CHROME_ARGS", shell_words::join(&args))
            .args(["write", "--renderer", mode, "-o"])
            .arg(&output)
            .write_stdin(missing.as_str())
            .assert()
            .code(1)
            .get_output()
            .stderr
            .clone();
        let error = String::from_utf8_lossy(&result);
        assert!(
            error.contains("mxgraph.missing.shape (include-shape)"),
            "{mode}: {error}"
        );
        assert_eq!(fs::read(&output).unwrap(), original, "{mode}");
    }
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_vscode_resolves_page_placeholders() {
    let model = |label: &str, placeholders: u8| {
        format!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><object id="2" label="{label}" placeholders="{placeholders}"><mxCell vertex="1" parent="1" style="whiteSpace=wrap;html=1;"><mxGeometry x="10" y="20" width="100" height="40" as="geometry"/></mxCell></object></root></mxGraphModel>"#
        )
    };
    // draw.io 26.0.2 (extension 1.9.0) has no %pagecount+1% arithmetic.
    let label = "%page% %pagenumber%/%pagecount%";
    let file = |first: String, pages: usize| {
        let mut xml = format!("<mxfile><diagram name=\"First\">{first}</diagram>");
        for _ in 1..pages {
            xml += &format!("<diagram name=\"Other\">{}</diagram>", model("x", 0));
        }
        xml + "</mxfile>"
    };
    let image = |xml: &str| pixels(&render(xml, None, false).unwrap());
    // Without EditorUi, upstream Graph leaves %pagecount% unresolved. The
    // extension names the page it creates for a bare model Page-1.
    assert_eq!(
        image(&model(label, 1)),
        image(&model("Page-1 1/1", 0)),
        "single model"
    );
    for pages in [1, 3] {
        assert_eq!(
            image(&file(model(label, 1), pages)),
            image(&file(model(&format!("First 1/{pages}"), 0), pages)),
            "{pages} pages"
        );
    }
}

/// Writes the bundled vscode-mode assets as a local DRAWIO_PNG_DRAWIO_WEB_PATH root.
fn vscode_web_root(root: &std::path::Path) {
    for (name, data) in [
        (
            "js/viewer.min.js",
            &include_bytes!("../assets/drawio/vscode/js_viewer.min.js.gz")[..],
        ),
        (
            "js/export-init.js",
            &include_bytes!("../assets/drawio/vscode/js_export-init.js.gz")[..],
        ),
        (
            "js/export.js",
            &include_bytes!("../assets/drawio/vscode/js_export.js.gz")[..],
        ),
        (
            "mxgraph/css/common.css",
            &include_bytes!("../assets/drawio/vscode/mxgraph_css_common.css.gz")[..],
        ),
    ] {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            drawio_png_cli::adapters::fs::read_limited(flate2::read::GzDecoder::new(data)).unwrap(),
        )
        .unwrap();
    }
    fs::write(
        root.join("export3.html"),
        include_str!("../assets/chromium.html"),
    )
    .unwrap();
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_vscode_preloads_extension_shape_bundles() {
    // draw.io 26.0.2 has no shapes/ directory; the extension preloads these
    // bundles instead. Stand-ins keep the test self-contained: the shapes bundle
    // registers a shape class that, like the AWS shapes, looks up its resIcon
    // stencil while painting, and the stencils bundle serves stencil XML.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    vscode_web_root(root);
    fs::write(
        root.join("js/shapes-14-6-5.min.js"),
        r#"function DipIcon() { mxRectangleShape.call(this); }
mxUtils.extend(DipIcon, mxRectangleShape);
DipIcon.prototype.paintVertexShape = function(c, x, y, w, h) {
  mxRectangleShape.prototype.paintVertexShape.apply(this, arguments);
  var icon = mxStencilRegistry.getStencil(mxUtils.getValue(this.style, 'resIcon', null));
  if (icon != null) icon.drawShape(c, this, x, y, w, h);
};
mxCellRenderer.registerShape('mxgraph.aws4.resourceIcon', DipIcon);"#,
    )
    .unwrap();
    fs::write(
        root.join("js/stencils.min.js"),
        r#"(function() {
var load = mxStencilRegistry.loadStencil;
mxStencilRegistry.loadStencil = function(filename, fn) {
  if (filename == STENCIL_PATH + '/aws4.xml') {
    var xml = mxUtils.parseXml('<shapes name="mxgraph.aws4"><shape name="Box" w="100" h="40" aspect="variable"><background><rect x="0" y="0" w="100" h="40"/></background><foreground><fillstroke/></foreground></shape></shapes>');
    return fn != null ? window.setTimeout(function() { fn(xml); }, 0) : xml;
  }
  return load.apply(this, arguments);
};
})();"#,
    )
    .unwrap();
    let cell =
        |style: &str| MODEL.replace("vertex=\"1\"", &format!("vertex=\"1\" style=\"{style}\""));
    let expected = pixels(&render(MODEL, None, false).unwrap());
    // Neither style may request shapes/mxAWS4.js, which would fail with 404.
    for style in [
        "shape=mxgraph.aws4.resourceIcon;",
        "shape=mxgraph.aws4.box;",
        "shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.box;",
    ] {
        assert_eq!(
            pixels(&render(&cell(style), Some(root.to_owned()), false).unwrap()),
            expected,
            "{style}"
        );
    }
    // Upstream shapes omit an icon they cannot find; drawio-png-cli must not save that image.
    let missing = cell("shape=mxgraph.aws4.resourceIcon;resIcon=mxgraph.aws4.missing_icon;");
    let error = format!(
        "{:#}",
        render(&missing, Some(root.to_owned()), false).unwrap_err()
    );
    assert!(
        error.contains("Unsupported shape: mxgraph.aws4.missing_icon"),
        "{error}"
    );
    // Cell shapes such as rect fall back to a rectangle, but an icon lookup of the
    // same name draws nothing upstream, so it still fails. (Namespaced fallbacks
    // like mxgraph.android.rect would load stencils this stand-in root lacks.)
    for name in ["rect", "text"] {
        let shape = cell(&format!("shape={name};"));
        render(&shape, Some(root.to_owned()), false).unwrap();
        let icon = cell(&format!("shape=mxgraph.aws4.resourceIcon;resIcon={name};"));
        let error = format!(
            "{:#}",
            render(&icon, Some(root.to_owned()), false).unwrap_err()
        );
        assert!(
            error.contains(&format!("Unsupported shape: {name};")),
            "{name}: {error}"
        );
    }
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_local_assets_and_unsupported_content() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    vscode_web_root(root);
    assert_eq!(
        pixels(&render(MODEL, Some(root.to_owned()), false).unwrap()),
        pixels(&render(MODEL, None, false).unwrap())
    );
    // Shape libraries load on demand from local assets and register shape
    // classes, not stencils. A stand-in library keeps the test self-contained.
    let aws = MODEL.replace(
        "vertex=\"1\"",
        "vertex=\"1\" style=\"shape=mxgraph.aws4.resourceIcon;\"",
    );
    assert!(render(&aws, Some(root.to_owned()), false).is_err());
    for (name, data) in [
        (
            "shapes/mxAWS4.js",
            r#"function DipIcon() { mxRectangleShape.call(this); }
mxUtils.extend(DipIcon, mxRectangleShape);
DipIcon.prototype.paintVertexShape = function(c, x, y, w, h) {
  mxRectangleShape.prototype.paintVertexShape.apply(this, arguments);
  var icon = mxStencilRegistry.getStencil(mxUtils.getValue(this.style, 'resIcon', null));
  if (icon != null) icon.drawShape(c, this, x, y, w, h);
};
mxCellRenderer.registerShape('mxgraph.aws4.resourceIcon', DipIcon);"#,
        ),
        ("stencils/aws4.xml", "<shapes name=\"mxgraph.aws4\"/>"),
    ] {
        fs::create_dir_all(root.join(name).parent().unwrap()).unwrap();
        fs::write(root.join(name), data).unwrap();
    }
    assert_eq!(
        pixels(&render(&aws, Some(root.to_owned()), false).unwrap()),
        pixels(&render(MODEL, None, false).unwrap())
    );
    assert!(format!("{:#}", render(&aws, None, false).unwrap_err()).contains("Unsupported shape"));
    fs::remove_file(root.join("js/viewer.min.js")).unwrap();
    assert!(
        render(MODEL, Some(root.to_owned()), false)
            .unwrap_err()
            .to_string()
            .contains("Chromium")
    );
    let unknown = MODEL.replace(
        "vertex=\"1\"",
        "vertex=\"1\" style=\"shape=mxgraph.missing.shape;\"",
    );
    assert!(
        format!("{:#}", render(&unknown, None, false).unwrap_err()).contains("Unsupported shape")
    );
    // Names draw.io's own sidebar writes but never registers draw as rectangles, as upstream does.
    let filled = |shape: &str| {
        MODEL.replace(
            "vertex=\"1\"",
            &format!("vertex=\"1\" style=\"{shape}fillColor=#33b5e5;strokeColor=none;\""),
        )
    };
    let rectangle = pixels(&render(&filled(""), None, false).unwrap());
    for name in ["rect", "text", "mxgraph.android.rect"] {
        let shape = format!("shape={name};");
        assert_eq!(
            pixels(&render(&filled(&shape), None, false).unwrap()),
            rectangle,
            "{name}"
        );
    }
    let math = MODEL.replace("<mxGraphModel ", "<mxGraphModel math=\"1\" ");
    assert!(format!("{:#}", render(&math, None, false).unwrap_err()).contains("Math"));
    let huge = MODEL.replace("width=\"100\"", "width=\"20000000\"");
    assert!(render(&huge, None, false).is_err());
    let large_area = MODEL
        .replace("width=\"100\"", "width=\"5000\"")
        .replace("height=\"40\"", "height=\"4000\"");
    assert!(
        format!("{:#}", render(&large_area, None, false).unwrap_err())
            .contains("Canvas image exceeds")
    );
    // Validate the requested scale before allocating the Canvas.
    let oversized = format!("<mxfile scale=\"1000000\"><diagram>{MODEL}</diagram></mxfile>");
    assert!(
        format!("{:#}", render(&oversized, None, false).unwrap_err())
            .contains("Canvas image exceeds")
    );
    for attributes in [
        "scale=\"0\"",
        "scale=\"-1\"",
        "scale=\"Infinity\"",
        "border=\"-1\"",
    ] {
        let invalid = format!("<mxfile {attributes}><diagram>{MODEL}</diagram></mxfile>");
        assert!(
            format!("{:#}", render(&invalid, None, false).unwrap_err())
                .contains("Invalid PNG scale or border")
        );
    }
    let broken_image = MODEL.replace(
        "vertex=\"1\"",
        "vertex=\"1\" style=\"shape=image;image=data:image/png,aW52YWxpZA==;\"",
    );
    assert!(render(&broken_image, None, false).is_err());
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_previews_a_custom_library_without_web_assets() {
    let (program, args) = browser();
    let dir = tempfile::tempdir().unwrap();
    let libraries = dir.path().join(".drawio-png/libraries");
    fs::create_dir_all(&libraries).unwrap();
    let library = libraries.join("icons.xml");
    // A red 1x1 PNG, as libraries such as Simple Icons embed their icons.
    fs::write(
        &library,
        r#"<mxlibrary>[{"data":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==","w":32,"h":32,"title":"Red Dot"}]</mxlibrary>"#,
    )
    .unwrap();
    let output = dir.path().join("preview.png");
    cli()
        .env("DRAWIO_PNG_CHROME_PATH", program)
        .env("DRAWIO_PNG_CHROME_ARGS", shell_words::join(args))
        .current_dir(dir.path())
        .args(["library", "preview", "dot", "--library", "icons", "-o"])
        .arg(&output)
        .assert()
        .success();
    let (width, height, rgba) = pixels(&fs::read(&output).unwrap());
    assert!(width > 32 && height > 32);
    // The icon itself is drawn, not only its label.
    assert!(
        rgba.chunks(4)
            .any(|p| p[0] > 200 && p[1] < 60 && p[2] < 60 && p[3] > 200)
    );
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_deadline_covers_javascript_execution() {
    let (program, args) = browser();
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("export3.html"),
        "<!doctype html><script>while (true) {}</script>",
    )
    .unwrap();
    let start = std::time::Instant::now();
    let error = chromium::render_with(
        &program,
        MODEL,
        Duration::from_secs(2),
        &args,
        Some(dir.path().to_owned()),
        false,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_reports_dialogs_without_waiting_for_the_deadline() {
    let (program, args) = browser();
    let dir = tempfile::tempdir().unwrap();
    for (script, message) in [
        ("alert('broken asset')", "alert dialog: broken asset"),
        ("confirm('continue?')", "confirm dialog: continue?"),
    ] {
        fs::write(
            dir.path().join("export3.html"),
            format!("<!doctype html><script>{script}</script>"),
        )
        .unwrap();
        let start = std::time::Instant::now();
        let error = chromium::render_with(
            &program,
            MODEL,
            Duration::from_secs(30),
            &args,
            Some(dir.path().to_owned()),
            false,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains(message), "{error:#}");
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_without_network_ignores_inherited_proxies() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    // Google Chrome sends background requests (updates, accounts) through an
    // inherited proxy, which resolves names itself. Count every connection.
    let (program, args) = browser();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let (connections, stop) = (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicBool::new(false)),
    );
    let (counter, signal) = (connections.clone(), stop.clone());
    let worker = std::thread::spawn(move || {
        while !signal.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok(_) => {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let mut command = cli();
    command
        .env("DRAWIO_PNG_CHROME_PATH", &program)
        .env("DRAWIO_PNG_CHROME_ARGS", shell_words::join(&args))
        .env("no_proxy", "localhost,127.0.0.1")
        .env("NO_PROXY", "localhost,127.0.0.1");
    for name in [
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
    ] {
        command.env(name, &proxy);
    }
    command
        .args(["write", "-o"])
        .arg(dir.path().join("out.png"))
        .write_stdin(MODEL)
        .assert()
        .success();
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    assert_eq!(connections.load(Ordering::Relaxed), 0);
}

#[test]
#[ignore = "requires Chromium; set DRAWIO_PNG_TEST_CHROME_PATH"]
fn real_chromium_html_embedded_images_and_network_policy() {
    use base64::Engine;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0, 255])
            .unwrap();
    }
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/red.png", server.server_addr());
    let requests = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let counter = requests.clone();
    let signal = stop.clone();
    let cors = Arc::new(AtomicBool::new(true));
    let server_cors = cors.clone();
    let data = png.clone();
    let worker = std::thread::spawn(move || {
        while !signal.load(Ordering::Relaxed) {
            if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(50)) {
                counter.fetch_add(1, Ordering::Relaxed);
                let mut response = tiny_http::Response::from_data(data.clone()).with_header(
                    tiny_http::Header::from_bytes("Content-Type", "image/png").unwrap(),
                );
                if server_cors.load(Ordering::Relaxed) {
                    response.add_header(
                        tiny_http::Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
                    );
                }
                let _ = request.respond(response);
            }
        }
    });
    let image_model = |url: &str| {
        MODEL.replace(
            "vertex=\"1\"",
            &format!("vertex=\"1\" style=\"shape=image;image={url};html=1;\""),
        )
    };
    let external = image_model(&url);
    let blocked = render(&external, None, false);
    let blocked_count = requests.load(Ordering::Relaxed);
    let allowed = render(&external, None, true);
    let allowed_count = requests.load(Ordering::Relaxed);
    cors.store(false, Ordering::Relaxed);
    let no_cors = render(&external, None, true);
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    assert!(blocked.is_err());
    assert_eq!(blocked_count, 0);
    assert!(allowed_count > 0);
    assert!(no_cors.is_err());
    let embedded = image_model(&format!(
        "data:image/png,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    ));
    let actual = render(&embedded, None, false).unwrap();
    assert_eq!(pixels(&actual), pixels(&allowed.unwrap()));
    assert!(
        pixels(&actual)
            .2
            .as_chunks::<4>()
            .0
            .contains(&[255, 0, 0, 255])
    );
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, time::Instant};
    fn script(root: &std::path::Path, name: &str, body: &str) -> PathBuf {
        let path = root.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
    #[test]
    fn explicit_browser_path_takes_precedence_and_renderers_never_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(dir.path(), "chrome", "echo browser-selected >&2; exit 4");
        let output = dir.path().join("out.png");
        fs::write(&output, b"original").unwrap();
        let result = cli()
            .env("DRAWIO_PNG_CHROME_PATH", &path)
            .env("CHROME_PATH", "missing")
            .args(["write", "-o"])
            .arg(&output)
            .write_stdin(MODEL)
            .assert()
            .code(1)
            .get_output()
            .stderr
            .clone();
        assert!(String::from_utf8_lossy(&result).contains("browser-selected"));
        let desktop = script(dir.path(), "desktop", "echo desktop-selected >&2; exit 4");
        // desktop-app runs Desktop only and ignores the browser settings.
        let result = cli()
            .env("DRAWIO_PNG_CHROME_PATH", &path)
            .env("DRAWIO_PNG_DRAWIO_PATH", &desktop)
            .env("DRAWIO_PNG_CHROME_ARGS", "'unfinished")
            .args(["write", "--renderer", "desktop-app", "-o"])
            .arg(&output)
            .write_stdin(MODEL)
            .assert()
            .code(1)
            .get_output()
            .stderr
            .clone();
        assert!(String::from_utf8_lossy(&result).contains("desktop-selected"));
        assert!(!String::from_utf8_lossy(&result).contains("browser-selected"));
        // The default renderer is Chromium even when Desktop is installed.
        let result = cli()
            .env("DRAWIO_PNG_DRAWIO_PATH", &desktop)
            .env("DRAWIO_PNG_CHROME_PATH", &path)
            .args(["write", "-o"])
            .arg(&output)
            .write_stdin(MODEL)
            .assert()
            .code(1)
            .get_output()
            .stderr
            .clone();
        let error = String::from_utf8_lossy(&result);
        assert!(error.contains("browser-selected"));
        assert!(!error.contains("desktop-selected"));
        assert_eq!(fs::read(&output).unwrap(), b"original");
    }

    #[test]
    fn path_search_prefers_chrome_over_earlier_chromium() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = (dir.path().join("first"), dir.path().join("second"));
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        script(&first, "chromium", "echo chromium-selected >&2; exit 4");
        let path = std::env::join_paths([&first, &second]).unwrap();
        let output = dir.path().join("out.png");
        let selected = || {
            let result = cli()
                .env("PATH", &path)
                .args(["write", "-o"])
                .arg(&output)
                .write_stdin(MODEL)
                .assert()
                .code(1)
                .get_output()
                .stderr
                .clone();
            String::from_utf8_lossy(&result).into_owned()
        };
        assert!(selected().contains("chromium-selected"));
        script(&second, "google-chrome", "echo chrome-selected >&2; exit 4");
        let error = selected();
        assert!(error.contains("chrome-selected"));
        assert!(!error.contains("chromium-selected"));
    }

    #[test]
    fn browser_name_resolution_is_limited_to_loopback_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            "chromium",
            r#"printf '%s\n' "$@" > "$(dirname "$0")/args"; exit 4"#,
        );
        let rules = |network: bool| {
            let mut command = cli();
            command.env("DRAWIO_PNG_CHROME_PATH", &path).args(["write"]);
            if network {
                command.arg("--allow-network");
            }
            command
                .arg("-o")
                .arg(dir.path().join("out.png"))
                .write_stdin(MODEL)
                .assert()
                .code(1);
            let args = fs::read_to_string(dir.path().join("args")).unwrap();
            let proxy_disabled = args.lines().any(|line| line == "--no-proxy-server");
            let rules = args
                .lines()
                .filter_map(|line| line.strip_prefix("--host-resolver-rules="))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (rules, proxy_disabled)
        };
        // An inherited proxy would resolve names itself, so it is disabled too.
        assert_eq!(
            rules(false),
            (
                vec!["MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE 127.0.0.1".to_owned()],
                true
            )
        );
        // With network access, only Chrome's own background services are blocked,
        // and the user's proxy stays available for diagram assets.
        let (allowed, proxy_disabled) = rules(true);
        assert_eq!(allowed.len(), 1);
        assert!(allowed[0].contains("MAP update.googleapis.com ~NOTFOUND"));
        assert!(!allowed[0].contains("MAP *"));
        assert!(!proxy_disabled);
    }

    #[test]
    fn browser_options_are_literal_and_invalid_options_never_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            "chromium",
            r#"
[ "$1" = '--label=日本語 space' ] || exit 10
[ "$2" = '--literal=$(touch injected)' ] || exit 11
touch "$(dirname "$0")/started"
echo literal-arguments-ok >&2
exit 7
"#,
        );
        let output = dir.path().join("out.png");
        let result = cli()
            .env("DRAWIO_PNG_CHROME_PATH", &path)
            .env(
                "DRAWIO_PNG_CHROME_ARGS",
                "--label='日本語 space' '--literal=$(touch injected)'",
            )
            .current_dir(dir.path())
            .args(["write", "-o"])
            .arg(&output)
            .write_stdin(MODEL)
            .assert()
            .code(1)
            .get_output()
            .stderr
            .clone();
        assert!(String::from_utf8_lossy(&result).contains("literal-arguments-ok"));
        assert!(!dir.path().join("injected").exists());
        fs::remove_file(dir.path().join("started")).unwrap();
        cli()
            .env("DRAWIO_PNG_CHROME_PATH", &path)
            .env("DRAWIO_PNG_CHROME_ARGS", "'unfinished")
            .args(["write", "-o"])
            .arg(&output)
            .write_stdin(MODEL)
            .assert()
            .code(1);
        assert!(!dir.path().join("started").exists());
    }
    #[test]
    fn browser_timeout_stops_helpers_and_removes_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            "chromium",
            r#"
for arg in "$@"; do case "$arg" in --user-data-dir=*) echo "${arg#--user-data-dir=}" > "$(dirname "$0")/profile";; esac; done
(sleep 0.5; touch "$(dirname "$0")/escaped") &
wait
"#,
        );
        let start = Instant::now();
        let error =
            chromium::render_with(&path, MODEL, Duration::from_millis(150), &[], None, false)
                .unwrap_err();
        assert!(format!("{error:#}").contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(600));
        assert!(!dir.path().join("escaped").exists());
        let profile = fs::read_to_string(dir.path().join("profile")).unwrap();
        assert!(!PathBuf::from(profile.trim()).exists());
    }

    #[test]
    fn termination_signals_stop_the_renderer_and_remove_its_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            "chromium",
            r#"
for arg in "$@"; do case "$arg" in --user-data-dir=*) echo "${arg#--user-data-dir=}" > "$(dirname "$0")/profile";; esac; done
(sleep 1; touch "$(dirname "$0")/escaped") &
wait
"#,
        );
        let output = dir.path().join("out.png");
        fs::write(dir.path().join("in.xml"), MODEL).unwrap();
        for signal in [libc::SIGTERM, libc::SIGINT] {
            fs::write(&output, b"original").unwrap();
            let _ = fs::remove_file(dir.path().join("profile"));
            let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_drawio-png-cli"))
                .env("DRAWIO_PNG_CHROME_PATH", &path)
                .env_remove("DRAWIO_PNG_CHROME_ARGS")
                .env_remove("DRAWIO_PNG_RENDERER")
                .env_remove("DRAWIO_PNG_DRAWIO_WEB_PATH")
                .arg("write")
                .arg(dir.path().join("in.xml"))
                .arg("-o")
                .arg(&output)
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            // Wait until the fake browser has started.
            let start = Instant::now();
            while !dir.path().join("profile").exists() {
                assert!(start.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(20));
            }
            unsafe { libc::kill(child.id() as i32, signal) };
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "drawio-png-cli did not exit"
                );
                std::thread::sleep(Duration::from_millis(20));
            };
            let mut stderr = String::new();
            std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
            // drawio-png-cli cleans up, then ends with the signal itself (130/143 in a shell).
            assert_eq!(
                std::os::unix::process::ExitStatusExt::signal(&status),
                Some(signal),
                "{stderr}"
            );
            assert!(stderr.contains("interrupted"), "{stderr}");
            assert_eq!(fs::read(&output).unwrap(), b"original");
            let profile = fs::read_to_string(dir.path().join("profile")).unwrap();
            assert!(!PathBuf::from(profile.trim()).exists());
            std::thread::sleep(Duration::from_millis(1200));
            assert!(!dir.path().join("escaped").exists());
        }
    }

    #[test]
    fn signals_outside_rendering_keep_their_default_behavior() {
        // While drawio-png-cli waits for stdin, a signal must stop it before it saves.
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.png");
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            fs::write(&output, b"original").unwrap();
            let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_drawio-png-cli"))
                .args(["write", "--metadata-only", "-o"])
                .arg(&output)
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            std::thread::sleep(Duration::from_millis(300));
            unsafe { libc::kill(child.id() as i32, signal) };
            let start = Instant::now();
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if start.elapsed() > Duration::from_secs(2) {
                    // Still waiting: completing the input must not save either.
                    use std::io::Write;
                    let mut stdin = child.stdin.take().unwrap();
                    let _ = stdin.write_all(MODEL.as_bytes());
                    drop(stdin);
                    let _ = child.wait();
                    panic!("drawio-png-cli ignored signal {signal}");
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            assert_eq!(
                std::os::unix::process::ExitStatusExt::signal(&status),
                Some(signal)
            );
            assert_eq!(fs::read(&output).unwrap(), b"original");
        }
    }

    #[test]
    fn ignored_signals_stay_ignored_while_rendering() {
        // nohup ignores SIGHUP; drawio-png-cli must not turn it into an interruption.
        let dir = tempfile::tempdir().unwrap();
        let path = script(dir.path(), "chromium", "sleep 1; exit 3");
        fs::write(dir.path().join("in.xml"), MODEL).unwrap();
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_drawio-png-cli"));
        command
            .env("DRAWIO_PNG_CHROME_PATH", &path)
            .env_remove("DRAWIO_PNG_CHROME_ARGS")
            .env_remove("DRAWIO_PNG_RENDERER")
            .env_remove("DRAWIO_PNG_DRAWIO_WEB_PATH")
            .arg("write")
            .arg(dir.path().join("in.xml"))
            .arg("-o")
            .arg(dir.path().join("out.png"))
            .stderr(std::process::Stdio::piped());
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut command, || {
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        unsafe { libc::kill(child.id() as i32, libc::SIGHUP) };
        let output = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        // The fake browser exits before startup, so drawio-png-cli reports that instead.
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert!(stderr.contains("exited before startup"), "{stderr}");
        assert!(!stderr.contains("interrupted"), "{stderr}");
    }
}
