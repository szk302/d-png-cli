//! Every library a reference can name, across repositories: the scope of a
//! name chooses the repositories, and the project's come before the global
//! ones (spec 9.1). Resolves references for the domain's expansion and
//! searches entries.
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    app::ports::{LibraryInfo, LibraryRepository},
    domain::{
        diagnostic::{Code, Diagnostic, Diagnostics, Location},
        expand::{References, Resolved},
        library::{Entry, EntryKind, Library, Origin},
        reference::{ImageRef, LibraryName, Ref, fingerprint},
        version::Version,
    },
};

/// Most candidates a failed reference suggests.
const CANDIDATES: usize = 5;

/// Library images by fingerprint: the reference and the image data.
type ImageIndex = HashMap<String, (Ref, String)>;

pub struct Libraries<'a> {
    /// In search order: the project's unscoped and installed libraries, then
    /// the global ones.
    repositories: Vec<&'a dyn LibraryRepository>,
    cache: RefCell<HashMap<LibraryName, Option<Rc<Library>>>>,
    index: RefCell<Option<Rc<ImageIndex>>>,
}

/// One search result.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub reference: Ref,
    pub origin: Origin,
    /// Whether the library is a global one.
    pub global: bool,
    /// The installed version, for an installed library's entry.
    pub version: Option<Version>,
    pub title: String,
    pub kind: EntryKind,
    pub width: f64,
    pub height: f64,
}

impl<'a> Libraries<'a> {
    /// `repositories` in any order; at most one per origin and area.
    pub fn new(repositories: Vec<&'a dyn LibraryRepository>) -> Self {
        let order = |r: &&dyn LibraryRepository| {
            (
                r.global(),
                match r.origin() {
                    Origin::Project => 0,
                    Origin::Installed => 1,
                },
            )
        };
        let mut repositories = repositories;
        repositories.sort_by_key(order);
        Self {
            repositories,
            cache: RefCell::default(),
            index: RefCell::default(),
        }
    }

