//! The project area (spec 2.2): the nearest `.drawio-png/` above the current
//! directory, its unscoped libraries in `libraries/<name>.xml`, the installed
//! ones in `libraries/@<scope>/<name>.xml`, and `lock.json`.
use anyhow::{Context, Result};
use std::{
    cell::{OnceCell, RefCell},
    collections::{BTreeMap, HashMap},
    fs as stdfs,
    path::{Path, PathBuf},
};

use crate::{
    adapters::fs,
    app::ports::{InstalledStore, LibraryInfo, LibraryRepository, LibraryStore, LockStore},
    domain::{
        diagnostic::{Code, Diagnostic},
        library::{Library, Origin},
        library_file,
        lock::Lock,
        reference::{LibraryName, is_name},
        registry::integrity,
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

    pub fn lock(&self) -> ProjectLock {
        ProjectLock {
            path: self.area().join("lock.json"),
        }
    }

    pub fn installed(&self) -> InstalledLibraries {
        InstalledLibraries {
            directory: self.area().join("libraries"),
            lock: self.lock(),
            locked: OnceCell::new(),
        }
    }
}

/// What `init` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Initialized {
    pub area: PathBuf,
    pub created: bool,
}

const CONFIG: &str = "# drawio-png-cli project settings (docs/spec.md 2.4)\n\
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
                    version: None,
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

/// `.drawio-png/lock.json`.
pub struct ProjectLock {
    path: PathBuf,
}

impl LockStore for ProjectLock {
    fn load(&self) -> Result<Lock, Diagnostic> {
        if !self.path.is_file() {
            return Ok(Lock::default());
        }
        let bytes =
            fs::read(&self.path).map_err(|e| Diagnostic::from_error(Code::InputRead, &e))?;
        Lock::parse(&bytes).map_err(|e| {
            Diagnostic::new(
                Code::LockOutdated,
                format!(
                    "{}: {e:#}; run `drawio-png-cli library install`",
                    self.path.display()
                ),
            )
        })
    }

    fn save(&self, lock: &Lock) -> Result<(), Diagnostic> {
        fs::atomic_write(&self.path, lock.to_json().as_bytes())
            .map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e))
    }
}

/// The installed libraries, `libraries/@<scope>/<name>.xml`, each checked
/// against `lock.json` when read (spec 9.6).
pub struct InstalledLibraries {
    directory: PathBuf,
    lock: ProjectLock,
    locked: OnceCell<Lock>,
}

impl InstalledLibraries {
    fn path(&self, name: &LibraryName) -> PathBuf {
        self.directory
            .join(format!("@{}", name.scope().unwrap_or("")))
            .join(format!("{}.xml", name.name()))
    }

    fn location(name: &LibraryName) -> String {
        format!(
            "{AREA}/libraries/@{}/{}.xml",
            name.scope().unwrap_or(""),
            name.name()
        )
    }

    fn lock(&self) -> Result<&Lock, Diagnostic> {
        if let Some(lock) = self.locked.get() {
            return Ok(lock);
        }
        let lock = self.lock.load()?;
        Ok(self.locked.get_or_init(|| lock))
    }

    /// The installed library, checked against `lock.json`: a file without a
    /// lock entry, a missing file and changed bytes are all errors.
    fn checked(&self, name: &LibraryName) -> Result<Option<(Library, Vec<u8>)>, Diagnostic> {
        let path = self.path(name);
        let locked = self.lock()?.libraries.get(name);
        let install = "; run `drawio-png-cli library install`";
        let bytes = match (path.is_file(), locked) {
            (false, None) => return Ok(None),
            (false, Some(locked)) => {
                return Err(Diagnostic::new(
                    Code::LibraryNotInstalled,
                    format!(
                        "{name}@{} is in lock.json but not installed{install}",
                        locked.version
                    ),
                ));
            }
            (true, None) => {
                return Err(Diagnostic::new(
                    Code::LibraryNotInstalled,
                    format!("{} is not in lock.json{install}", Self::location(name)),
                ));
            }
            (true, Some(_)) => {
                fs::read(&path).map_err(|e| Diagnostic::from_error(Code::InputRead, &e))?
            }
        };
        let locked = locked.expect("locked");
        if integrity(&bytes) != locked.integrity {
            return Err(Diagnostic::new(
                Code::LibraryIntegrity,
                format!(
                    "{} does not match lock.json ({name}@{}){install}",
                    Self::location(name),
                    locked.version
                ),
            ));
        }
        let library = library_file::parse(name.clone(), &bytes).map_err(|e| {
            Diagnostic::new(
                Code::LibraryInvalid,
                format!("{}: {e:#}", Self::location(name)),
            )
        })?;
        Ok(Some((library, bytes)))
    }

