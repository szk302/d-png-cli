#![cfg(unix)]
use assert_cmd::{Command, cargo::cargo_bin_cmd};
use drawio_png_cli::{adapters::render::desktop, domain::drawio_png as png_data};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};
use tempfile::{TempDir, tempdir};

const MODEL: &str = include_str!("fixtures/model.xml");
const MIXED: &str = include_str!("fixtures/mixed-pages.xml");

fn cli() -> Command {
    let mut command = cargo_bin_cmd!("drawio-png-cli");
    command
        .env_remove("DRAWIO_PNG_DRAWIO_ARGS")
        .env_remove("DRAWIO_PNG_RENDERER")
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

fn script(body: &str) -> (TempDir, PathBuf) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("fake drawio");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        directory.path().join("plain.png"),
        include_bytes!("fixtures/plain.png"),
    )
    .unwrap();
    (directory, path)
}

#[test]
fn renderer_arguments_and_full_document_embedding() {
    let (directory, path) = script(
        r#"
[ "$1" = --export ] && [ "$2" = --format ] && [ "$3" = png ] && [ "$4" = --page-index ] && [ "$5" = 1 ] && [ "$6" = --output ] || exit 5
[ -f "$8" ] || exit 6
cp "$8" "$(dirname "$0")/received.xml"
cp "$(dirname "$0")/plain.png" "$7"
"#,
    );
    let output = directory.path().join("output.png");
    cli()
        .env("DRAWIO_PNG_DRAWIO_PATH", &path)
        .args(["write", "--renderer", "desktop-app", "-o"])
        .arg(&output)
        .write_stdin(MIXED)
        .assert()
        .success()
        .stdout("");
    let xml = png_data::extract(&fs::read(output).unwrap()).unwrap();
    assert_eq!(xml.matches("<mxGraphModel").count(), 2);
    assert_eq!(
        fs::read_to_string(directory.path().join("received.xml")).unwrap(),
        xml
    );
}

#[test]
fn font_preferences_reach_desktop_and_the_embedded_xml() {
    let (directory, path) = script(
        r#"
cp "$8" "$(dirname "$0")/received.xml"
cp "$(dirname "$0")/plain.png" "$7"
"#,
    );
    let output = directory.path().join("output.png");
    cli()
        .env("DRAWIO_PNG_DRAWIO_PATH", &path)
        .args([
            "write",
            "--renderer",
            "desktop-app",
            "--default-font",
            "Noto Sans CJK JP",
            "--fallback-font",
            "Noto Color Emoji",
            "-o",
        ])
        .arg(&output)
        .write_stdin(MIXED)
        .assert()
        .success();
    let xml = png_data::extract(&fs::read(output).unwrap()).unwrap();
    assert_eq!(
        fs::read_to_string(directory.path().join("received.xml")).unwrap(),
        xml
    );
    assert_eq!(xml.matches("fontFamily=").count(), 2);
    assert!(xml.contains("Noto Sans CJK JP"));
    assert!(xml.contains("Noto Color Emoji"));
}

#[test]
fn renderer_failures_and_invalid_output_are_errors() {
    for body in [
        "echo 'failure detail' >&2; exit 3",
        "exit 0",
        "echo broken > \"$7\"",
    ] {
        let (_directory, path) = script(body);
        assert!(desktop::render_with(&path, MODEL, Duration::from_secs(2)).is_err());
    }
}

#[test]
fn renderer_failure_does_not_overwrite_destination() {
    let (directory, path) = script("echo failure >&2; exit 1");
    let output = directory.path().join("output.png");
    fs::write(&output, b"original").unwrap();
    cli()
        .env("DRAWIO_PNG_DRAWIO_PATH", &path)
        .args(["write", "--renderer", "desktop-app", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(1);
    assert_eq!(fs::read(&output).unwrap(), b"original");
}

#[test]
fn timeout_terminates_renderer_promptly() {
    let (_directory, path) = script("exec sleep 30");
    let start = Instant::now();
    let error = desktop::render_with(&path, MODEL, Duration::from_millis(80)).unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn path_discovery_works_and_bad_explicit_path_does_not_fall_back() {
    let (directory, path) = script("exit 7");
    fs::rename(&path, directory.path().join("drawio")).unwrap();
    let output = directory.path().join("output.png");
    let result = cli()
        .env_remove("DRAWIO_PNG_DRAWIO_PATH")
        .env("PATH", directory.path())
        .args(["write", "--renderer", "desktop-app", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(1)
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&result).contains("Desktop failed"));
    let result = cli()
        .env("DRAWIO_PNG_DRAWIO_PATH", "does-not-exist")
        .env("PATH", directory.path())
        .args(["write", "--renderer", "desktop-app", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(1)
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&result).contains("DRAWIO_PNG_DRAWIO_PATH"));
}

#[test]
fn timeout_also_stops_renderer_helpers() {
    let (directory, path) = script(
        r#"
(sleep 0.4; echo escaped > "$(dirname "$0")/escaped") &
wait
"#,
    );
    assert!(desktop::render_with(&path, MODEL, Duration::from_millis(80)).is_err());
    std::thread::sleep(Duration::from_millis(500));
    assert!(!directory.path().join("escaped").exists());
}

#[test]
fn environment_options_reach_renderer_as_literal_arguments_before_export_args() {
    let (directory, path) = script(
        r#"
[ "$1" = --disable-gpu ] || exit 10
[ "$2" = '--user-data-dir=/tmp/日本語 profile' ] || exit 11
[ "$3" = '$HOME' ] || exit 12
[ "$4" = '$(touch injected)' ] || exit 13
[ "$5" = '`touch injected`' ] || exit 14
shift 5
[ "$1" = --export ] && [ "$2" = --format ] && [ "$3" = png ] && [ "$4" = --page-index ] && [ "$5" = 1 ] && [ "$6" = --output ] || exit 15
cp "$(dirname "$0")/plain.png" "$7"
"#,
    );
    let output = directory.path().join("output.png");
    cli().current_dir(directory.path()).env("DRAWIO_PNG_DRAWIO_PATH", &path)
        .env("DRAWIO_PNG_DRAWIO_ARGS", r#"--disable-gpu --user-data-dir="/tmp/日本語 profile" '$HOME' '$(touch injected)' '`touch injected`'"#)
        .args(["write", "--renderer", "desktop-app", "-o"]).arg(&output).write_stdin(MODEL).assert().success();
    assert_eq!(
        png_data::extract(&fs::read(output).unwrap()).unwrap(),
        MODEL
    );
    assert!(!directory.path().join("injected").exists());
}

#[test]
fn invalid_environment_options_do_not_start_renderer_or_change_output() {
    let (directory, path) = script("touch \"$(dirname \"$0\")/started\"; exit 1");
    let output = directory.path().join("output.png");
    fs::write(&output, b"original").unwrap();
    let result = cli()
        .env("DRAWIO_PNG_DRAWIO_PATH", &path)
        .env("DRAWIO_PNG_DRAWIO_ARGS", "--user-data-dir='unfinished")
        .args(["write", "--renderer", "desktop-app", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(1)
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&result).contains("invalid DRAWIO_PNG_DRAWIO_ARGS"));
    assert!(!directory.path().join("started").exists());
    assert_eq!(fs::read(output).unwrap(), b"original");
}
