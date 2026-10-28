//! `library vscode` (spec 9.17): the libraries as the VS Code extension's
//! customLibraries setting in .vscode/settings.json.
mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};
use support::{cli, stderr, stdout};

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/examples")
}

/// The example project of use case I: three project libraries and @acme/brand
/// installed from the example source.
fn example_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("acme-d-png-libraries/@acme/brand");
    fs::create_dir_all(&source).unwrap();
    for file in ["1.0.0.xml", "versions.json"] {
        fs::copy(
            examples()
                .join("acme-d-png-libraries/@acme/brand")
                .join(file),
            source.join(file),
        )
        .unwrap();
    }
    let project = dir.path().join("project");
    fs::create_dir(&project).unwrap();
    run(dir.path(), &["init"]);
    fs::copy(
        examples().join("project/.d-png/config.toml"),
        project.join(".d-png/config.toml"),
    )
    .unwrap();
    let mut add = command(dir.path(), &["library", "add"]);
    add.arg(examples().join("sources/simple-icons.xml"));
    stdout(&mut add);
    let mut add = command(dir.path(), &["library", "add", "--title", "Components"]);
    add.arg(examples().join("sources/components.xml"));
    stdout(&mut add);
    let mut add = command(dir.path(), &["library", "add", "--title", "Shapes"]);
    add.arg(examples().join("sources/shapes.xml"));
    stdout(&mut add);
    run(dir.path(), &["library", "install"]);
    dir
}

fn command(root: &Path, args: &[&str]) -> assert_cmd::Command {
    let mut command = cli();
    command
        .current_dir(root.join("project"))
        .env("D_PNG_CACHE", root.join("cache"))
        .env("D_PNG_HOME", root.join("home"))
        .args(args);
    command
}

fn run(root: &Path, args: &[&str]) -> String {
    stdout(&mut command(root, args))
}

fn settings(root: &Path) -> PathBuf {
    root.join("project/.vscode/settings.json")
}

#[test]
fn the_example_settings_are_written() {
    let dir = example_project();
    let value = run(dir.path(), &["library", "vscode"]);
    assert!(!settings(dir.path()).exists());
    assert_eq!(
        run(dir.path(), &["library", "vscode", "--write"]),
        "wrote 4 libraries to .vscode/settings.json\n"
    );
    let written = fs::read_to_string(settings(dir.path())).unwrap();
    assert_eq!(
        written,
        fs::read_to_string(examples().join("project/.vscode/settings.json")).unwrap()
    );
    // The printed value is the setting's value as written.
    let parsed: serde_json::Value = serde_json::from_str(&written).unwrap();
    let printed: serde_json::Value = serde_json::from_str(&value).unwrap();
    assert_eq!(parsed["hediet.vscode-drawio.customLibraries"], printed);
}

#[test]
fn the_users_settings_and_items_stay() {
    let dir = example_project();
    fs::create_dir_all(settings(dir.path()).parent().unwrap()).unwrap();
    fs::write(
        settings(dir.path()),
        "{\n  // mine\n  \"editor.tabSize\": 2,\n  \"hediet.vscode-drawio.customLibraries\": [\n    {\"entryId\": \"team\", \"libName\": \"Team\", \"file\": \"/team.xml\"},\n    {\"entryId\": \"d-png-cli\", \"libName\": \"Stale\", \"file\": \"stale.xml\"},\n  ],\n}\n",
    )
    .unwrap();
    run(dir.path(), &["library", "vscode", "--write"]);
    let written = fs::read_to_string(settings(dir.path())).unwrap();
    assert!(
        written.starts_with("{\n  // mine\n  \"editor.tabSize\": 2,\n"),
        "{written}"
    );
    assert!(written.contains(r#"{"entryId": "team", "libName": "Team", "file": "/team.xml"}"#));
    assert!(!written.contains("Stale"));
    assert_eq!(written.matches("\"entryId\"").count(), 5);
    // Settings that cannot be read are left alone.
    fs::write(settings(dir.path()), "{ \"broken\": ").unwrap();
    let error = stderr(
        &mut command(dir.path(), &["library", "vscode", "--write"]),
        1,
    );
    assert!(error.contains("CONFIG_INVALID"), "{error}");
    assert_eq!(
        fs::read_to_string(settings(dir.path())).unwrap(),
        "{ \"broken\": "
    );
}

#[test]
fn adding_a_library_suggests_the_settings_until_they_list_it() {
    let dir = example_project();
    let add = |args: &[&str]| {
        let mut command = command(dir.path(), &["library", "add", "--name", "logos"]);
        command.args(args).arg(examples().join("sources/Logo.svg"));
        let output = command.assert().success().get_output().clone();
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    assert!(add(&[]).contains("library vscode --write"));
    run(dir.path(), &["library", "vscode", "--write"]);
    assert!(!add(&["--replace"]).contains("library vscode"));
    // No project, no settings.
    let elsewhere = tempfile::tempdir().unwrap();
    let error = stderr(
        cli()
            .current_dir(elsewhere.path())
            .args(["library", "vscode"]),
        1,
    );
    assert!(error.contains("PROJECT_NOT_FOUND"), "{error}");
}
