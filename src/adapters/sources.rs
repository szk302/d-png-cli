//! The sources of scopes (spec 9.5): directories, Git repositories (through
//! the `git` command) and HTTPS servers (through `curl`), all laid out as
//! `@<scope>/<name>/versions.json` and one file per version. Fetched files are
//! cached by integrity, which also serves `--offline`.
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    cell::RefCell,
    collections::HashMap,
    env, fs as stdfs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    MAX_BYTES,
    adapters::fs,
    app::ports::{Publication, Registry},
    domain::{
        diagnostic::{Code, Diagnostic},
        reference::LibraryName,
        registry::{Versions, integrity},
        settings::{Layer, Scope},
        source::Source,
        version::Version,
    },
};

/// The cache of fetched files (spec 2.2): `D_PNG_CACHE`, or the OS's
/// cache directory.
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("D_PNG_CACHE").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let var = |key: &str| {
        env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = match env::consts::OS {
        "windows" => var("LOCALAPPDATA")?,
        "macos" => var("HOME")?.join("Library/Caches"),
        _ => var("XDG_CACHE_HOME").or_else(|| Some(var("HOME")?.join(".cache")))?,
    };
    Some(base.join("d-png-cli"))
}

/// Files by integrity.
pub struct Cache {
    dir: Option<PathBuf>,
}

impl Cache {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    fn path(&self, integrity: &str) -> Option<PathBuf> {
        let digest = STANDARD.decode(integrity.strip_prefix("sha256-")?).ok()?;
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        Some(
            self.dir
                .as_ref()?
                .join("libraries")
                .join(format!("{hex}.xml")),
        )
    }

    fn get(&self, expected: &str) -> Option<Vec<u8>> {
        let bytes = fs::read(&self.path(expected)?).ok()?;
        (integrity(&bytes) == expected).then_some(bytes)
    }

    /// Keeps a file whose integrity is as expected; failures only lose the cache.
    fn put(&self, expected: &str, bytes: &[u8]) {
        if integrity(bytes) != expected {
            return;
        }
        if let Some(path) = self.path(expected)
            && let Some(parent) = path.parent()
            && stdfs::create_dir_all(parent).is_ok()
        {
            let _ = fs::atomic_write(&path, bytes);
        }
    }
}

fn fetch_error(message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(Code::SourceFetch, message.to_string())
}

/// `@<scope>/<name>`, the library's place in a source.
fn place(name: &LibraryName) -> String {
    format!("@{}/{}", name.scope().unwrap_or(""), name.name())
}

/// A source directory: a `file:` source, a Git checkout, or what `library
/// pack` writes into.
pub struct SourceDirectory {
    root: PathBuf,
}

impl SourceDirectory {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn library(&self, name: &LibraryName) -> PathBuf {
        self.root.join(place(name))
    }

    fn read_versions(&self, name: &LibraryName) -> Result<Option<Versions>> {
        let path = self.library(name).join("versions.json");
        if !path.is_file() {
            return Ok(None);
        }
        Versions::parse(&fs::read(&path)?, name)
            .with_context(|| format!("{}", path.display()))
            .map(Some)
    }

    fn read_file(&self, name: &LibraryName, file: &str) -> Result<Vec<u8>> {
        ensure!(
            !file.contains(['/', '\\']) && file != ".." && file != ".",
            "invalid file name {file:?}"
        );
        fs::read(&self.library(name).join(file))
    }
}

impl Publication for SourceDirectory {
    fn versions(&self, name: &LibraryName) -> Result<Option<Versions>, Diagnostic> {
        self.read_versions(name)
            .map_err(|e| Diagnostic::from_error(Code::LibraryInvalid, &e))
    }

    fn file(&self, name: &LibraryName, file: &str) -> Result<Vec<u8>, Diagnostic> {
        self.read_file(name, file)
            .map_err(|e| Diagnostic::from_error(Code::InputRead, &e))
    }

