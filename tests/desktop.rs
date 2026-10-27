use assert_cmd::cargo::cargo_bin_cmd;
use drawio_png_cli::domain::{diagram as document, drawio_png as png_data};
use std::{env, fs, time::Duration};

/// Run explicitly with DRAWIO_PNG_TEST_DRAWIO_PATH set to Desktop or an xvfb-run wrapper.
#[test]
#[ignore = "requires an installed draw.io Desktop and graphical environment"]
fn real_desktop_renders_first_page_and_preserves_all_pages() {
    let renderer =
        env::var_os("DRAWIO_PNG_TEST_DRAWIO_PATH").expect("set DRAWIO_PNG_TEST_DRAWIO_PATH");
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("diagram.drawio.png");
    cargo_bin_cmd!("drawio-png-cli")
        .env("DRAWIO_PNG_DRAWIO_PATH", &renderer)
        .args(["embed", "-o"])
        .arg(&output)
        .write_stdin(
            include_str!("fixtures/mixed-pages.xml").replace("width=\"100\"", "width=\"300\""),
        )
        .assert()
        .success();
    let bytes = fs::read(&output).unwrap();
    let xml = png_data::extract(&bytes).unwrap();
    document::validate(&xml).unwrap();
    assert_eq!(xml.matches("<mxGraphModel").count(), 2);
    fn pixels(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buffer).unwrap();
        buffer.truncate(info.buffer_size());
        (info.width, info.height, buffer)
    }
    let first_page = directory.path().join("first.png");
    cargo_bin_cmd!("drawio-png-cli")
        .env("DRAWIO_PNG_DRAWIO_PATH", &renderer)
        .args(["embed", "-o"])
        .arg(&first_page)
        .write_stdin(include_str!("fixtures/model.xml"))
        .assert()
        .success();
    let actual = pixels(&bytes);
    assert!(actual.0 > 1 && actual.1 > 1);
    assert_eq!(actual, pixels(&fs::read(first_page).unwrap()));

    // Desktop must itself reopen the generated PNG and recover both pages.
    let reopened = directory.path().join("reopened.xml");
    let extra_args =
        shell_words::split(&env::var("DRAWIO_PNG_DRAWIO_ARGS").unwrap_or_default()).unwrap();
    assert_cmd::Command::new(renderer)
        .args(extra_args)
        .timeout(Duration::from_secs(60))
        .args(["--export", "--format", "xml", "--uncompressed", "--output"])
        .arg(&reopened)
        .arg(&output)
        .assert()
        .success();
    let reopened = fs::read_to_string(reopened).unwrap();
    document::validate(&reopened).unwrap();
    assert_eq!(reopened.matches("<mxGraphModel").count(), 2);
    assert!(reopened.contains("width=\"300\""));
}