    /// The repositories of a namespace: the project's, then the global one.
    fn repositories(&self, origin: Origin) -> impl Iterator<Item = &'a dyn LibraryRepository> {
        self.repositories
            .iter()
            .copied()
            .filter(move |r| r.origin() == origin)
    }

    fn has_project(&self) -> bool {
        self.repositories.iter().any(|r| !r.global())
    }

    /// The library with this name, or `None`; a missing area reads as an
    /// empty one. The project's library hides a global one of the same name;
    /// a project library that cannot be read is an error, not a reason to
    /// use the global one.
    pub fn load(&self, name: &LibraryName) -> Result<Option<Rc<Library>>, Diagnostic> {
        if let Some(found) = self.cache.borrow().get(name) {
            return Ok(found.clone());
        }
        let mut found = None;
        for repository in self.repositories(name.origin()) {
            if let Some(library) = repository.load(name)? {
                found = Some(Rc::new(library));
                break;
            }
        }
        self.cache.borrow_mut().insert(name.clone(), found.clone());
        Ok(found)
    }

    pub fn entry(&self, reference: &Ref) -> Result<Option<Entry>, Diagnostic> {
        Ok(self
            .load(&reference.library)?
            .and_then(|library| library.entry(&reference.entry).cloned()))
    }

    /// Every library a reference can name, in search order, with where it
    /// comes from; a global library hidden by the project's is left out.
    pub fn list(&self) -> Result<Vec<(Origin, LibraryInfo)>, Diagnostic> {
        let mut all: Vec<(Origin, LibraryInfo)> = Vec::new();
        for repository in &self.repositories {
            for info in repository.list()? {
                if !all.iter().any(|(_, listed)| listed.name == info.name) {
                    all.push((repository.origin(), info));
                }
            }
        }
        Ok(all)
    }

    /// Looks up everything a diagram refers to. Every missing entry and
    /// unusable image is reported.
    pub fn resolve(&self, references: &References) -> Result<Resolved, Diagnostics> {
        let mut resolved = Resolved::default();
        let mut problems = Diagnostics::default();
        let missing = |reference: &Ref, location: &Location, problems: &mut Diagnostics| {
            problems.push(
                self.not_found(reference, &reference.to_string())
                    .at(location.clone()),
            );
        };
        for found in &references.cells {
            if resolved.entries.contains_key(&found.value) {
                continue;
            }
            match self.entry(&found.value) {
                Ok(Some(entry)) => {
                    resolved.entries.insert(found.value.clone(), entry);
                }
                Ok(None) => missing(&found.value, &found.location, &mut problems),
                Err(d) => problems.push(d.at(found.location.clone())),
            }
        }
        for found in &references.images {
            let text = found.value.to_string();
            if resolved.images.contains_key(&text) {
                continue;
            }
            match self.image(&found.value) {
                Ok(data) => {
                    resolved.images.insert(text, data);
                }
                Err(d) => problems.push(d.at(found.location.clone())),
            }
        }
        problems.into_result(resolved)
    }

    /// The image data an image reference stands for (spec 4.4).
    fn image(&self, image: &ImageRef) -> Result<String, Diagnostic> {
        let text = image.to_string();
        let entry = self.entry(&image.reference)?;
        let current = match &entry {
            Some(entry) => entry
                .image()
                .map_err(|e| Diagnostic::from_error(Code::RefKind, &e).reference(&text))?,
            None => None,
        };
        let Some(wanted) = &image.fingerprint else {
            return match (entry, current) {
                (None, _) => Err(self.not_found(&image.reference, &text)),
                (Some(entry), None) => Err(Diagnostic::new(
                    Code::RefKind,
                    format!(
                        "{} is a {} entry; image=lib: needs an image",
                        image.reference,
                        entry.kind()
                    ),
                )
                .reference(text)),
                (Some(_), Some(data)) => Ok(data),
            };
        };
        if let Some(data) = current.filter(|d| &fingerprint(d) == wanted) {
            return Ok(data);
        }
        if let Some((_, data)) = self.image_index()?.get(wanted) {
            return Ok(data.clone());
        }
        if entry.is_none() {
            return Err(self.not_found(&image.reference, &text));
        }
        Err(Diagnostic::new(
            Code::ImageChanged,
            format!(
                "no library holds the image {text} had when it was read; remove @{wanted} to use the current image, or take the image from the original PNG with `expand`"
            ),
        )
        .reference(text))
    }

    /// Images of the libraries by fingerprint, the first in search order
    /// winning.
    fn image_index(&self) -> Result<Rc<ImageIndex>, Diagnostic> {
        if let Some(index) = self.index.borrow().as_ref() {
            return Ok(index.clone());
        }
        let mut index = HashMap::new();
        for (_, info) in self.list()? {
            let Some(library) = self.load(&info.name)? else {
                continue;
            };
            for entry in &library.entries {
                if let Ok(Some(data)) = entry.image() {
                    let reference = Ref {
                        library: library.name.clone(),
                        entry: entry.id.clone(),
                    };
                    index.entry(fingerprint(&data)).or_insert((reference, data));
                }
            }
        }
        let index = Rc::new(index);
        *self.index.borrow_mut() = Some(index.clone());
        Ok(index)
    }

    /// References by image fingerprint, for `read`.
    pub fn images(&self) -> Result<HashMap<String, Ref>, Diagnostic> {
        Ok(self
            .image_index()?
            .iter()
            .map(|(print, (reference, _))| (print.clone(), reference.clone()))
            .collect())
    }

    /// REF_NOT_FOUND with close references that exist.
    fn not_found(&self, reference: &Ref, text: &str) -> Diagnostic {
        let project = self.has_project();
        let message = match reference.library.origin() {
            origin if self.repositories(origin).next().is_none() => {
                format!("{reference} is not found: no project (.d-png/) here; run `d-png-cli init`")
            }
            Origin::Installed if matches!(self.load(&reference.library), Ok(None)) => format!(
                "{reference} is not found: {} is not installed; add it with `d-png-cli library install{} {}`",
                reference.library,
                if project { "" } else { " --global" },
                reference.library
            ),
            Origin::Project if !project => format!(
                "{reference} is not found: no project (.d-png/) here, and no global library {}; run `d-png-cli init`, or register it for every project with `d-png-cli library add --global`",
                reference.library
            ),
            _ => format!("{reference} is not found"),
        };
        Diagnostic::new(Code::RefNotFound, message)
            .reference(text)
            .candidates(self.candidates(reference))
    }

    /// Existing references close to one that does not exist: entries of the
    /// same library, or of libraries with a similar or the same bare name.
    pub fn candidates(&self, reference: &Ref) -> Vec<String> {
        let wanted_library = reference.library.to_string();
        let wanted_entry = reference.entry.as_str();
        let mut scored: Vec<(usize, String)> = Vec::new();
        let Ok(list) = self.list() else {
            return Vec::new();
        };
        for (_, info) in list {
            let same = info.name == reference.library;
            let library_distance = if same {
                0
            } else if info.name.name() == reference.library.name() {
                1
            } else {
                let d = distance(&info.name.to_string(), &wanted_library);
                if d > 2 {
                    continue;
                }
                d
            };
            let Ok(Some(library)) = self.load(&info.name) else {
                continue;
            };
            for entry in &library.entries {
                let id = entry.id.as_str();
                let d = distance(id, wanted_entry);
                if d <= (wanted_entry.chars().count() / 3).max(2) || shares_prefix(id, wanted_entry)
                {
                    scored.push((
                        library_distance * 10 + d,
                        format!("{}/{}", info.name, entry.id),
                    ));
                }
            }
        }
        scored.sort();
        let mut seen = HashSet::new();
        scored
            .into_iter()
            .map(|(_, r)| r)
            .filter(|r| seen.insert(r.clone()))
            .take(CANDIDATES)
            .collect()
    }

    /// Entries whose title or ID contains every word (case-insensitive):
    /// exact matches first, then prefixes, then the rest; ties in search order.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, Diagnostic> {
        let words: Vec<String> = query.words.iter().map(|w| w.to_lowercase()).collect();
        let phrase = words.join(" ");
        let mut hits: Vec<(u8, usize, Hit)> = Vec::new();
        let mut order = 0;
        for (origin, info) in self.list()? {
            if query.library.as_ref().is_some_and(|l| *l != info.name)
                || query
                    .scope
                    .as_deref()
                    .is_some_and(|s| info.name.scope() != Some(s))
            {
                continue;
            }
            let Some(library) = self.load(&info.name)? else {
                continue;
            };
            for entry in &library.entries {
                order += 1;
                let title = entry.title.to_lowercase();
                let id = entry.id.as_str();
                if !words.iter().all(|w| title.contains(w) || id.contains(w)) {
                    continue;
                }
                let kind = entry.kind();
                if query.kind.is_some_and(|k| k != kind) {
                    continue;
                }
                let rank = if words.is_empty() || title == phrase || id == phrase.replace(' ', "-")
                {
                    0
                } else if title.starts_with(&phrase) || id.starts_with(&phrase.replace(' ', "-")) {
                    1
                } else {
                    2
                };
                hits.push((
                    rank,
                    order,
                    Hit {
                        reference: Ref {
                            library: library.name.clone(),
                            entry: entry.id.clone(),
                        },
                        origin,
                        global: info.global,
                        version: info.version,
                        title: entry.title.clone(),
                        kind,
                        width: entry.width,
                        height: entry.height,
                    },
                ));
            }
        }
        hits.sort_by_key(|(rank, order, _)| (*rank, *order));
        Ok(hits.into_iter().map(|(_, _, hit)| hit).collect())
    }
}

/// What `library search` looks for.
#[derive(Clone, Debug, Default)]
pub struct SearchQuery {
    pub words: Vec<String>,
    pub library: Option<LibraryName>,
    pub scope: Option<String>,
    pub kind: Option<EntryKind>,
}

/// Edit distance between two strings, by characters.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = if ca == cb {
                previous
            } else {
                1 + previous.min(row[j]).min(current)
            };
            previous = current;
        }
    }
    row[b.len()]
}

/// Whether two IDs start with the same three characters.
fn shares_prefix(a: &str, b: &str) -> bool {
    let prefix: String = b.chars().take(3).collect();
    prefix.chars().count() == 3 && a.starts_with(&prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_count_edits() {
        assert_eq!(distance("lambda", "lamda"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert!(shares_prefix("lambda-function", "lamda"));
        assert!(!shares_prefix("lambda", "la"));
    }
}