    /// The installed files' libraries, in name order.
    fn files(&self) -> Result<Vec<LibraryName>, Diagnostic> {
        let mut names = Vec::new();
        let Ok(scopes) = stdfs::read_dir(&self.directory) else {
            return Ok(names);
        };
        for scope in scopes.filter_map(|e| e.ok()) {
            let dir = scope.path();
            let Some(scope) = dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix('@'))
                .map(str::to_owned)
            else {
                continue;
            };
            let Ok(files) = stdfs::read_dir(&dir) else {
                continue;
            };
            for file in files.filter_map(|e| e.ok()).map(|e| e.path()) {
                if let Some(stem) = file
                    .extension()
                    .filter(|e| *e == "xml")
                    .and(file.file_stem())
                    .and_then(|s| s.to_str())
                    && let Ok(name) = LibraryName::new(Some(&scope), stem)
                {
                    names.push(name);
                }
            }
        }
        names.sort();
        Ok(names)
    }
}

impl LibraryRepository for InstalledLibraries {
    fn origin(&self) -> Origin {
        Origin::Installed
    }

    fn list(&self) -> Result<Vec<LibraryInfo>, Diagnostic> {
        let mut infos = Vec::new();
        for name in self.files()? {
            let version = self.lock()?.libraries.get(&name).map(|l| l.version);
            match self.checked(&name) {
                Ok(Some((library, bytes))) => infos.push(LibraryInfo {
                    location: Some(Self::location(&name)),
                    title: library.title.clone(),
                    entries: library.entries.len(),
                    normalized: library_file::serialize(&library).as_bytes() == bytes.as_slice(),
                    version,
                    name,
                }),
                // Listed so that it shows up; reading it reports why.
                _ => infos.push(LibraryInfo {
                    location: Some(Self::location(&name)),
                    title: None,
                    entries: 0,
                    normalized: false,
                    version,
                    name,
                }),
            }
        }
        Ok(infos)
    }

    fn load(&self, name: &LibraryName) -> Result<Option<Library>, Diagnostic> {
        if name.scope().is_none() {
            return Ok(None);
        }
        Ok(self.checked(name)?.map(|(library, _)| library))
    }
}

impl InstalledStore for InstalledLibraries {
    fn installed(&self) -> Result<BTreeMap<LibraryName, String>, Diagnostic> {
        let mut found = BTreeMap::new();
        for name in self.files()? {
            let bytes = fs::read(&self.path(&name))
                .map_err(|e| Diagnostic::from_error(Code::InputRead, &e))?;
            found.insert(name, integrity(&bytes));
        }
        Ok(found)
    }

    fn read(&self, name: &LibraryName) -> Result<Option<Vec<u8>>, Diagnostic> {
        let path = self.path(name);
        if !path.is_file() {
            return Ok(None);
        }
        fs::read(&path)
            .map(Some)
            .map_err(|e| Diagnostic::from_error(Code::InputRead, &e))
    }

    fn install(&self, name: &LibraryName, bytes: &[u8]) -> Result<(), Diagnostic> {
        let path = self.path(name);
        let write = || -> anyhow::Result<()> {
            if let Some(parent) = path.parent() {
                stdfs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            fs::atomic_write(&path, bytes)
        };
        write().map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e))
    }

    fn uninstall(&self, name: &LibraryName) -> Result<bool, Diagnostic> {
        let path = self.path(name);
        let removed = match stdfs::remove_file(&path) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return Err(Diagnostic::new(
                    Code::OutputWrite,
                    format!("cannot remove {}: {e}", path.display()),
                ));
            }
        };
        // An empty scope directory goes too.
        if let Some(parent) = path.parent() {
            let _ = stdfs::remove_dir(parent);
        }
        Ok(removed)
    }
}
