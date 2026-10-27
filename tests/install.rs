//! Scoped libraries (spec 9.5, 9.6, 9.12-9.15): pack a version into a
//! source, install it into a project with lock.json, update and remove it.
mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};
use support::{cli, stderr, stdout};
use tempfile::TempDir;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/v0.2.0_claude/examples")
}

/// A workspace with a source directory `acme-drawio-libraries/`, a project
/// `project/` whose `acme` scope points at it, and its own cache and user area.
struct Workspace {
    dir: TempDir,
}

impl Workspace {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Self { dir };
        fs::create_dir(workspace.project()).unwrap();
        workspace.run(&["init"]);
        fs::write(
            workspace.config(),
            "# the company's libraries\n[scopes]\nacme = \"file:../acme-drawio-libraries\"\n",
        )
        .unwrap();
        workspace
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn project(&self) -> PathBuf {
        self.root().join("project")
    }

    fn config(&self) -> PathBuf {
        self.project().join(".drawio-png/config.toml")
    }

    fn lock(&self) -> String {
        fs::read_to_string(self.project().join(".drawio-png/lock.json")).unwrap()
    }

    fn installed(&self) -> PathBuf {
        self.project().join(".drawio-png/libraries/@acme/brand.xml")
    }

    fn command(&self, args: &[&str]) -> assert_cmd::Command {
        let mut command = cli();
        command
            .current_dir(self.project())
            .env("DRAWIO_PNG_CACHE", self.root().join("cache"))
            .env("DRAWIO_PNG_HOME", self.root().join("home"))
            .args(args);
        command
    }

    fn run(&self, args: &[&str]) -> String {
        stdout(&mut self.command(args))
    }

    fn fail(&self, args: &[&str], code: i32) -> String {
        stderr(&mut self.command(args), code)
    }

    /// Publishes a version of @acme/brand from files in the workspace.
    fn pack(&self, version: &str, sources: &[&Path]) -> String {
        let mut command = self.command(&["library", "pack"]);
        command.args(sources).args([
            "--name",
            "@acme/brand",
            "--title",
            "ACME Brand",
            "--version",
            version,
            "--to",
        ]);
        command.arg(self.root().join("acme-drawio-libraries"));
        stdout(&mut command)
    }
}

fn logo() -> PathBuf {
    examples().join("sources/Logo.svg")
}

#[test]
fn packed_and_installed_files_are_the_examples() {
    let w = Workspace::new();
    let out = w.pack("1.0.0", &[&logo()]);
    assert!(
        out.starts_with("packed @acme/brand@1.0.0 (1 entry) -> "),
        "{out}"
    );
    for file in ["1.0.0.xml", "versions.json"] {
        assert_eq!(
            fs::read(
                w.root()
                    .join("acme-drawio-libraries/@acme/brand")
                    .join(file)
            )
            .unwrap(),
            fs::read(
                examples()
                    .join("acme-drawio-libraries/@acme/brand")
                    .join(file)
            )
            .unwrap(),
            "{file}"
        );
    }
    let out = w.run(&["library", "install", "@acme/brand"]);
    assert_eq!(
        out,
        "installed @acme/brand@1.0.0 (1 entry) from file:../acme-drawio-libraries\n  @acme/brand/logo\timage\t240x120\tLogo\n"
    );
    assert_eq!(
        w.lock(),
        fs::read_to_string(examples().join("project/.drawio-png/lock.json")).unwrap()
    );
    // The dependency is recorded, keeping what was there.
    assert_eq!(
        fs::read_to_string(w.config()).unwrap(),
        "# the company's libraries\n[scopes]\nacme = \"file:../acme-drawio-libraries\"\n\n[dependencies]\n\"@acme/brand\" = \"^1.0.0\"\n"
    );
    assert_eq!(
        fs::read(w.installed()).unwrap(),
        fs::read(examples().join("acme-drawio-libraries/@acme/brand/1.0.0.xml")).unwrap()
    );
    assert_eq!(
        w.run(&["library", "list", "--scope", "acme"]),
        "@acme/brand\t1.0.0\t1\tACME Brand\n"
    );
    // Diagrams use it like any library: case D expands to its expected XML.
    let mut expand = w.command(&["expand"]);
    expand.arg(examples().join("d-data-image/cli.xml"));
    let expanded = stdout(&mut expand);
    support::assert_same_cells(
        &expanded,
        &fs::read_to_string(examples().join("d-data-image/no-cli.xml")).unwrap(),
    );
    // Installing again changes nothing.
    assert_eq!(
        w.run(&["library", "install"]),
        "@acme/brand@1.0.0 is up to date\n"
    );
}

