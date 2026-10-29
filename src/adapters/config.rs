//! `config.toml` (spec 2.4) in the project area and the user area: read into
//! settings, and `[dependencies]` (the project's, or the global libraries' in
//! the user's file) edited in place, keeping comments, key order and the
//! other sections.
use std::{
    env,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, TableLike, Value};

use crate::{
    adapters::{fs, render::RendererKind},
    app::ports::DependencyStore,
    domain::{
        diagnostic::{Code, Diagnostic},
        fonts,
        reference::{LibraryName, is_name},
        settings::{Layer, RenderSettings, Scope, Settings},
        source::Source,
        version::Range,
    },
};

pub const FILE: &str = "config.toml";

/// The user area (spec 2.2): `D_PNG_HOME`, or the OS's configuration
/// directory.
pub fn user_area() -> Option<PathBuf> {
    if let Some(home) = env::var_os("D_PNG_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(home));
    }
    let var = |key: &str| {
        env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = match env::consts::OS {
        "windows" => var("APPDATA")?,
        "macos" => var("HOME")?.join("Library/Application Support"),
        _ => var("XDG_CONFIG_HOME").or_else(|| Some(var("HOME")?.join(".config")))?,
    };
    Some(base.join("d-png-cli"))
}

/// One `config.toml`; a missing file reads as empty.
pub struct ConfigFile {
    path: PathBuf,
    layer: Layer,
    doc: DocumentMut,
}

impl ConfigFile {
    pub fn load(path: &Path, layer: Layer) -> Result<Self, Diagnostic> {
        let doc = if path.is_file() {
            let bytes = fs::read(path).map_err(|e| Diagnostic::from_error(Code::InputRead, &e))?;
            let text = String::from_utf8(bytes).map_err(|_| {
                Diagnostic::new(
                    Code::ConfigInvalid,
                    format!("{}: not UTF-8", path.display()),
                )
            })?;
            text.parse::<DocumentMut>().map_err(|e| {
                Diagnostic::new(
                    Code::ConfigInvalid,
                    format!("{}: {}", path.display(), e.to_string().trim()),
                )
            })?
        } else {
            DocumentMut::new()
        };
        Ok(Self {
            path: path.to_owned(),
            layer,
            doc,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn invalid(&self, message: impl std::fmt::Display) -> Diagnostic {
        Diagnostic::new(
            Code::ConfigInvalid,
            format!("{}: {message}", self.path.display()),
        )
    }

    /// The settings of this file; unknown keys and wrong types are errors.
    pub fn settings(&self) -> Result<Settings, Diagnostic> {
        let mut settings = Settings::default();
        for (key, item) in self.doc.iter() {
            let table = item
                .as_table_like()
                .ok_or_else(|| self.invalid(format!("{key} must be a table")))?;
            match key {
                "render" => settings.render = self.render(table)?,
                "scopes" => {
                    for (scope, value) in table.iter() {
                        if !is_name(scope) {
                            return Err(self.invalid(format!(
                                "[scopes] {scope:?} is not a scope name that can have a source"
                            )));
                        }
                        let source = self.source(scope, value)?;
                        settings.scopes.insert(
                            scope.to_owned(),
                            Scope {
                                source,
                                layer: self.layer,
                            },
                        );
                    }
                }
                "dependencies" => {
                    for (name, value) in table.iter() {
                        let name = LibraryName::parse(name)
                            .ok()
                            .filter(|n| n.scope().is_some())
                            .ok_or_else(|| {
                                self.invalid(format!(
                                    "[dependencies] {name:?} is not a scoped library name such as \"@acme/brand\""
                                ))
                            })?;
                        let range: Range = value
                            .as_str()
                            .ok_or_else(|| {
                                self.invalid(format!("[dependencies] {name} must be a string"))
                            })?
                            .parse()
                            .map_err(|e| self.invalid(format!("[dependencies] {name}: {e}")))?;
                        settings.dependencies.insert(name, range);
                    }
                }
                other => return Err(self.invalid(format!("unknown section [{other}]"))),
            }
        }
        Ok(settings)
    }

    fn render(&self, table: &dyn TableLike) -> Result<RenderSettings, Diagnostic> {
        let mut render = RenderSettings::default();
        for (key, item) in table.iter() {
            let text = || {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| self.invalid(format!("[render] {key} must be a string")))
            };
            let family = |name: String| {
                fonts::parse_family(&name)
                    .map_err(|e| self.invalid(format!("[render] {key} {name:?}: {e}")))
            };
            match key {
                "renderer" => {
                    let name = text()?;
                    if RendererKind::parse(&name).is_none() {
                        return Err(self.invalid(format!(
                            "[render] renderer {name:?}: expected {}",
                            RendererKind::NAMES.join(", ")
                        )));
                    }
                    render.renderer = Some(name);
                }
                "default-font" => render.default_font = Some(family(text()?)?),
                "fallback-fonts" => {
                    let fonts = item
                        .as_array()
                        .and_then(|a| {
                            a.iter()
                                .map(|v| v.as_str().map(str::to_owned))
                                .collect::<Option<Vec<_>>>()
                        })
                        .ok_or_else(|| {
                            self.invalid("[render] fallback-fonts must be an array of strings")
                        })?;
                    render.fallback_fonts =
                        Some(fonts.into_iter().map(family).collect::<Result<_, _>>()?);
                }
                "allow-network" => {
                    render.allow_network = Some(item.as_bool().ok_or_else(|| {
                        self.invalid("[render] allow-network must be true or false")
                    })?)
                }
                other => return Err(self.invalid(format!("unknown key [render] {other}"))),
            }
        }
        Ok(render)
    }

    /// A `[scopes]` value: a URL, or a table with `url` and `token-env`.
    fn source(&self, scope: &str, item: &Item) -> Result<Source, Diagnostic> {
        let (url, token) = if let Some(url) = item.as_str() {
            (url.to_owned(), None)
        } else if let Some(table) = item.as_table_like() {
            let mut url = None;
            let mut token = None;
            for (key, value) in table.iter() {
                let text = value.as_str().map(str::to_owned).ok_or_else(|| {
                    self.invalid(format!("[scopes] {scope}.{key} must be a string"))
                })?;
                match key {
                    "url" => url = Some(text),
                    "token-env" => token = Some(text),
                    other => {
                        return Err(self.invalid(format!("unknown key [scopes] {scope}.{other}")));
                    }
                }
            }
            (
                url.ok_or_else(|| self.invalid(format!("[scopes] {scope} needs a url")))?,
                token,
            )
        } else {
            return Err(self.invalid(format!(
                "[scopes] {scope} must be a URL or {{ url = \"…\", token-env = \"…\" }}"
            )));
        };
        Source::parse(&url, token.as_deref())
            .map_err(|e| self.invalid(format!("[scopes] {scope}: {e}")))
    }

    /// Sets a dependency's range, adding `[dependencies]` if needed.
    pub fn set_dependency(&mut self, name: &LibraryName, range: &Range) {
        if !self.doc.contains_key("dependencies") {
            self.doc["dependencies"] = toml_edit::table();
        }
        let key = name.to_string();
        if let Some(table) = self.doc["dependencies"].as_table_like_mut() {
            match table.get_mut(&key) {
                Some(Item::Value(Value::String(old))) => {
                    // Keep the comments around the old value.
                    let decor = old.decor().clone();
                    let mut value = toml_edit::Formatted::new(range.to_string());
                    *value.decor_mut() = decor;
                    *old = value;
                }
                _ => {
                    table.insert(&key, toml_edit::value(range.to_string()));
                }
            }
        }
    }

    /// Removes a dependency; false if there was none.
    pub fn remove_dependency(&mut self, name: &LibraryName) -> bool {
        self.doc
            .get_mut("dependencies")
            .and_then(Item::as_table_like_mut)
            .is_some_and(|t| t.remove(&name.to_string()).is_some())
    }

    pub fn save(&self) -> Result<(), Diagnostic> {
        fs::atomic_write(&self.path, self.doc.to_string().as_bytes())
            .map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e))
    }
}

/// The project's `config.toml` over the user's, as one store of settings
/// whose project `[dependencies]` can change; or, for the global libraries,
/// the user's alone with its `[dependencies]`.
pub struct Configuration {
    /// The project's file, if there is a project.
    pub project: Option<PathBuf>,
    pub user: Option<PathBuf>,
    /// Whether the dependencies are the global libraries' (the user's file).
    pub global: bool,
}

impl Configuration {
    /// The files of a project area (`.d-png/`) and the user area.
    pub fn new(area: Option<&Path>) -> Self {
        Self {
            project: area.map(|a| a.join(FILE)),
            user: user_area().map(|a| a.join(FILE)),
            global: false,
        }
    }

    /// The user's file, whose `[dependencies]` are the global libraries'.
    pub fn global() -> Self {
        Self {
            project: None,
            user: user_area().map(|a| a.join(FILE)),
            global: true,
        }
    }
}

impl DependencyStore for Configuration {
    fn settings(&self) -> Result<Settings, Diagnostic> {
        let mut user = match &self.user {
            Some(path) => ConfigFile::load(path, Layer::User)?.settings()?,
            None => Settings::default(),
        };
        if self.global {
            return Ok(user);
        }
        // The user's dependencies are the global libraries', not the project's.
        user.dependencies.clear();
        Ok(match &self.project {
            Some(path) => ConfigFile::load(path, Layer::Project)?
                .settings()?
                .over(user),
            None => user,
        })
    }

    fn change(&self, changes: &[(LibraryName, Option<Range>)]) -> Result<(), Diagnostic> {
        let (path, layer) = if self.global {
            let path = self.user.as_ref().ok_or_else(no_user_area)?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| {
                    Diagnostic::new(
                        Code::OutputWrite,
                        format!("cannot create {}: {e}", dir.display()),
                    )
                })?;
            }
            (path, Layer::User)
        } else {
            let path = self.project.as_ref().ok_or_else(|| {
                Diagnostic::new(
                    Code::ProjectNotFound,
                    "no project (.d-png/) here or above; run `d-png-cli init`, or use --global",
                )
            })?;
            (path, Layer::Project)
        };
        let mut file = ConfigFile::load(path, layer)?;
        // Check the file before changing it.
        file.settings()?;
        for (name, range) in changes {
            match range {
                Some(range) => file.set_dependency(name, range),
                None => {
                    file.remove_dependency(name);
                }
            }
        }
        file.save()
    }
}