    fn publish(
        &self,
        name: &LibraryName,
        file: &str,
        bytes: &[u8],
        versions: &Versions,
    ) -> Result<(), Diagnostic> {
        let dir = self.library(name);
        let write = || -> Result<()> {
            stdfs::create_dir_all(&dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
            fs::atomic_write(&dir.join(file), bytes)?;
            fs::atomic_write(&dir.join("versions.json"), versions.to_json().as_bytes())
        };
        write().map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e))
    }
}

/// Runs `git` without prompting for credentials.
fn git(args: &[&str], dir: Option<&Path>) -> Result<String> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    let output = command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .context("cannot run git; install Git to use git+ sources")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.first().unwrap_or(&""),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A shallow checkout of a branch or tag (or the default branch), and its commit.
fn clone(url: &str, reference: Option<&str>) -> Result<(tempfile::TempDir, String)> {
    let dir = tempfile::Builder::new()
        .prefix("d-png-cli-git-")
        .tempdir()?;
    let target = dir.path().to_string_lossy().into_owned();
    let mut args = vec!["clone", "--quiet", "--depth", "1"];
    if let Some(reference) = reference {
        args.extend(["--branch", reference]);
    }
    args.extend(["--", url, &target]);
    git(&args, None)?;
    let commit = git(&["rev-parse", "HEAD"], Some(dir.path()))?;
    Ok((dir, commit))
}

/// A checkout of one commit; a full clone if the server cannot fetch it alone.
fn checkout(url: &str, commit: &str) -> Result<tempfile::TempDir> {
    ensure!(
        commit.len() >= 7 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid commit {commit:?} in lock.json"
    );
    let dir = tempfile::Builder::new()
        .prefix("d-png-cli-git-")
        .tempdir()?;
    git(&["init", "--quiet"], Some(dir.path()))?;
    let fetched = git(
        &["fetch", "--quiet", "--depth", "1", "--", url, commit],
        Some(dir.path()),
    );
    if fetched.is_err() {
        git(&["fetch", "--quiet", "--", url], Some(dir.path()))?;
    }
    git(&["checkout", "--quiet", commit], Some(dir.path()))
        .or_else(|_| git(&["checkout", "--quiet", "FETCH_HEAD"], Some(dir.path())))?;
    let head = git(&["rev-parse", "HEAD"], Some(dir.path()))?;
    ensure!(head.starts_with(commit), "{url} has no commit {commit}");
    Ok(dir)
}