#[test]
fn a_clone_restores_the_locked_versions() {
    let w = Workspace::new();
    w.pack("1.0.0", &[&logo()]);
    w.run(&["library", "install", "@acme/brand"]);
    let xml = examples().join("d-data-image/cli.xml");
    let validate = |w: &Workspace, code: i32| {
        let mut command = w.command(&["validate"]);
        command.arg(&xml);
        stderr(&mut command, code)
    };
    // As after a clone: the installed file is ignored by Git.
    fs::remove_dir_all(w.installed().parent().unwrap()).unwrap();
    assert!(validate(&w, 1).contains("LIBRARY_NOT_INSTALLED"));
    let error = w.fail(&["library", "sync", "--check"], 1);
    assert!(error.contains("LIBRARY_NOT_INSTALLED"), "{error}");
    let lock = w.lock();
    // --offline restores from the cache, --frozen keeps lock.json.
    w.run(&["library", "install", "--frozen", "--offline"]);
    assert_eq!(w.lock(), lock);
    validate(&w, 0);
    w.run(&["library", "sync", "--check"]);
    // Without the cache, the source has it.
    fs::remove_dir_all(w.root().join("cache")).unwrap();
    fs::remove_dir_all(w.installed().parent().unwrap()).unwrap();
    w.run(&["library", "install", "--frozen"]);
    validate(&w, 0);
    // A changed installed file is noticed.
    fs::write(
        w.installed(),
        fs::read_to_string(w.installed())
            .unwrap()
            .replace("ACME Brand", "Edited"),
    )
    .unwrap();
    assert!(validate(&w, 1).contains("LIBRARY_INTEGRITY"));
    assert!(
        w.fail(&["library", "sync", "--check"], 1)
            .contains("LIBRARY_INTEGRITY")
    );
    w.run(&["library", "install"]);
    validate(&w, 0);
    // --frozen refuses a lock that disagrees with config.toml.
    let config = fs::read_to_string(w.config()).unwrap();
    fs::write(w.config(), config.replace("^1.0.0", "^2.0.0")).unwrap();
    let error = w.fail(&["library", "install", "--frozen"], 1);
    assert!(error.contains("LOCK_OUTDATED"), "{error}");
    assert_eq!(w.lock(), lock);
}

#[test]
fn updates_stay_in_range_unless_asked() {
    let w = Workspace::new();
    w.pack("1.0.0", &[&logo()]);
    w.run(&["library", "install", "@acme/brand"]);
    let dark = w.root().join("Logo Dark.svg");
    fs::copy(logo(), &dark).unwrap();
    let out = w.pack("1.1.0", &[&logo(), &dark]);
    assert!(out.contains("added: logo-dark"), "{out}");
    w.pack("2.0.0", &[&dark]);
    let lock = w.lock();
    assert_eq!(
        w.run(&["library", "update", "--dry-run"]),
        "would update @acme/brand 1.0.0 -> 1.1.0 (range ^1.0.0)\n  added: logo-dark\n  removed: (none)\n"
    );
    assert_eq!(w.lock(), lock);
    w.run(&["library", "update"]);
    assert!(w.lock().contains("\"version\": \"1.1.0\""));
    let json: serde_json::Value =
        serde_json::from_str(&w.run(&["library", "update", "--latest", "--json"])).unwrap();
    assert_eq!(json[0]["version"], "2.0.0");
    assert_eq!(json[0]["removed"], serde_json::json!(["@acme/brand/logo"]));
    assert!(
        fs::read_to_string(w.config())
            .unwrap()
            .contains("\"@acme/brand\" = \"^2.0.0\"")
    );
    // An explicit range installs the newest version in it.
    w.run(&["library", "install", "@acme/brand@~1.0.0"]);
    assert!(w.lock().contains("\"version\": \"1.0.0\""));
    assert!(
        fs::read_to_string(w.config())
            .unwrap()
            .contains("\"~1.0.0\"")
    );
}