/// The error when neither `D_PNG_HOME` nor the OS gives a user area.
pub fn no_user_area() -> Diagnostic {
    Diagnostic::new(
        Code::ProjectNotFound,
        "no user area for the global libraries; set D_PNG_HOME",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str, layer: Layer) -> Result<Settings, Diagnostic> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, text).unwrap();
        ConfigFile::load(&path, layer)?.settings()
    }

    #[test]
    fn every_section_is_read() {
        let settings = file(
            r#"
[render]
renderer = "desktop-compat"
default-font = "Noto Sans CJK JP"
fallback-fonts = ["Noto Color Emoji"]
allow-network = true

[scopes]
acme = "git+https://github.com/acme/d-png-libraries.git"
partner = { url = "https://libs.example.com/d-png/", token-env = "PARTNER_TOKEN" }

[dependencies]
"@acme/brand" = "^1.0.0"
"#,
            Layer::Project,
        )
        .unwrap();
        assert_eq!(settings.render.renderer.as_deref(), Some("desktop-compat"));
        assert_eq!(settings.render.allow_network, Some(true));
        assert_eq!(
            settings.scopes["partner"].source,
            Source::Https {
                url: "https://libs.example.com/d-png/".into(),
                token_env: Some("PARTNER_TOKEN".into())
            }
        );
        assert_eq!(
            settings.dependencies[&LibraryName::parse("@acme/brand").unwrap()].to_string(),
            "^1.0.0"
        );
    }

    #[test]
    fn unknown_keys_and_misplaced_settings_are_errors() {
        for (text, layer) in [
            ("[render]\nrenderer = 1\n", Layer::Project),
            ("[render]\nrenderer = \"chromium\"\n", Layer::Project),
            ("[render]\ndefault-font = \"A;B\"\n", Layer::Project),
            ("[render]\ncolour = \"red\"\n", Layer::Project),
            ("[other]\n", Layer::Project),
            ("top = 1\n", Layer::Project),
            ("[scopes]\nAcme = \"file:x\"\n", Layer::Project),
            ("[scopes]\nacme = \"http://x/\"\n", Layer::Project),
            (
                "[scopes]\nacme = { url = \"https://x/\", token = \"t\" }\n",
                Layer::Project,
            ),
            ("[dependencies]\n\"@acme/brand\" = \"^1\"\n", Layer::Project),
            ("[dependencies]\nbrand = \"^1.0.0\"\n", Layer::Project),
            ("[dependencies]\nbrand = \"^1.0.0\"\n", Layer::User),
            ("not toml", Layer::Project),
        ] {
            let error = file(text, layer).unwrap_err();
            assert_eq!(error.code, Code::ConfigInvalid, "{text}");
            assert!(error.message.contains(FILE), "{}", error.message);
        }
    }

    #[test]
    fn the_users_dependencies_are_the_global_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(
            project.join(FILE),
            "[dependencies]\n\"@acme/brand\" = \"^1.0.0\"\n",
        )
        .unwrap();
        let user = dir.path().join("user").join(FILE);
        std::fs::create_dir(user.parent().unwrap()).unwrap();
        std::fs::write(&user, "[dependencies]\n\"@mine/icons\" = \"^2.0.0\"\n").unwrap();
        let configuration = |project: Option<&Path>, global| Configuration {
            project: project.map(|p| p.join(FILE)),
            user: Some(user.clone()),
            global,
        };
        let names = |c: Configuration| -> Vec<String> {
            let settings = c.settings().unwrap();
            settings
                .dependencies
                .keys()
                .map(|n| n.to_string())
                .collect()
        };
        assert_eq!(names(configuration(Some(&project), false)), ["@acme/brand"]);
        assert!(names(configuration(None, false)).is_empty());
        assert_eq!(names(configuration(None, true)), ["@mine/icons"]);
        // A global change writes the user's file.
        let icons = LibraryName::parse("@mine/icons").unwrap();
        configuration(None, true)
            .change(&[(icons, Some("^2.1.0".parse().unwrap()))])
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&user).unwrap(),
            "[dependencies]\n\"@mine/icons\" = \"^2.1.0\"\n"
        );
    }

    #[test]
    fn dependencies_are_edited_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(
            &path,
            "# Project settings\n[render]\nrenderer = \"vscode\" # the default\n\n[dependencies]\n\"@acme/brand\" = \"^1.0.0\" # logos\n",
        )
        .unwrap();
        let mut config = ConfigFile::load(&path, Layer::Project).unwrap();
        let brand = LibraryName::parse("@acme/brand").unwrap();
        let icons = LibraryName::parse("@acme/icons").unwrap();
        config.set_dependency(&brand, &"^1.2.0".parse().unwrap());
        config.set_dependency(&icons, &"~2.0.0".parse().unwrap());
        config.save().unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Project settings\n[render]\nrenderer = \"vscode\" # the default\n\n[dependencies]\n\"@acme/brand\" = \"^1.2.0\" # logos\n\"@acme/icons\" = \"~2.0.0\"\n"
        );
        let mut config = ConfigFile::load(&path, Layer::Project).unwrap();
        assert!(config.remove_dependency(&icons));
        assert!(!config.remove_dependency(&icons));
        // A file without dependencies gets the section.
        let path = dir.path().join("empty.toml");
        let mut config = ConfigFile::load(&path, Layer::Project).unwrap();
        config.set_dependency(&brand, &"^1.0.0".parse().unwrap());
        config.save().unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[dependencies]\n\"@acme/brand\" = \"^1.0.0\"\n"
        );
    }
}
