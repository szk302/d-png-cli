use assert_cmd::{Command, cargo::cargo_bin_cmd};
use drawio_png_cli::{adapters::fs as storage, domain::drawio_png as png_data};
use std::fs;
use tempfile::tempdir;

const MODEL: &str = include_str!("fixtures/model.xml");
const MIXED: &str = include_str!("fixtures/mixed-pages.xml");
const PLAIN: &[u8] = include_bytes!("fixtures/plain.png");

fn cli() -> Command {
    let mut command = cargo_bin_cmd!("drawio-png-cli");
    command
        .env_remove("DRAWIO_PNG_DRAWIO_PATH")
        .env_remove("DRAWIO_PNG_DRAWIO_ARGS")
        .env_remove("DRAWIO_PNG_RENDERER")
        .env_remove("DRAWIO_PNG_PROJECT")
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
fn stdin_round_trip_and_validation() {
    let directory = tempdir().unwrap();
    let png = directory.path().join("日本語 space.drawio.png");
    cli()
        .args(["write", "--metadata-only", "-o"])
        .arg(&png)
        .write_stdin(MIXED)
        .assert()
        .success()
        .stdout("");
    cli()
        .arg("validate")
        .arg(&png)
        .assert()
        .success()
        .stdout("")
        .stderr("");
    let result = cli()
        .arg("expand")
        .arg(&png)
        .assert()
        .success()
        .stderr("")
        .get_output()
        .stdout
        .clone();
    let xml = String::from_utf8(result).unwrap();
    assert_eq!(xml.matches("<mxGraphModel").count(), 2);
    assert!(xml.contains("日本語"));
    let output = directory.path().join("output.xml");
    cli()
        .arg("read")
        .arg(&png)
        .arg("-o")
        .arg(&output)
        .assert()
        .success()
        .stdout("");
    assert_eq!(fs::read_to_string(output).unwrap(), xml);
    // Standard input and "-" read the same way as a file.
    for args in [vec!["expand"], vec!["expand", "-"]] {
        cli()
            .args(args)
            .write_stdin(fs::read(&png).unwrap())
            .assert()
            .success()
            .stdout(xml.clone());
    }
}

#[test]
fn file_input_bom_and_same_path_base_update() {
    let directory = tempdir().unwrap();
    let xml = directory.path().join("input.xml");
    fs::write(&xml, format!("\u{feff}{MODEL}")).unwrap();
    cli().arg("validate").arg(&xml).assert().success();
    let png = directory.path().join("output.png");
    fs::write(&png, PLAIN).unwrap();
    cli()
        .args(["write", "--metadata-only"])
        .arg(&xml)
        .arg("--base-image")
        .arg(&png)
        .arg("-o")
        .arg(&png)
        .assert()
        .success();
    assert_eq!(png_data::extract(&fs::read(png).unwrap()).unwrap(), MODEL);
}

#[test]
fn invalid_xml_never_changes_existing_output() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("output.png");
    fs::write(&path, PLAIN).unwrap();
    for invalid in [
        "<broken",
        "<mxfile/>",
        "<mxGraphModel><root/></mxGraphModel>",
    ] {
        cli()
            .args(["write", "--metadata-only", "-o"])
            .arg(&path)
            .write_stdin(invalid)
            .assert()
            .code(1)
            .stdout("");
        assert_eq!(fs::read(&path).unwrap(), PLAIN);
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn invalid_renderer_is_an_error_without_touching_output() {
    let directory = tempdir().unwrap();
    let output = directory.path().join("output.png");
    fs::write(&output, PLAIN).unwrap();
    cli()
        .env(
            "DRAWIO_PNG_DRAWIO_PATH",
            directory.path().join("missing-renderer"),
        )
        .args(["write", "--renderer", "desktop-app", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("RENDERER_NOT_FOUND"));
    assert_eq!(fs::read(output).unwrap(), PLAIN);
}

#[test]
fn metadata_only_does_not_discover_a_renderer_or_reuse_output_pixels() {
    let directory = tempdir().unwrap();
    let output = directory.path().join("output.png");
    fs::write(&output, "not a PNG").unwrap();
    cli()
        .env("DRAWIO_PNG_DRAWIO_PATH", "missing-renderer")
        .env("DRAWIO_PNG_CHROME_PATH", "missing-renderer")
        .args(["write", "--metadata-only", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    png_data::validate(&fs::read(output).unwrap()).unwrap();
}

#[test]
fn invalid_base_images_never_replace_output() {
    let directory = tempdir().unwrap();
    let output = directory.path().join("output.png");
    cli()
        .args(["write", "--metadata-only", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    let before = fs::read(&output).unwrap();
    let bad_base = directory.path().join("bad.png");
    fs::write(&bad_base, "bad").unwrap();
    cli()
        .args(["write", "--metadata-only", "--base-image"])
        .arg(&bad_base)
        .arg("-o")
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("PNG_INVALID"));
    assert_eq!(fs::read(output).unwrap(), before);
}

#[test]
fn argument_errors_and_io_errors_have_distinct_exit_codes() {
    cli().arg("write").assert().code(2);
    cli()
        .args(["write", "--base-image", "base.png", "-o", "out.png"])
        .assert()
        .code(2);
    // v0.1.0 commands are gone.
    for command in ["embed", "extract", "insert"] {
        cli().arg(command).assert().code(2);
    }
    cli()
        .args(["validate", "does-not-exist.xml"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("INPUT_READ"));
    cli().arg("--help").assert().success();
}

#[test]
fn atomic_save_failure_cleans_up_temporary_file() {
    let directory = tempdir().unwrap();
    let destination = directory.path().join("directory");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), "unchanged").unwrap();
    assert!(storage::atomic_write(&destination, b"replacement").is_err());
    assert_eq!(
        fs::read_to_string(destination.join("keep")).unwrap(),
        "unchanged"
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn atomic_save_keeps_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempdir().unwrap();
    let path = directory.path().join("private.xml");
    fs::write(&path, "old").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    storage::atomic_write(&path, b"new").unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn commands_without_rendering_ignore_invalid_desktop_options() {
    let directory = tempdir().unwrap();
    let output = directory.path().join("output.png");
    cli()
        .env("DRAWIO_PNG_DRAWIO_ARGS", "'unfinished")
        .args(["write", "--metadata-only", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    cli()
        .env("DRAWIO_PNG_DRAWIO_ARGS", "'unfinished")
        .arg("validate")
        .arg(&output)
        .assert()
        .success();
    cli()
        .env("DRAWIO_PNG_DRAWIO_ARGS", "'unfinished")
        .arg("read")
        .arg(&output)
        .assert()
        .success()
        .stdout(MODEL);
}

#[test]
fn base_images_with_broken_zlib_trailers_never_replace_output() {
    // Rewrite IDAT with a valid chunk CRC so only the zlib stream is broken.
    fn with_idat(edit: impl Fn(&mut Vec<u8>)) -> Vec<u8> {
        let mut result = PLAIN[..8].to_vec();
        let mut pos = 8;
        while pos < PLAIN.len() {
            let length = u32::from_be_bytes(PLAIN[pos..pos + 4].try_into().unwrap()) as usize;
            let (kind, end) = (&PLAIN[pos + 4..pos + 8], pos + 12 + length);
            if kind == b"IDAT" {
                let mut data = PLAIN[pos + 8..pos + 8 + length].to_vec();
                edit(&mut data);
                result.extend_from_slice(&(data.len() as u32).to_be_bytes());
                let start = result.len();
                result.extend_from_slice(kind);
                result.extend_from_slice(&data);
                result.extend_from_slice(&crc32fast::hash(&result[start..]).to_be_bytes());
            } else {
                result.extend_from_slice(&PLAIN[pos..end]);
            }
            pos = end;
        }
        result
    }
    let directory = tempdir().unwrap();
    let output = directory.path().join("out.png");
    for (name, base) in [
        ("bad-adler", with_idat(|d| *d.last_mut().unwrap() ^= 1)),
        ("no-adler", with_idat(|d| d.truncate(d.len() - 4))),
    ] {
        assert!(png_data::validate(&base).is_err(), "{name}");
        let path = directory.path().join(format!("{name}.png"));
        fs::write(&path, &base).unwrap();
        fs::write(&output, PLAIN).unwrap();
        let result = cli()
            .args(["write", "--metadata-only", "--base-image"])
            .arg(&path)
            .arg("-o")
            .arg(&output)
            .write_stdin(MODEL)
            .assert()
            .code(1)
            .get_output()
            .stderr
            .clone();
        assert!(String::from_utf8_lossy(&result).contains("invalid PNG image data"));
        assert_eq!(fs::read(&output).unwrap(), PLAIN, "{name}");
    }
    png_data::validate(&with_idat(|_| ())).unwrap();
}

#[test]
fn skill_guides_match_this_version() {
    let core = include_str!("../assets/skill/drawio-png.md");
    // Normalize in case a checkout converted line endings.
    let stub = &include_str!("../skills/drawio-png/SKILL.md").replace("\r\n", "\n");
    let output = cli()
        .arg("skill")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(String::from_utf8(output).unwrap(), core);
    cli().args(["skill", "--full"]).assert().code(2);
    // The installed stub only points at the guide shipped with the binary.
    assert!(stub.starts_with("---\nname: drawio-png\n"));
    assert!(stub.contains("drawio-png-cli skill ") && !stub.contains("skill --full"));
    // Every `drawio-png-cli <command>` the guides mention must exist in this version.
    let help = |args: &[&str]| {
        let output = cli()
            .args(args)
            .arg("--help")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(output).unwrap()
    };
    let commands = |help: String| -> Vec<String> {
        help.split("Commands:")
            .nth(1)
            .unwrap_or("")
            .lines()
            .skip(1) // the rest of the "Commands:" line
            .take_while(|line| !line.trim().is_empty())
            .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
            .collect()
    };
    let (top, library) = (commands(help(&[])), commands(help(&["library"])));
    for text in [core, stub.as_str()] {
        for mention in text.split("drawio-png-cli ").skip(1) {
            let mut words = mention.split(|c: char| !c.is_ascii_alphanumeric() && c != '-');
            let command = words.next().unwrap_or("");
            if command.is_empty() || !command.chars().all(|c| c.is_ascii_lowercase()) {
                continue;
            }
            assert!(
                top.contains(&command.to_owned()),
                "unknown command: drawio-png-cli {command}"
            );
            if command == "library" {
                let sub = words.find(|w| !w.is_empty()).unwrap_or("");
                assert!(
                    library.contains(&sub.to_owned()),
                    "unknown command: drawio-png-cli library {sub}"
                );
            }
        }
    }
}