#[test]
fn packing_keeps_published_versions_and_ids() {
    let w = Workspace::new();
    w.pack("1.0.0", &[&logo()]);
    let mut again = w.command(&["library", "pack"]);
    again
        .arg(logo())
        .args(["--name", "@acme/brand", "--version", "1.0.0", "--to"])
        .arg(w.root().join("acme-drawio-libraries"));
    assert!(stderr(&mut again, 1).contains("VERSION_EXISTS"));
    // A renamed file keeps the entry's ID when its title stays.
    let renamed = w.root().join("renamed");
    fs::create_dir(&renamed).unwrap();
    let source = renamed.join("Logo.svg");
    fs::copy(logo(), &source).unwrap();
    let out = w.pack("1.0.1", &[&source]);
    assert!(
        out.contains("added: (none)") && out.contains("removed: (none)"),
        "{out}"
    );
    // Removing an entry without a new major version is warned about.
    let other = w.root().join("Other.svg");
    fs::copy(logo(), &other).unwrap();
    let mut pack = w.command(&["library", "pack"]);
    pack.arg(&other)
        .args(["--name", "@acme/brand", "--version", "1.1.0", "--to"])
        .arg(w.root().join("acme-drawio-libraries"));
    let output = pack.assert().success().get_output().clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning[ENTRY_REMOVED]"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("removed: logo"));
    // Unscoped or built-in names are not packed.
    for name in ["brand", "@drawio/brand"] {
        let mut pack = w.command(&["library", "pack"]);
        pack.arg(logo())
            .args(["--name", name, "--version", "1.0.0", "--to"])
            .arg(w.root().join("acme-drawio-libraries"));
        stderr(&mut pack, 2);
    }
}

#[test]
fn install_problems_change_nothing() {
    let w = Workspace::new();
    w.pack("1.0.0", &[&logo()]);
    let config = fs::read_to_string(w.config()).unwrap();
    for (args, code, expected) in [
        (
            vec!["library", "install", "@other/brand"],
            1,
            "SCOPE_UNKNOWN",
        ),
        (
            vec!["library", "install", "@acme/brand@^2.0.0"],
            1,
            "VERSION_NOT_FOUND",
        ),
        (
            vec!["library", "install", "@acme/missing"],
            1,
            "SOURCE_FETCH",
        ),
        (vec!["library", "install", "brand"], 2, "USAGE"),
        (vec!["library", "install", "@drawio/general"], 2, "USAGE"),
        (vec!["library", "install", "@acme/brand@latest"], 2, "USAGE"),
        (
            vec!["library", "install", "--offline", "@acme/brand"],
            1,
            "SOURCE_FETCH",
        ),
        (vec!["library", "update", "@acme/brand"], 1, "REF_NOT_FOUND"),
        (vec!["library", "remove", "@acme/brand"], 1, "REF_NOT_FOUND"),
    ] {
        let error = w.fail(&args, code);
        assert!(error.contains(expected), "{args:?}: {error}");
        assert_eq!(fs::read_to_string(w.config()).unwrap(), config, "{args:?}");
        assert!(
            !w.project().join(".drawio-png/lock.json").exists(),
            "{args:?}"
        );
    }
    // A source file that does not match versions.json is refused.
    let file = w.root().join("acme-drawio-libraries/@acme/brand/1.0.0.xml");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replace("Logo", "Changed")).unwrap();
    let error = w.fail(&["library", "install", "@acme/brand"], 1);
    assert!(error.contains("LIBRARY_INTEGRITY"), "{error}");
    assert!(!w.installed().exists());
    assert_eq!(fs::read_to_string(w.config()).unwrap(), config);
}

#[test]
fn removing_uninstalls_everywhere() {
    let w = Workspace::new();
    w.pack("1.0.0", &[&logo()]);
    w.run(&["library", "install", "@acme/brand"]);
    assert_eq!(
        w.run(&["library", "remove", "@acme/brand"]),
        "removed @acme/brand\n"
    );
    assert!(!w.installed().exists());
    assert!(!w.lock().contains("@acme/brand"));
    assert!(
        !fs::read_to_string(w.config())
            .unwrap()
            .contains("@acme/brand")
    );
    // A library left out of config.toml is removed by install.
    w.run(&["library", "install", "@acme/brand"]);
    let config = fs::read_to_string(w.config()).unwrap();
    fs::write(
        w.config(),
        config.replace("\"@acme/brand\" = \"^1.0.0\"\n", ""),
    )
    .unwrap();
    assert_eq!(w.run(&["library", "install"]), "removed @acme/brand\n");
    assert!(!w.installed().exists());
}
