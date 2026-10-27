use assert_cmd::cargo::cargo_bin_cmd;
use drawio_png_cli::domain::{diagram as document, drawio_png as png_data, fonts::FontPolicy};
use std::fs;

const MODEL: &str = include_str!("fixtures/model.xml");

fn policy() -> FontPolicy {
    FontPolicy::new("Noto Sans CJK JP", &["Noto Color Emoji".into()]).unwrap()
}

fn style(xml: &str, id: &str) -> String {
    let doc = roxmltree::Document::parse(xml).unwrap();
    doc.descendants()
        .find(|n| n.has_tag_name("mxCell") && n.attribute("id") == Some(id))
        .unwrap()
        .attribute("style")
        .unwrap_or("")
        .to_owned()
}

#[test]
fn fonts_are_persisted_for_all_pages_and_user_objects() {
    let xml = include_str!("fixtures/mixed-pages.xml");
    let output = policy().apply(xml).unwrap();
    document::validate(&output).unwrap();
    let doc = roxmltree::Document::parse(&output).unwrap();
    let cells: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("mxCell") && n.attribute("vertex") == Some("1"))
        .collect();
    assert_eq!(cells.len(), 2);
    for cell in cells {
        assert_eq!(
            cell.attribute("style"),
            Some("fontFamily=\"Noto Sans CJK JP\", \"Noto Color Emoji\";")
        );
    }
    assert_eq!(style(&output, "0"), "");
    assert!(output.contains("custom=\"retain\""));
    assert!(output.contains("<extension value=\"preserve\"/>"));
    assert_eq!(policy().apply(&output).unwrap(), output);
}

#[test]
fn explicit_fonts_keep_priority_and_fallbacks_precede_generic_fonts() {
    let xml = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="2" vertex="1" style="text;fillColor=red;fontFamily=Arial, sans-serif;"/>
<mxCell id="3" edge="1" style="fontFamily=DejaVu Serif;"/>
<mxCell id="4" vertex="1" style="fontFamily=none;"/>
<mxCell id="5" vertex="1" style="fontFamily=inherit;"/>
<mxCell id="6" vertex="1" style="fontFamily='A, B';fontFamily='Noto Color Emoji', monospace;"/>
<mxCell id="7" vertex="1" style="fontFamily=Custom Font;fontSource=https%3A%2F%2Fexample.test%2Ffont.woff2;"/>
</root></mxGraphModel>"#;
    let output = policy().apply(xml).unwrap();
    assert_eq!(
        style(&output, "2"),
        "text;fillColor=red;fontFamily=Arial, \"Noto Color Emoji\", sans-serif;"
    );
    assert_eq!(
        style(&output, "3"),
        "fontFamily=DejaVu Serif, \"Noto Color Emoji\";"
    );
    assert_eq!(
        style(&output, "4"),
        "fontFamily=\"Noto Sans CJK JP\", \"Noto Color Emoji\";"
    );
    assert_eq!(style(&output, "5"), "fontFamily=inherit;");
    assert_eq!(style(&output, "7"), style(xml, "7"));
    // The last assignment wins and an existing fallback is not appended again.
    assert_eq!(
        style(&output, "6"),
        "fontFamily='A, B';fontFamily='Noto Color Emoji', monospace;"
    );
    assert_eq!(policy().apply(&output).unwrap(), output);
    let no_fallback = FontPolicy::new("Noto Sans CJK JP", &[])
        .unwrap()
        .apply(xml)
        .unwrap();
    assert_eq!(style(&no_fallback, "2"), style(xml, "2"));
}

#[test]
fn html_labels_and_extension_data_are_preserved() {
    let xml = r#"<mxGraphModel custom="yes"><root><mxCell id="0"/><mxCell id="1" parent="0"/>
<!--keep--><object id="wrapped" label="&lt;font face=&quot;DejaVu Serif&quot;&gt;日本語&lt;/font&gt;"><mxCell id="2" vertex="1" style='html=1;fillColor=red;' /></object>
</root><extension><mxGraphModel><root><mxCell vertex="1"/></root></mxGraphModel></extension></mxGraphModel>"#;
    let output = policy().apply(xml).unwrap();
    assert!(
        output.contains("label=\"&lt;font face=&quot;DejaVu Serif&quot;&gt;日本語&lt;/font&gt;\"")
    );
    assert!(output.contains("<!--keep-->"));
    assert!(output.contains(
        "<extension><mxGraphModel><root><mxCell vertex=\"1\"/></root></mxGraphModel></extension>"
    ));
    assert!(style(&output, "2").starts_with("html=1;fillColor=red;"));
}

#[test]
fn quoted_names_are_escaped_and_invalid_lists_are_rejected() {
    let fonts = FontPolicy::new("A & B's \"Font\"", &["Other Font".into()]).unwrap();
    let output = fonts.apply(MODEL).unwrap();
    document::validate(&output).unwrap();
    let doc = roxmltree::Document::parse(&output).unwrap();
    let cell = doc
        .descendants()
        .find(|n| n.attribute("vertex") == Some("1"))
        .unwrap();
    assert_eq!(
        cell.attribute("style"),
        Some("fontFamily=\"A & B's \\\"Font\\\"\", \"Other Font\";")
    );
    assert_eq!(fonts.apply(&output).unwrap(), output);
    for name in ["", " ", "A,B", "A;B", "A=B", "A\nB"] {
        assert!(FontPolicy::new(name, &[]).is_err());
    }
    let broken = MODEL.replace(
        "vertex=\"1\"",
        "vertex=\"1\" style=\"fontFamily='unfinished;\"",
    );
    assert!(policy().apply(&broken).is_err());
}

