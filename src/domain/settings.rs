//! The settings of `config.toml` (spec 2.4), already read: rendering
//! defaults, where scopes come from, and the project's dependencies.
use std::collections::BTreeMap;

use crate::domain::{reference::LibraryName, source::Source, version::Range};

/// `[render]`: defaults for rendering and fonts. `None` leaves the choice to
/// a lower layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderSettings {
    pub renderer: Option<String>,
    pub default_font: Option<String>,
    pub fallback_fonts: Option<Vec<String>>,
    pub allow_network: Option<bool>,
}

impl RenderSettings {
    /// These settings over `lower`, one key at a time.
    pub fn over(self, lower: RenderSettings) -> RenderSettings {
        RenderSettings {
            renderer: self.renderer.or(lower.renderer),
            default_font: self.default_font.or(lower.default_font),
            fallback_fonts: self.fallback_fonts.or(lower.fallback_fonts),
            allow_network: self.allow_network.or(lower.allow_network),
        }
    }
}

/// Which `config.toml` a setting comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Project,
    User,
}

/// A scope's source and the configuration that names it, from whose
/// directory a relative `file:` path is resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub source: Source,
    pub layer: Layer,
}

/// One `config.toml`, or the project's merged over the user's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub render: RenderSettings,
    /// `[scopes]`: the source of each scope.
    pub scopes: BTreeMap<String, Scope>,
    /// `[dependencies]`: the installed libraries and their ranges; only in
    /// the project's configuration.
    pub dependencies: BTreeMap<LibraryName, Range>,
}

impl Settings {
    /// The project's settings over the user's: rendering keys and scopes one
    /// at a time; dependencies only from the project.
    pub fn over(self, user: Settings) -> Settings {
        let mut scopes = user.scopes;
        scopes.extend(self.scopes);
        Settings {
            render: self.render.over(user.render),
            scopes,
            dependencies: self.dependencies,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_wins_one_key_at_a_time() {
        let file = |path: &str, layer| Scope {
            source: Source::File { path: path.into() },
            layer,
        };
        let project = Settings {
            render: RenderSettings {
                renderer: Some("raw".into()),
                ..Default::default()
            },
            scopes: BTreeMap::from([("acme".to_owned(), file("project", Layer::Project))]),
            ..Default::default()
        };
        let user = Settings {
            render: RenderSettings {
                renderer: Some("vscode".into()),
                default_font: Some("Noto Sans".into()),
                ..Default::default()
            },
            scopes: BTreeMap::from([
                ("acme".to_owned(), file("user", Layer::User)),
                ("mine".to_owned(), file("home", Layer::User)),
            ]),
            ..Default::default()
        };
        let merged = project.over(user);
        assert_eq!(merged.render.renderer.as_deref(), Some("raw"));
        assert_eq!(merged.render.default_font.as_deref(), Some("Noto Sans"));
        assert_eq!(merged.scopes["acme"], file("project", Layer::Project));
        assert_eq!(merged.scopes["mine"], file("home", Layer::User));
    }
}
