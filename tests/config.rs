//! `config.toml` (spec 2.4): rendering defaults from the project and the
//! user area, with options and the environment taking precedence.
mod support;

use drawio_png_cli::domain::drawio_png;
use std::{fs, path::Path};
use support::{cli, stderr};
use tempfile::tempdir;

const MODEL: &str = include_str!("fixtures/model.xml");

fn project(config: &str) -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    cli().arg("init").arg(dir.path()).assert().success();
    fs::write(dir.path().join(".drawio-png/config.toml"), config).unwrap();
    dir
}

/// The XML a metadata-only write saves, run from `dir`.
fn saved(dir: &Path, args: &[&str]) -> String {
    let output = dir.join("out.png");
    cli()
        .current_dir(dir)
        .env("DRAWIO_PNG_HOME", dir.join("no-user-area"))
        .args(["write", "--metadata-only", "-o"])
        .arg(&output)
        .args(args)
        .write_stdin(MODEL)
        .assert()
        .success();
    drawio_png::extract(&fs::read(output).unwrap()).unwrap()
}

#[test]
fn fonts_come_from_options_then_the_project_then_the_user() {
    let dir = project(
        "[render]\ndefault-font = \"Project Sans\"\nfallback-fonts = [\"Noto Color Emoji\"]\n",
    );
    let font = |xml: &str| {
        let doc = roxmltree::Document::parse(xml).unwrap();
        doc.descendants()
            .find_map(|n| n.attribute("style"))
            .unwrap()
            .to_owned()
    };
    let xml = saved(dir.path(), &[]);
    assert_eq!(
        font(&xml),
        "fontFamily=\"Project Sans\", \"Noto Color Emoji\";"
    );
    let xml = saved(dir.path(), &["--default-font", "Option Sans"]);
    assert_eq!(font(&xml), "fontFamily=\"Option Sans\";");
    // expand applies the same settings.
    let out = String::from_utf8(
        cli()
            .current_dir(dir.path())
            .arg("expand")
            .write_stdin(MODEL)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    assert_eq!(font(&out), font(&saved(dir.path(), &[])));
    // The user area applies where the project says nothing.
    let user = tempdir().unwrap();
    fs::write(
        user.path().join("config.toml"),
        "[render]\ndefault-font = \"User Sans\"\n",
    )
    .unwrap();
    let plain = project("# nothing\n");
    let output = plain.path().join("out.png");
    cli()
        .current_dir(plain.path())
        .env("DRAWIO_PNG_HOME", user.path())
        .args(["write", "--metadata-only", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    let xml = drawio_png::extract(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(font(&xml), "fontFamily=\"User Sans\";");
    cli()
        .current_dir(dir.path())
        .env("DRAWIO_PNG_HOME", user.path())
        .args(["write", "--metadata-only", "-o"])
        .arg(&output)
        .write_stdin(MODEL)
        .assert()
        .success();
    let xml = drawio_png::extract(&fs::read(&output).unwrap()).unwrap();
    assert!(font(&xml).contains("Project Sans"), "{xml}");
}

#[test]
fn invalid_settings_are_reported_where_they_are_used() {
    for config in [
        "[render]\nrenderer = \"chromium\"\n",
        "[render]\ncolour = \"red\"\n",
        "[render]\nfallback-fonts = [\"A\"]\n",
        "[render]\ndefault-font = \"A;B\"\n",
        "[scopes]\nacme = \"http://insecure/\"\n",
        "[dependencies]\n\"@acme/brand\" = \"latest\"\n",
        "not toml",
    ] {
        let dir = project(config);
        let output = dir.path().join("out.png");
        fs::write(&output, b"original").unwrap();
        let error = stderr(
            cli()
                .current_dir(dir.path())
                .args(["write", "-o"])
                .arg(&output)
                .write_stdin(MODEL),
            1,
        );
        assert!(error.contains("CONFIG_INVALID"), "{config}: {error}");
        assert!(error.contains("config.toml"), "{config}: {error}");
        assert_eq!(fs::read(&output).unwrap(), b"original");
        // Commands that neither render nor install do not read it.
        cli()
            .current_dir(dir.path())
            .arg("validate")
            .write_stdin(MODEL)
            .assert()
            .success();
    }
    // Dependencies belong to the project.
    let user = tempdir().unwrap();
    fs::write(
        user.path().join("config.toml"),
        "[dependencies]\n\"@acme/brand\" = \"^1.0.0\"\n",
    )
    .unwrap();
    let dir = project("");
    let error = stderr(
        cli()
            .current_dir(dir.path())
            .env("DRAWIO_PNG_HOME", user.path())
            .arg("expand")
            .write_stdin(MODEL),
        1,
    );
    assert!(
        error.contains("[dependencies] belongs in the project"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn the_renderer_comes_from_options_the_environment_then_settings() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project("[render]\nrenderer = \"desktop-app\"\nallow-network = true\n");
    let script = |name: &str, body: &str| {
        let path = dir.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let desktop = script("desktop", "echo desktop-selected >&2; exit 4");
    let chrome = script(
        "chrome",
        r#"printf '%s\n' "$@" > "$(dirname "$0")/args"; echo browser-selected >&2; exit 4"#,
    );
    let run = |args: &[&str], renderer_env: Option<&str>| {
        let mut command = cli();
        command
            .current_dir(dir.path())
            .env("DRAWIO_PNG_HOME", dir.path().join("no-user-area"))
            .env("DRAWIO_PNG_DRAWIO_PATH", &desktop)
            .env("DRAWIO_PNG_CHROME_PATH", &chrome)
            .args(["write", "-o", "out.png"])
            .args(args)
            .write_stdin(MODEL);
        if let Some(value) = renderer_env {
            command.env("DRAWIO_PNG_RENDERER", value);
        }
        // desktop-app with allow-network from the settings is refused before rendering.
        let output = command.assert().failure().get_output().clone();
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let error = run(&[], None);
    assert!(
        error.contains("--allow-network needs a Chromium renderer"),
        "{error}"
    );
    let error = run(&[], Some("vscode"));
    assert!(error.contains("browser-selected"), "{error}");
    // allow-network = true reaches Chromium: only service hosts are blocked.
    let args = fs::read_to_string(dir.path().join("args")).unwrap();
    assert!(!args.contains("MAP * ~NOTFOUND"), "{args}");
    let error = run(&["--renderer", "raw"], Some("desktop-app"));
    assert!(error.contains("browser-selected"), "{error}");
    fs::write(
        dir.path().join(".drawio-png/config.toml"),
        "[render]\nrenderer = \"desktop-app\"\n",
    )
    .unwrap();
    let error = run(&[], None);
    assert!(error.contains("desktop-selected"), "{error}");
}
