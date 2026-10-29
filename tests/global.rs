//! The global libraries in the user area (spec 2.2, 9.1): registered once
//! with `--global` and used from every project or without one, unless the
//! project has a library of the same name.
mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};
use support::{cli, stderr, stdout};
use tempfile::{TempDir, tempdir};

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/examples")
}

fn diagram(reference: &str) -> String {
    format!(
        r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="x" parent="1" dpng-ref="{reference}"><mxGeometry x="10" y="10" as="geometry"/></mxCell></root></mxGraphModel>"#
    )
}

/// A user area, and a directory with no project above it.
struct Workspace {
    user: TempDir,
    dir: TempDir,
}

impl Workspace {
    fn new() -> Self {
        Self {
            user: tempdir().unwrap(),
            dir: tempdir().unwrap(),
        }
    }

    fn here(&self) -> &Path {
        self.dir.path()
    }

    fn libraries(&self) -> PathBuf {
        self.user.path().join("libraries")
    }

    fn command(&self, dir: &Path, args: &[&str]) -> assert_cmd::Command {
        let mut command = cli();
        command
            .current_dir(dir)
            .env("D_PNG_HOME", self.user.path())
            .args(args);
        command
    }

    fn run(&self, dir: &Path, args: &[&str]) -> String {
        stdout(&mut self.command(dir, args))
    }

    /// `library add --global` of an example source: standard output and error.
    fn add(&self, source: &str) -> (String, String) {
        let output = self
            .command(self.here(), &["library", "add", "--global"])
            .arg(examples().join("sources").join(source))
            .assert()
            .success()
            .get_output()
            .clone();
        (
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }
}

#[test]
fn global_libraries_need_no_project() {
    let w = Workspace::new();
    let here = w.here();
    let error = stderr(
        w.command(here, &["library", "add"])
            .arg(examples().join("sources/simple-icons.xml")),
        1,
    );
    assert!(
        error.contains("PROJECT_NOT_FOUND") && error.contains("--global"),
        "{error}"
    );

    let (out, err) = w.add("simple-icons.xml");
    let file = w.libraries().join("simple-icons.xml");
    assert_eq!(
        out,
        format!(
            "added simple-icons (global, 1 entry) -> {}\n  simple-icons/postgresql\timage\t48x48\tPostgreSQL\n",
            file.display()
        )
    );
    // VS Code reads the project's libraries only, so there is no hint.
    assert_eq!(err, "");
    // The file a project's `library add` writes.
    assert_eq!(
        fs::read(&file).unwrap(),
        fs::read(examples().join("project/.d-png/libraries/simple-icons.xml")).unwrap()
    );

    assert_eq!(
        w.run(here, &["library", "search", "postgres"]),
        "simple-icons/postgresql\timage\t48x48\tPostgreSQL\n"
    );
    let found = w.run(here, &["library", "search", "--json", "postgres"]);
    assert!(found.contains("\"global\":true"), "{found}");
    let shown = w.run(here, &["library", "show", "simple-icons/postgresql"]);
    assert!(
        shown.contains("\norigin: project\nglobal: true\n"),
        "{shown}"
    );
    let listed = w.run(here, &["library", "list", "--json"]);
    assert!(
        listed.contains(&format!(
            "\"file\":{},\"global\":true,",
            serde_json::to_string(&file.display().to_string()).unwrap()
        )),
        "{listed}"
    );

    // References expand, and read turns the image back into a reference.
    let expanded = stdout(
        w.command(here, &["expand"])
            .write_stdin(diagram("simple-icons/postgresql")),
    );
    assert!(
        expanded.contains("data:image/") && !expanded.contains("dpng-ref"),
        "{expanded}"
    );
    let read = stdout(w.command(here, &["read"]).write_stdin(expanded));
    assert!(
        read.contains("image=lib:simple-icons/postgresql@"),
        "{read}"
    );

    assert_eq!(
        w.run(here, &["library", "remove", "--global", "simple-icons"]),
        "removed simple-icons\n"
    );
    assert!(!file.exists());
    let error = stderr(
        w.command(here, &["validate"])
            .write_stdin(diagram("simple-icons/postgresql")),
        1,
    );
    assert!(
        error.contains("REF_NOT_FOUND")
            && error.contains("no project (.d-png/) here")
            && error.contains("library add --global"),
        "{error}"
    );
}

#[test]
fn the_project_hides_global_libraries_of_the_same_name() {
    let w = Workspace::new();
    w.add("components.xml");
    w.add("shapes.xml");
    let project = w.here();
    w.run(project, &["init"]);
    // Another library with the name of a global one.
    stdout(
        w.command(project, &["library", "add", "--name", "components"])
            .arg(examples().join("sources/simple-icons.xml")),
    );

    assert_eq!(
        w.run(project, &["library", "list"]),
        "components\t-\t1\tSimple Icons\nshapes\t-\t3\tshapes\n"
    );
    assert_eq!(
        w.run(project, &["library", "list", "--global"]),
        "components\t-\t3\tcomponents\nshapes\t-\t3\tshapes\n"
    );
    for reference in ["components/postgresql", "shapes/cylinder"] {
        w.command(project, &["validate"])
            .write_stdin(diagram(reference))
            .assert()
            .success();
    }
    let error = stderr(
        w.command(project, &["validate"])
            .write_stdin(diagram("components/service")),
        1,
    );
    assert!(error.contains("REF_NOT_FOUND"), "{error}");
    // Outside the project, the global one.
    let elsewhere = tempdir().unwrap();
    w.command(elsewhere.path(), &["validate"])
        .write_stdin(diagram("components/service"))
        .assert()
        .success();

    // VS Code is given the project's libraries only.
    let settings = w.run(project, &["library", "vscode"]);
    assert!(
        settings.contains("${workspaceFolder}/.d-png/libraries/components.xml")
            && !settings.contains("shapes"),
        "{settings}"
    );

    // sync works on the global libraries with --global.
    let shapes = w.libraries().join("shapes.xml");
    fs::copy(examples().join("sources/shapes.xml"), &shapes).unwrap();
    w.run(project, &["library", "sync", "--check"]);
    let error = stderr(
        &mut w.command(project, &["library", "sync", "--global", "--check"]),
        1,
    );
    assert!(error.contains(&shapes.display().to_string()), "{error}");
    assert_eq!(
        w.run(project, &["library", "sync", "--global"]),
        "normalized shapes\n"
    );
    w.run(project, &["library", "sync", "--global", "--check"]);
}
