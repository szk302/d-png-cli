//! Registering project libraries (spec 9.11, 9.15, 9.16): add, remove, sync.
mod support;

use std::{fs, path::Path, path::PathBuf};
use support::{cli, compress, library_file, stderr, stdout};
use tempfile::tempdir;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/v0.2.0_claude/examples")
}

fn project() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    cli().arg("init").arg(dir.path()).assert().success();
    dir
}

fn libraries(dir: &Path) -> PathBuf {
    dir.join(".drawio-png/libraries")
}

#[test]
fn added_libraries_are_written_in_the_normalized_form() {
    let dir = project();
    let out = stdout(
        cli()
            .current_dir(dir.path())
            .args(["library", "add"])
            .arg(examples().join("sources/components.xml"))
            .args(["--title", "Components"]),
    );
    assert_eq!(
        out,
        "added components (project, 3 entries) -> .drawio-png/libraries/components.xml\n  components/service\tshape\t120x60\tService\n  components/card\tgroup\t240x120\tCard\n  components/branch\tset\t260x60\tBranch\n"
    );
    // The CDATA source and the compressed source become the files of the
    // example project, which the VS Code extension reads.
    stdout(
        cli()
            .current_dir(dir.path())
            .args(["library", "add"])
            .arg(examples().join("sources/simple-icons.xml")),
    );
    for name in ["components.xml", "simple-icons.xml"] {
        assert_eq!(
            fs::read_to_string(libraries(dir.path()).join(name)).unwrap(),
            fs::read_to_string(examples().join("project/.drawio-png/libraries").join(name))
                .unwrap(),
            "{name}"
        );
    }
    // An image file is one data entry, sized from the image.
    let out = stdout(
        cli()
            .current_dir(dir.path())
            .args(["library", "add", "--json", "--name", "brand"])
            .arg(examples().join("sources/Logo.svg")),
    );
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["entries"][0]["ref"], "brand/logo");
    assert_eq!(json["entries"][0]["kind"], "image");
    assert_eq!(json["entries"][0]["width"], 240.0);
    let written = fs::read_to_string(libraries(dir.path()).join("brand.xml")).unwrap();
    let installed =
        fs::read_to_string(examples().join("acme-drawio-libraries/@acme/brand/1.0.0.xml")).unwrap();
    assert_eq!(
        written.replace("title=\"brand\"", "title=\"ACME Brand\""),
        installed
    );
    // Registered entries are referenced right away.
    let xml = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="db" parent="1" libref="simple-icons/postgresql"/><mxCell id="c" parent="1" libref="components/card"/></root></mxGraphModel>"#;
    cli()
        .current_dir(dir.path())
        .arg("validate")
        .write_stdin(xml)
        .assert()
        .success();
}

#[test]
fn existing_libraries_are_kept_unless_replaced_or_merged() {
    let dir = project();
    let source = dir.path().join("icons.xml");
    let entries = |titles: &[&str]| {
        let icon = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" style="rounded=1;" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell></root></mxGraphModel>"#;
        library_file(serde_json::Value::Array(
            titles
                .iter()
                .map(|t| serde_json::json!({"xml": compress(icon), "w": 10, "h": 10, "title": t}))
                .collect(),
        ))
    };
    let add = |args: &[&str]| {
        let mut command = cli();
        command
            .current_dir(dir.path())
            .args(["library", "add"])
            .arg(&source)
            .args(args);
        command
    };
    fs::write(&source, entries(&["A", "B"])).unwrap();
    stdout(&mut add(&[]));
    let before = fs::read(libraries(dir.path()).join("icons.xml")).unwrap();
    let error = stderr(&mut add(&[]), 1);
    assert!(error.contains("error[LIBRARY_EXISTS]"), "{error}");
    assert_eq!(
        fs::read(libraries(dir.path()).join("icons.xml")).unwrap(),
        before
    );
    // --merge replaces entries with the same ID and appends the rest.
    fs::write(&source, entries(&["B", "C"])).unwrap();
    assert!(stdout(&mut add(&["--merge"])).starts_with("merged icons (project, 2 entries)"));
    let search = |dir: &Path| {
        stdout(
            cli()
                .current_dir(dir)
                .args(["library", "search", "--library", "icons"]),
        )
    };
    assert_eq!(
        search(dir.path()),
        "icons/a\tshape\t10x10\tA\nicons/b\tshape\t10x10\tB\nicons/c\tshape\t10x10\tC\n"
    );
    // --replace starts over.
    fs::write(&source, entries(&["Z"])).unwrap();
    assert!(stdout(&mut add(&["--replace"])).starts_with("replaced icons"));
    assert_eq!(search(dir.path()), "icons/z\tshape\t10x10\tZ\n");
    stderr(&mut add(&["--replace", "--merge"]), 2);
}

