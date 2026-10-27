//! Where the libraries of a scope come from (spec 9.5): a directory, a Git
//! repository or an HTTPS server, all holding `@<scope>/<name>/versions.json`
//! and one file per version.
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// `file:<path>`; a relative path is resolved from the directory of the
    /// configuration that names it.
    File { path: String },
    /// `git+https://…` or `git+ssh://…`, with an optional `#<branch or tag>`.
    Git {
        url: String,
        reference: Option<String>,
    },
    /// `https://…/`, with the name of an environment variable holding a token.
    Https {
        url: String,
        token_env: Option<String>,
    },
}

impl Source {
    /// Parses a `[scopes]` value: the URL and, for HTTPS, a `token-env`.
    pub fn parse(url: &str, token_env: Option<&str>) -> Result<Self, String> {
        let source = if let Some(path) = url.strip_prefix("file:") {
            if path.is_empty() {
                return Err("a file: source needs a path".into());
            }
            Source::File {
                path: path.to_owned(),
            }
        } else if let Some(rest) = url.strip_prefix("git+") {
            if !(rest.starts_with("https://") || rest.starts_with("ssh://")) {
                return Err(format!(
                    "invalid Git source {url:?}: expected git+https:// or git+ssh://"
                ));
            }
            let (url, reference) = match rest.split_once('#') {
                Some((url, reference)) if !reference.is_empty() => {
                    (url.to_owned(), Some(reference.to_owned()))
                }
                Some((url, _)) => (url.to_owned(), None),
                None => (rest.to_owned(), None),
            };
            Source::Git { url, reference }
        } else if url.starts_with("https://") {
            Source::Https {
                url: url.to_owned(),
                token_env: token_env.map(str::to_owned),
            }
        } else {
            return Err(format!(
                "invalid source {url:?}: expected file:, git+https://, git+ssh:// or https://"
            ));
        };
        if token_env.is_some() && !matches!(source, Source::Https { .. }) {
            return Err("token-env is only for https:// sources".into());
        }
        if let Some(name) = token_env
            && (name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            return Err(format!(
                "invalid token-env {name:?}: expected an environment variable name"
            ));
        }
        Ok(source)
    }
}

/// The source as written in `[scopes]` and `lock.json`, without its token.
impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::File { path } => write!(f, "file:{path}"),
            Source::Git {
                url,
                reference: Some(reference),
            } => write!(f, "git+{url}#{reference}"),
            Source::Git { url, .. } => write!(f, "git+{url}"),
            Source::Https { url, .. } => f.write_str(url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_parse_by_their_prefix() {
        assert_eq!(
            Source::parse("file:../libraries", None).unwrap(),
            Source::File {
                path: "../libraries".into()
            }
        );
        let git = Source::parse(
            "git+https://github.com/acme/drawio-libraries.git#release",
            None,
        )
        .unwrap();
        assert_eq!(
            git,
            Source::Git {
                url: "https://github.com/acme/drawio-libraries.git".into(),
                reference: Some("release".into())
            }
        );
        assert_eq!(
            git.to_string(),
            "git+https://github.com/acme/drawio-libraries.git#release"
        );
        let https = Source::parse("https://libs.example.com/drawio/", Some("TOKEN")).unwrap();
        assert_eq!(https.to_string(), "https://libs.example.com/drawio/");
        for (url, token) in [
            ("http://insecure/", None),
            ("git+file:///x", None),
            ("file:", None),
            ("file:x", Some("TOKEN")),
            ("https://x/", Some("not a name")),
            ("ftp://x/", None),
        ] {
            assert!(Source::parse(url, token).is_err(), "{url}");
        }
    }
}