#[test]
fn cli_round_trip_keeps_the_selected_fonts_and_protects_invalid_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("diagram.png");
    cargo_bin_cmd!("drawio-png-cli")
        .args([
            "write",
            "--metadata-only",
            "--default-font",
            "Noto Sans CJK JP",
            "--fallback-font",
            "Noto Color Emoji",
            "-o",
        ])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    let bytes = fs::read(&output).unwrap();
    let xml = png_data::extract(&bytes).unwrap();
    assert_eq!(xml, policy().apply(MODEL).unwrap());
    for args in [
        vec!["--fallback-font", "Noto Color Emoji"],
        vec!["--default-font", ""],
        vec!["--default-font", "A;B"],
        vec!["--default-font", "Noto Sans CJK JP", "--no-validate"],
        vec![
            "--default-font",
            "Noto Sans CJK JP",
            "--renderer",
            "unknown",
        ],
    ] {
        cargo_bin_cmd!("drawio-png-cli")
            .args(["write", "--metadata-only", "-o"])
            .arg(&output)
            .args(args)
            .write_stdin(MODEL)
            .assert()
            .code(2);
        assert_eq!(fs::read(&output).unwrap(), bytes);
    }
    let broken = MODEL.replace(
        "vertex=\"1\"",
        "vertex=\"1\" style=\"fontFamily='unfinished;\"",
    );
    cargo_bin_cmd!("drawio-png-cli")
        .args([
            "write",
            "--default-font",
            "Noto Sans CJK JP",
            "--fallback-font",
            "Noto Color Emoji",
            "-o",
        ])
        .arg(&output)
        .write_stdin(broken)
        .assert()
        .code(1);
    assert_eq!(fs::read(&output).unwrap(), bytes);
}

#[test]
fn adding_fonts_cannot_expand_xml_past_the_size_limit() {
    let xml = format!(
        "<mxGraphModel><root>{}</root></mxGraphModel>",
        "<mxCell vertex=\"1\"/>".repeat(drawio_png_cli::MAX_BYTES / 1024)
    );
    let policy = FontPolicy::new(&"A".repeat(1024), &[]).unwrap();
    assert!(
        policy
            .apply(&xml)
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
}

#[test]
#[ignore = "requires Desktop, Chromium and a common font; set DRAWIO_PNG_TEST_DRAWIO_PATH, DRAWIO_PNG_TEST_CHROME_PATH and DRAWIO_PNG_TEST_FONT"]
fn real_renderers_use_the_default_and_explicit_fallback() {
    let desktop =
        std::env::var_os("DRAWIO_PNG_TEST_DRAWIO_PATH").expect("set DRAWIO_PNG_TEST_DRAWIO_PATH");
    let chrome =
        std::env::var_os("DRAWIO_PNG_TEST_CHROME_PATH").expect("set DRAWIO_PNG_TEST_CHROME_PATH");
    let family = std::env::var("DRAWIO_PNG_TEST_FONT")
        .expect("set DRAWIO_PNG_TEST_FONT to an installed font");
    let escaped = family
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;");
    let reference = MODEL.replace(
        "vertex=\"1\"",
        &format!("vertex=\"1\" style=\"fontFamily={escaped};\""),
    );
    let missing = MODEL.replace(
        "vertex=\"1\"",
        "vertex=\"1\" style=\"fontFamily=Drawio PNG Nonexistent Test Font;\"",
    );
    let dir = tempfile::tempdir().unwrap();
    let mut saved_xml = Vec::new();
    for renderer in ["desktop-app", "vscode"] {
        let mut images = Vec::new();
        for (name, input, options) in [
            ("reference", reference.as_str(), vec![]),
            ("default", MODEL, vec!["--default-font", family.as_str()]),
            (
                "fallback",
                missing.as_str(),
                vec![
                    "--default-font",
                    family.as_str(),
                    "--fallback-font",
                    family.as_str(),
                ],
            ),
            (
                "explicit",
                reference.as_str(),
                vec!["--default-font", "Drawio PNG Unused Default Font"],
            ),
        ] {
            let output = dir.path().join(format!("{renderer}-{name}.png"));
            cargo_bin_cmd!("drawio-png-cli")
                .env("DRAWIO_PNG_DRAWIO_PATH", &desktop)
                .env("DRAWIO_PNG_CHROME_PATH", &chrome)
                .env(
                    "DRAWIO_PNG_CHROME_ARGS",
                    std::env::var("DRAWIO_PNG_TEST_CHROME_ARGS").unwrap_or_default(),
                )
                .env_remove("DRAWIO_PNG_DRAWIO_WEB_PATH")
                .env_remove("DRAWIO_PNG_RENDERER")
                .args(["write", "--renderer", renderer, "-o"])
                .arg(&output)
                .args(options)
                .write_stdin(input)
                .assert()
                .success();
            let png = fs::read(&output).unwrap();
            let xml = png_data::extract(&png).unwrap();
            document::validate(&xml).unwrap();
            saved_xml.push(xml);
            let mut reader = png::Decoder::new(std::io::Cursor::new(png))
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut pixels).unwrap();
            pixels.truncate(info.buffer_size());
            images.push((info.width, info.height, pixels));
        }
        // Compare within each backend: their DPR and rasterization may still differ.
        for image in &images[1..] {
            assert_eq!(image, &images[0], "font selection differs in {renderer}");
        }
    }
    assert_eq!(&saved_xml[..4], &saved_xml[4..]);
}
