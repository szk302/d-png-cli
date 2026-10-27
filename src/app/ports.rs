//! What the use cases need from the outside world (docs/architecture.md 2).
//! Ports speak domain types and report failures as coded diagnostics; where
//! and how things are stored stays inside the adapters.
use crate::domain::{
    diagnostic::Diagnostic,
    library::{Library, Origin},
    reference::LibraryName,
};

/// A library as a listing shows it, without its entries.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryInfo {
    pub name: LibraryName,
    pub title: Option<String>,
    pub entries: usize,
    /// Where the library is kept, for people (a path relative to the project).
    pub location: Option<String>,
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

/// Draws one page of a diagram.
pub trait Renderer {
    /// PNG bytes for page `page` (1-based) of an expanded, normalized diagram.
    /// The pixels only: the caller embeds the diagram.
    fn render(&self, xml: &str, page: usize, allow_network: bool) -> Result<Vec<u8>, Diagnostic>;
}
