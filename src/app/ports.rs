//! What the use cases need from the outside world (docs/architecture.md 2).
//! Ports speak domain types and report failures as coded diagnostics; where
//! and how things are stored stays inside the adapters.
use std::collections::BTreeMap;

use crate::app::vscode::{CustomLibrary, EntryIds};
use crate::domain::{
    diagnostic::Diagnostic,
    library::{Library, Origin},
    lock::Lock,
    reference::LibraryName,
    registry::Versions,
    settings::{Scope, Settings},
    version::{Range, Version},
};

/// A library as a listing shows it, without its entries.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryInfo {
    pub name: LibraryName,
    pub title: Option<String>,
    pub entries: usize,
    /// Where the library is kept, for people (a path relative to the project).
    pub location: Option<String>,
    /// Whether the stored file has the normalized form of spec 9.3.
    pub normalized: bool,
    /// The installed version.
    pub version: Option<Version>,
}

/// Read access to the libraries of one namespace: the project's unscoped
/// libraries, the installed ones, or the built-in ones.
pub trait LibraryRepository {
    fn origin(&self) -> Origin;

    /// Every library, in name order.
    fn list(&self) -> Result<Vec<LibraryInfo>, Diagnostic>;

    /// The library with this name, or `None`.
    fn load(&self, name: &LibraryName) -> Result<Option<Library>, Diagnostic>;
}

/// A `LibraryRepository` whose libraries can also be saved and removed.
pub trait LibraryStore: LibraryRepository {
    /// Saves the library in the normalized form of spec 9.3, replacing one
    /// of the same name.
    fn save(&self, library: &Library) -> Result<(), Diagnostic>;

    /// Removes the library; false if there was none.
    fn remove(&self, name: &LibraryName) -> Result<bool, Diagnostic>;
}

/// Draws one page of a diagram.
pub trait Renderer {
    /// PNG bytes for page `page` (1-based) of an expanded, normalized diagram.
    /// The pixels only: the caller embeds the diagram.
    fn render(&self, xml: &str, page: usize, allow_network: bool) -> Result<Vec<u8>, Diagnostic>;
}

/// The sources of the scopes (spec 9.5), where `library install` and
/// `library update` get libraries. The only port that uses the network.
pub trait Registry {
    /// The library's `versions.json` as its source publishes it now, and the
    /// source as `lock.json` records it (for Git, with the commit).
    fn versions(&self, scope: &Scope, name: &LibraryName)
    -> Result<(Versions, String), Diagnostic>;

    /// The file of a version, from the source as `resolved` recorded it (a
    /// Git source at that commit), or from a cache of files by integrity.
    fn fetch(
        &self,
        scope: &Scope,
        name: &LibraryName,
        version: &Version,
        resolved: &str,
        integrity: &str,
    ) -> Result<Vec<u8>, Diagnostic>;
}

/// `lock.json`; a missing file reads as an empty lock.
pub trait LockStore {
    fn load(&self) -> Result<Lock, Diagnostic>;
    fn save(&self, lock: &Lock) -> Result<(), Diagnostic>;
}

/// The files of the installed libraries.
pub trait InstalledStore {
    /// Each installed file's library and the integrity of its bytes.
    fn installed(&self) -> Result<BTreeMap<LibraryName, String>, Diagnostic>;
    /// An installed file's bytes, or `None`.
    fn read(&self, name: &LibraryName) -> Result<Option<Vec<u8>>, Diagnostic>;
    fn install(&self, name: &LibraryName, bytes: &[u8]) -> Result<(), Diagnostic>;
    /// Removes an installed file; false if there was none.
    fn uninstall(&self, name: &LibraryName) -> Result<bool, Diagnostic>;
}

/// The settings, and the project's `[dependencies]` to change.
pub trait DependencyStore {
    /// The project's settings over the user's.
    fn settings(&self) -> Result<Settings, Diagnostic>;
    /// Sets (`Some`) or removes (`None`) dependencies in one write.
    fn change(&self, changes: &[(LibraryName, Option<Range>)]) -> Result<(), Diagnostic>;
}

/// A source directory that `library pack` writes versions into.
pub trait Publication {
    fn versions(&self, name: &LibraryName) -> Result<Option<Versions>, Diagnostic>;
    /// A file next to the library's `versions.json`.
    fn file(&self, name: &LibraryName, file: &str) -> Result<Vec<u8>, Diagnostic>;
    /// Writes a new version's file, then `versions.json`.
    fn publish(
        &self,
        name: &LibraryName,
        file: &str,
        bytes: &[u8],
        versions: &Versions,
    ) -> Result<(), Diagnostic>;
}

/// The editor's settings: VS Code's `.vscode/settings.json`.
pub trait EditorSettings {
    /// Replaces the `customLibraries` items that `ids` manages with `items`,
    /// keeping the user's items, the other settings and comments.
    fn write_custom_libraries(
        &self,
        ids: &EntryIds,
        items: &[CustomLibrary],
    ) -> Result<(), Diagnostic>;
}