#[test]
fn invalid_entries_and_names_are_rejected() {
    let dir = project();
    let source = dir.path().join("mixed.xml");
    fs::write(
        &source,
        library_file(serde_json::json!([
            {"title": "Good", "data": "data:image/png;base64,iVBORw0KGgo=", "w": 4, "h": 4},
            {"title": "Broken", "xml": "not base64!"},
            {"title": "Nested", "xml": r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" parent="1" libref="x/y"/></root></mxGraphModel>"#},
        ])),
    )
    .unwrap();
    let run = |args: &[&str]| {
        let mut command = cli();
        command
            .current_dir(dir.path())
            .args(["library", "add"])
            .arg(&source)
            .args(args);
        command
    };
    let error = stderr(&mut run(&[]), 1);
    assert!(error.contains("entry 2 \"Broken\""), "{error}");
    assert!(
        error.contains("entry 3 \"Nested\": an entry cannot hold library references"),
        "{error}"
    );
    assert!(!libraries(dir.path()).join("mixed.xml").exists());
    let out = cli()
        .current_dir(dir.path())
        .args(["library", "add", "--skip-invalid"])
        .arg(&source)
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&out.stdout).contains("mixed/good\timage"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("warning[LIBRARY_INVALID]"));
    // Scoped names are installed, not added; names must follow the rules.
    for name in ["@acme/brand", "Bad Name"] {
        let error = stderr(&mut run(&["--name", name]), 2);
        assert!(error.contains("USAGE"), "{error}");
    }
    let unnamed = dir.path().join("図形集.xml");
    fs::copy(&source, &unnamed).unwrap();
    let error = stderr(
        cli()
            .current_dir(dir.path())
            .args(["library", "add", "--skip-invalid"])
            .arg(&unnamed),
        2,
    );
    assert!(error.contains("--name"), "{error}");
    // Writing needs a project.
    let elsewhere = tempdir().unwrap();
    let error = stderr(
        cli()
            .current_dir(elsewhere.path())
            .args(["library", "add"])
            .arg(&source),
        1,
    );
    assert!(error.contains("PROJECT_NOT_FOUND"), "{error}");
}

#[test]
fn libraries_and_entries_are_removed() {
    let dir = project();
    stdout(
        cli()
            .current_dir(dir.path())
            .args(["library", "add"])
            .arg(examples().join("sources/components.xml")),
    );
    let remove = |target: &str, code: i32| {
        let output = cli()
            .current_dir(dir.path())
            .args(["library", "remove", target])
            .assert()
            .code(code)
            .get_output()
            .clone();
        String::from_utf8_lossy(if code == 0 {
            &output.stdout
        } else {
            &output.stderr
        })
        .into_owned()
    };
    assert_eq!(remove("components/card", 0), "removed components/card\n");
    assert!(remove("components/card", 1).contains("REF_NOT_FOUND"));
    assert!(remove("@acme/brand/logo", 1).contains("LIBRARY_MANAGED"));
    assert_eq!(remove("components", 0), "removed components\n");
    assert!(remove("components", 1).contains("REF_NOT_FOUND"));
}

#[test]
fn sync_normalizes_hand_edited_libraries() {
    let dir = project();
    // As draw.io Desktop or a person leaves it: CDATA, entities, no IDs.
    fs::copy(
        examples().join("sources/components.xml"),
        libraries(dir.path()).join("components.xml"),
    )
    .unwrap();
    let list = cli()
        .current_dir(dir.path())
        .args(["library", "list", "--scope", "none"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&list.stderr).is_empty());
    let list = cli()
        .current_dir(dir.path())
        .args(["library", "list"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&list.stderr).contains("library sync components"));
    let error = stderr(
        cli()
            .current_dir(dir.path())
            .args(["library", "sync", "--check"]),
        1,
    );
    assert!(error.contains("LIBRARY_INVALID"), "{error}");
    assert_eq!(
        stdout(cli().current_dir(dir.path()).args(["library", "sync"])),
        "normalized components\n"
    );
    stdout(
        cli()
            .current_dir(dir.path())
            .args(["library", "sync", "--check"]),
    );
    let written = fs::read_to_string(libraries(dir.path()).join("components.xml")).unwrap();
    assert!(written.starts_with("<mxlibrary>[\n{\"id\":\"service\",\"title\":\"Service\""));
    // IDs given once stay, even when titles change.
    fs::write(
        libraries(dir.path()).join("components.xml"),
        written.replace("\"title\":\"Service\"", "\"title\":\"Renamed Service\""),
    )
    .unwrap();
    stdout(cli().current_dir(dir.path()).args(["library", "sync"]));
    assert!(
        stdout(
            cli()
                .current_dir(dir.path())
                .args(["library", "search", "renamed"])
        )
        .starts_with("components/service\t")
    );
    assert!(
        stderr(
            cli()
                .current_dir(dir.path())
                .args(["library", "sync", "@acme/brand"]),
            1
        )
        .contains("LIBRARY_MANAGED")
    );
}
