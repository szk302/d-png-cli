//! The project area (spec 2.2): the nearest `.drawio-png/` above the current
//! directory, and its unscoped libraries in `libraries/<name>.xml`.
use anyhow::{Context, Result};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs as stdfs,
    path::{Path, PathBuf},
};

use crate::{
    adapters::fs,
    app::ports::{LibraryInfo, LibraryRepository, LibraryStore},
    domain::{
        diagnostic::{Code, Diagnostic},
        library::{Library, Origin},
        library_file,
        reference::{LibraryName, is_name},
    },
};

/// The directory name of the project area.
pub const AREA: &str = ".drawio-png";

/// A directory holding a project area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    root: PathBuf,
}

impl Project {
    /// The project of `explicit` (`--project` or `DRAWIO_PNG_PROJECT`), which
    /// must hold a project area, or the nearest one above `start`.
    pub fn discover(explicit: Option<&Path>, start: &Path) -> Result<Option<Self>, Diagnostic> {
        if let Some(root) = explicit {
            return if root.join(AREA).is_dir() {
                Ok(Some(Self {
                    root: root.to_owned(),
                }))
            } else {
                Err(Diagnostic::new(
                    Code::ProjectNotFound,
                    format!(
                        "{} has no {AREA}/; run `drawio-png-cli init {}`",
                        root.display(),
                        root.display()
                    ),
                ))
            };
        }
        Ok(start
            .ancestors()
            .find(|dir| dir.join(AREA).is_dir())
            .map(|root| Self {
                root: root.to_owned(),
            }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project area, `.drawio-png/`.
    pub fn area(&self) -> PathBuf {
        self.root.join(AREA)
    }

    pub fn libraries(&self) -> ProjectLibraries {
        ProjectLibraries {
            directory: self.root.join(AREA).join("libraries"),
            cache: RefCell::default(),
        }
    }
}

/// What `init` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Initialized {
    pub area: PathBuf,
    pub created: bool,
}

const CONFIG: &str = "# drawio-png-cli project settings (docs/v0.2.0_claude/spec.md 2.4)\n\
#\n\
# [render]\n\
# renderer = \"vscode\"\n\
# default-font = \"Noto Sans CJK JP\"\n\
# fallback-fonts = [\"Noto Sans CJK JP\", \"Noto Color Emoji\"]\n\
# allow-network = false\n\
#\n\
# [scopes]\n\
# acme = \"git+https://github.com/acme/drawio-libraries.git\"\n\
#\n\
# [dependencies]\n\
# \"@acme/brand\" = \"^1.0.0\"\n";

const GITIGNORE: &str =
    "# Libraries installed with `library install`; restored from lock.json\n/libraries/@*/\n";

/// Creates a project area in `directory`, keeping one that exists.
pub fn init(directory: &Path) -> Result<Initialized, Diagnostic> {
    let area = directory.join(AREA);
    let created = !area.exists();
    let write = || -> Result<()> {
        stdfs::create_dir_all(area.join("libraries"))
            .with_context(|| format!("cannot create {}", area.display()))?;
        for (name, text) in [("config.toml", CONFIG), (".gitignore", GITIGNORE)] {
            let path = area.join(name);
            if !path.exists() {
                fs::atomic_write(&path, text.as_bytes())?;
            }
        }
        Ok(())
    };
    write().map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e))?;
    Ok(Initialized { area, created })
}

/// The project's unscoped libraries.
pub struct ProjectLibraries {
    directory: PathBuf,
    /// Each library read, and whether its file is normalized.
    cache: RefCell<HashMap<LibraryName, (Library, bool)>>,
}

impl ProjectLibraries {
    fn path(&self, name: &LibraryName) -> PathBuf {
        self.directory.join(format!("{}.xml", name.name()))
    }

    /// Library names of the `*.xml` files, in name order; other files are
    /// not libraries.
    fn names(&self) -> Result<Vec<LibraryName>, Diagnostic> {
        let entries = match stdfs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(Diagnostic::new(
                    Code::InputRead,
                    format!("cannot read {}: {e}", self.directory.display()),
                ));
            }
        };
        let mut names: Vec<LibraryName> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "xml"))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
            .filter(|stem| is_name(stem))
            .filter_map(|stem| LibraryName::new(None, &stem).ok())
            .collect();
        names.sort();
        Ok(names)
    }

    fn read(&self, name: &LibraryName) -> Result<Option<(Library, bool)>, Diagnostic> {
        if let Some(found) = self.cache.borrow().get(name) {
            return Ok(Some(found.clone()));
        }
        let path = self.path(name);
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|e| Diagnostic::from_error(Code::InputRead, &e))?;
        let library = library_file::parse(name.clone(), &bytes).map_err(|e| {
            Diagnostic::new(
                Code::LibraryInvalid,
                format!("invalid library {}: {e:#}", path.display()),
            )
        })?;
        let normalized = library_file::serialize(&library).as_bytes() == bytes.as_slice();
        let found = (library, normalized);
        self.cache.borrow_mut().insert(name.clone(), found.clone());
        Ok(Some(found))
    }
}

impl LibraryRepository for ProjectLibraries {
    fn origin(&self) -> Origin {
        Origin::Project
    }

    fn list(&self) -> Result<Vec<LibraryInfo>, Diagnostic> {
        let mut infos = Vec::new();
        for name in self.names()? {
            if let Some((library, normalized)) = self.read(&name)? {
                infos.push(LibraryInfo {
                    location: Some(format!("{AREA}/libraries/{}.xml", name.name())),
                    title: library.title.clone(),
                    entries: library.entries.len(),
                    normalized,
                    name,
                });
            }
        }
        Ok(infos)
    }

    fn load(&self, name: &LibraryName) -> Result<Option<Library>, Diagnostic> {
        if name.scope().is_some() {
            return Ok(None);
        }
        Ok(self.read(name)?.map(|(library, _)| library))
    }
}

impl LibraryStore for ProjectLibraries {
    fn save(&self, library: &Library) -> Result<(), Diagnostic> {
        if library.name.scope().is_some() {
            return Err(Diagnostic::new(
                Code::LibraryManaged,
                format!(
                    "{} is installed from its source; it is not saved here",
                    library.name
                ),
            ));
        }
        let text = library_file::serialize(library);
        let path = self.path(&library.name);
        let write = || -> anyhow::Result<()> {
            stdfs::create_dir_all(&self.directory)
                .with_context(|| format!("cannot create {}", self.directory.display()))?;
            fs::atomic_write(&path, text.as_bytes())
        };
        write().map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e))?;
        self.cache
            .borrow_mut()
            .insert(library.name.clone(), (library.clone(), true));
        Ok(())
    }

    fn remove(&self, name: &LibraryName) -> Result<bool, Diagnostic> {
        self.cache.borrow_mut().remove(name);
        match stdfs::remove_file(self.path(name)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(Diagnostic::new(
                Code::OutputWrite,
                format!("cannot remove {}: {e}", self.path(name).display()),
            )),
        }
    }
}