/// A file from an HTTPS server through `curl`; the token goes through
/// standard input, never the command line.
fn download(url: &str, token: Option<&str>) -> Result<Vec<u8>> {
    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config = format!("url = \"{}\"\n", quote(url));
    if let Some(token) = token {
        ensure!(!token.contains(['\n', '\r']), "the token must be one line");
        config.push_str(&format!(
            "header = \"Authorization: Bearer {}\"\n",
            quote(token)
        ));
    }
    let mut child = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            "60",
            "--max-filesize",
            &MAX_BYTES.to_string(),
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run curl; install curl to use https:// sources")?;
    child
        .stdin
        .take()
        .context("no curl input")?
        .write_all(config.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("{url}: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    ensure!(output.stdout.len() <= MAX_BYTES, "{url} exceeds 64 MiB");
    Ok(output.stdout)
}

/// Where the files of a source are: a directory, or an HTTPS base URL.
enum Location {
    Directory(PathBuf),
    Web { base: String, token: Option<String> },
}

impl Location {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        match self {
            Location::Directory(root) => {
                let file = root.join(path);
                if !file.is_file() {
                    return Ok(None);
                }
                fs::read(&file).map(Some)
            }
            Location::Web { base, token } => {
                let url = format!("{}/{path}", base.trim_end_matches('/'));
                download(&url, token.as_deref()).map(Some)
            }
        }
    }
}

/// The scopes' sources, for `library install` and `library update`.
pub struct Sources {
    project_root: Option<PathBuf>,
    user_area: Option<PathBuf>,
    cache: Cache,
    offline: bool,
    /// Git checkouts by `resolved`, kept for the command.
    checkouts: RefCell<HashMap<String, tempfile::TempDir>>,
}

impl Sources {
    pub fn new(
        project_root: Option<PathBuf>,
        user_area: Option<PathBuf>,
        cache: Cache,
        offline: bool,
    ) -> Self {
        Self {
            project_root,
            user_area,
            cache,
            offline,
            checkouts: RefCell::default(),
        }
    }

    /// A `file:` path, relative to the configuration that names it.
    fn directory(&self, path: &str, layer: Layer) -> PathBuf {
        let path = match path.strip_prefix("~/") {
            Some(rest) => match env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
                Some(home) => PathBuf::from(home).join(rest),
                None => PathBuf::from(path),
            },
            None => PathBuf::from(path),
        };
        if path.is_absolute() {
            return path;
        }
        let base = match layer {
            Layer::Project => self.project_root.clone(),
            Layer::User => self.user_area.clone(),
        };
        base.unwrap_or_default().join(path)
    }

    fn token(source: &Source) -> Result<Option<String>> {
        match source {
            Source::Https {
                token_env: Some(name),
                ..
            } => match env::var(name) {
                Ok(token) if !token.is_empty() => Ok(Some(token)),
                _ => bail!("the token variable {name} of {source} is not set"),
            },
            _ => Ok(None),
        }
    }

    /// Where the files are as `resolved` recorded them, checking out a Git
    /// commit once per command.
    fn location(&self, scope: &Scope, resolved: &str) -> Result<Location> {
        match &scope.source {
            Source::File { path } => Ok(Location::Directory(self.directory(path, scope.layer))),
            Source::Https { .. } => Ok(Location::Web {
                base: resolved.to_owned(),
                token: Self::token(&scope.source)?,
            }),
            Source::Git { .. } => {
                let (url, commit) = resolved
                    .strip_prefix("git+")
                    .and_then(|r| r.rsplit_once('#'))
                    .with_context(|| format!("invalid Git source {resolved:?} in lock.json"))?;
                if !self.checkouts.borrow().contains_key(resolved) {
                    let dir = checkout(url, commit)?;
                    self.checkouts.borrow_mut().insert(resolved.to_owned(), dir);
                }
                Ok(Location::Directory(
                    self.checkouts.borrow()[resolved].path().to_owned(),
                ))
            }
        }
    }
}

impl Registry for Sources {
    fn versions(
        &self,
        scope: &Scope,
        name: &LibraryName,
    ) -> Result<(Versions, String), Diagnostic> {
        if self.offline {
            return Err(fetch_error(format!(
                "{name} is not in lock.json and --offline cannot ask {}",
                scope.source
            )));
        }
        let found = (|| -> Result<(Versions, String)> {
            let (location, resolved) = match &scope.source {
                Source::Git { url, reference } => {
                    let (dir, commit) = clone(url, reference.as_deref())?;
                    let resolved = format!("git+{url}#{commit}");
                    let root = dir.path().to_owned();
                    self.checkouts.borrow_mut().insert(resolved.clone(), dir);
                    (Location::Directory(root), resolved)
                }
                other => {
                    let resolved = other.to_string();
                    (self.location(scope, &resolved)?, resolved)
                }
            };
            let bytes = location
                .read(&format!("{}/versions.json", place(name)))?
                .with_context(|| format!("{name} is not published at {}", scope.source))?;
            Ok((Versions::parse(&bytes, name)?, resolved))
        })();
        found.map_err(|e| fetch_error(format!("{e:#}")))
    }

    fn fetch(
        &self,
        scope: &Scope,
        name: &LibraryName,
        version: &Version,
        resolved: &str,
        integrity: &str,
    ) -> Result<Vec<u8>, Diagnostic> {
        if let Some(bytes) = self.cache.get(integrity) {
            return Ok(bytes);
        }
        if self.offline {
            return Err(fetch_error(format!(
                "{name}@{version} is not in the cache; run without --offline"
            )));
        }
        let bytes = (|| -> Result<Vec<u8>> {
            let location = self.location(scope, resolved)?;
            let versions = location
                .read(&format!("{}/versions.json", place(name)))?
                .with_context(|| format!("{name} is not published at {resolved}"))?;
            let versions = Versions::parse(&versions, name)?;
            let release = versions
                .versions
                .get(version)
                .with_context(|| format!("{name}@{version} is not published at {resolved}"))?;
            ensure!(
                !release.file.contains(['/', '\\']),
                "invalid file name {:?}",
                release.file
            );
            location
                .read(&format!("{}/{}", place(name), release.file))?
                .with_context(|| format!("{name}@{version}: {} is missing", release.file))
        })()
        .map_err(|e| fetch_error(format!("{e:#}")))?;
        self.cache.put(integrity, &bytes);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
            .args(args)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// Publishes `bytes` as @acme/brand 1.0.0 in a source directory.
    fn publish(root: &Path, bytes: &[u8]) {
        let name = LibraryName::parse("@acme/brand").unwrap();
        let mut versions = Versions::new(name.clone());
        versions.versions.insert(
            "1.0.0".parse().unwrap(),
            crate::domain::registry::Release {
                file: "1.0.0.xml".into(),
                integrity: integrity(bytes),
                entries: 1,
            },
        );
        SourceDirectory::new(root.to_owned())
            .publish(&name, "1.0.0.xml", bytes, &versions)
            .unwrap();
    }

    #[test]
    fn git_sources_are_read_at_the_locked_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        stdfs::create_dir(&repo).unwrap();
        run_git(&repo, &["init", "--quiet"]);
        publish(&repo, b"first");
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "--quiet", "-m", "first"]);
        let name = LibraryName::parse("@acme/brand").unwrap();
        let url = repo.to_string_lossy().into_owned();
        let scope = Scope {
            source: Source::Git {
                url: url.clone(),
                reference: None,
            },
            layer: Layer::Project,
        };
        let sources = Sources::new(None, None, Cache::new(None), false);
        let (versions, resolved) = sources.versions(&scope, &name).unwrap();
        assert!(resolved.starts_with(&format!("git+{url}#")), "{resolved}");
        let version = versions.latest().unwrap();
        let first = integrity(b"first");
        assert_eq!(
            sources
                .fetch(&scope, &name, &version, &resolved, &first)
                .unwrap(),
            b"first"
        );
        // The source moves on; the locked commit still gives the same file.
        publish(&repo, b"second");
        run_git(&repo, &["commit", "--quiet", "-am", "second"]);
        let later = Sources::new(None, None, Cache::new(None), false);
        assert_eq!(
            later
                .fetch(&scope, &name, &version, &resolved, &first)
                .unwrap(),
            b"first"
        );
        assert_ne!(later.versions(&scope, &name).unwrap().1, resolved);
        // Offline, only the cache answers.
        let cache = dir.path().join("cache");
        let cached = Sources::new(None, None, Cache::new(Some(cache.clone())), false);
        cached
            .fetch(&scope, &name, &version, &resolved, &first)
            .unwrap();
        let offline = Sources::new(None, None, Cache::new(Some(cache)), true);
        assert_eq!(
            offline
                .fetch(&scope, &name, &version, &resolved, &first)
                .unwrap(),
            b"first"
        );
        assert_eq!(
            offline.versions(&scope, &name).unwrap_err().code,
            Code::SourceFetch
        );
    }

    #[test]
    fn https_tokens_come_from_the_environment() {
        let scope = Scope {
            source: Source::Https {
                url: "https://libraries.invalid/drawio/".into(),
                token_env: Some("D_PNG_TEST_UNSET_TOKEN".into()),
            },
            layer: Layer::Project,
        };
        let sources = Sources::new(None, None, Cache::new(None), false);
        let error = sources
            .versions(&scope, &LibraryName::parse("@acme/brand").unwrap())
            .unwrap_err();
        assert!(
            error
                .message
                .contains("token variable D_PNG_TEST_UNSET_TOKEN"),
            "{}",
            error.message
        );
    }
}
