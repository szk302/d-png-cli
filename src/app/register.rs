//! The use cases that change the project's own libraries (spec 9.11, 9.15,
//! 9.16): add, remove and sync.
use crate::{
    app::ports::LibraryStore,
    domain::{
        diagnostic::{Code, Diagnostic, Diagnostics},
        library::{EntryKind, Library},
        library_file::{self, Source},
        reference::{LibraryName, Ref},
    },
};

/// What `library add` does when the library exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Existing {
    Fail,
    /// Replace the whole library (`--replace`).
    Replace,
    /// Replace entries with the same ID and append the others (`--merge`).
    Merge,
}

pub struct AddRequest<'a> {
    pub sources: Vec<Source<'a>>,
    pub name: Option<&'a str>,
    pub title: Option<&'a str>,
    pub existing: Existing,
    pub skip_invalid: bool,
}

/// An entry a command wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub reference: Ref,
    pub kind: EntryKind,
    pub title: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug)]
pub struct Added {
    pub name: LibraryName,
    pub location: Option<String>,
    /// `added`, `replaced` or `merged`.
    pub action: &'static str,
    /// The entries this command wrote.
    pub entries: Vec<Written>,
    /// Entries left out with `--skip-invalid`.
    pub skipped: Vec<Diagnostic>,
}

fn usage(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::Usage, message)
}

/// The library name for `library add`: the given one, or the first source's
/// file name. Scoped names belong to `library install`.
fn name_for(request: &AddRequest<'_>) -> Result<LibraryName, Diagnostic> {
    let name = match request.name {
        Some(text) => {
            LibraryName::parse(text).map_err(|d| Diagnostic::new(Code::Usage, d.message))?
        }
        None => request
            .sources
            .first()
            .and_then(|s| s.file_name)
            .and_then(library_file::name_from_file)
            .ok_or_else(|| {
                usage("give the library a name with --name (lowercase letters, digits and '-')")
            })?,
    };
    if name.scope().is_some() {
        return Err(usage(format!(
            "{name} has a scope: scoped libraries come from their source; publish with `library pack` and add them with `library install`"
        )));
    }
    Ok(name)
}

/// Registers sources as a project library (spec 9.11).
pub fn add(store: &dyn LibraryStore, request: AddRequest<'_>) -> Result<Added, Diagnostics> {
    let name = name_for(&request)?;
    let (source_title, entries) = library_file::read_sources(&request.sources)
        .map_err(|e| Diagnostic::from_error(Code::LibraryInvalid, &e))?;
    let mut problems = Diagnostics::default();
    let mut skipped = Vec::new();
    let mut kept = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        match entry.problem() {
            None => kept.push(entry),
            Some(problem) => {
                let d = Diagnostic::new(
                    Code::LibraryInvalid,
                    format!("entry {} {:?}: {problem}", index + 1, entry.title),
                );
                if request.skip_invalid {
                    skipped.push(d);
                } else {
                    problems.push(d);
                }
            }
        }
    }
    if !problems.is_empty() {
        problems.push(Diagnostic::new(
            Code::LibraryInvalid,
            "nothing was written; fix the entries or use --skip-invalid",
        ));
        return Err(problems);
    }
    if kept.is_empty() {
        return Err(Diagnostic::new(Code::LibraryInvalid, "the sources have no entries").into());
    }
    let existing = store.load(&name)?;
    let (library, action) = match (existing, request.existing) {
        (Some(_), Existing::Fail) => {
            return Err(Diagnostic::new(
                Code::LibraryExists,
                format!("library {name} exists; use --replace or --merge"),
            )
            .into());
        }
        (Some(mut library), Existing::Merge) => {
            if let Some(title) = request.title {
                library.title = Some(title.to_owned());
            }
            for entry in kept.iter().cloned() {
                match library.entries.iter_mut().find(|e| e.id == entry.id) {
                    Some(slot) => *slot = entry,
                    None => library.entries.push(entry),
                }
            }
            (library, "merged")
        }
        (found, _) => (
            Library {
                title: Some(
                    request
                        .title
                        .map(str::to_owned)
                        .or(source_title)
                        .unwrap_or_else(|| name.to_string()),
                ),
                name: name.clone(),
                entries: kept.clone(),
            },
            if found.is_some() { "replaced" } else { "added" },
        ),
    };
    store.save(&library)?;
    let location = store
        .list()?
        .into_iter()
        .find(|info| info.name == name)
        .and_then(|info| info.location);
    Ok(Added {
        entries: kept
            .iter()
            .map(|entry| Written {
                reference: Ref {
                    library: name.clone(),
                    entry: entry.id.clone(),
                },
                kind: entry.kind(),
                title: entry.title.clone(),
                width: entry.width,
                height: entry.height,
            })
            .collect(),
        name,
        location,
        action,
        skipped,
    })
}

/// What `library remove` removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Removed {
    Library(LibraryName),
    Entry(Ref),
}

/// Removes a project library or one of its entries (spec 9.15).
pub fn remove(store: &dyn LibraryStore, target: &str) -> Result<Removed, Diagnostics> {
    if let Ok(reference) = Ref::parse(target) {
        if reference.library.scope().is_some() {
            return Err(Diagnostic::new(
                Code::LibraryManaged,
                format!(
                    "{reference} belongs to an installed library; publish a new version without it at its source"
                ),
            )
            .into());
        }
        let mut library = store.load(&reference.library)?.ok_or_else(|| {
            Diagnostic::new(
                Code::RefNotFound,
                format!("no library named {}", reference.library),
            )
            .reference(target)
        })?;
        let before = library.entries.len();
        library.entries.retain(|e| e.id != reference.entry);
        if library.entries.len() == before {
            return Err(
                Diagnostic::new(Code::RefNotFound, format!("{reference} is not found"))
                    .reference(target)
                    .into(),
            );
        }
        store.save(&library)?;
        return Ok(Removed::Entry(reference));
    }
    let name = LibraryName::parse(target).map_err(|d| d.reference(target))?;
    if name.scope().is_some() {
        return Err(Diagnostic::new(
            Code::LibraryManaged,
            format!("{name} is installed; remove it from the dependencies instead"),
        )
        .into());
    }
    if store.remove(&name)? {
        Ok(Removed::Library(name))
    } else {
        Err(
            Diagnostic::new(Code::RefNotFound, format!("no library named {name}"))
                .reference(target)
                .into(),
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Synced {
    /// Libraries rewritten in the normalized form, or that need it with `check`.
    pub normalized: Vec<LibraryName>,
    pub checked: usize,
}

/// Rewrites project libraries in the normalized form, giving entries IDs;
/// with `check`, only reports the ones that need it (spec 9.16).
pub fn sync(
    store: &dyn LibraryStore,
    names: &[String],
    check: bool,
) -> Result<Synced, Diagnostics> {
    let infos = store.list()?;
    let mut problems = Diagnostics::default();
    let mut chosen = Vec::new();
    for text in names {
        match LibraryName::parse(text) {
            Ok(name) if name.scope().is_some() => problems.push(Diagnostic::new(
                Code::LibraryManaged,
                format!("{name} is installed from its source; it is never rewritten here"),
            )),
            Ok(name) => match infos.iter().find(|i| i.name == name) {
                Some(info) => chosen.push(info.clone()),
                None => problems.push(
                    Diagnostic::new(Code::RefNotFound, format!("no library named {name}"))
                        .reference(text.clone()),
                ),
            },
            Err(d) => problems.push(d.reference(text.clone())),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    if names.is_empty() {
        chosen = infos;
    }
    let mut synced = Synced {
        checked: chosen.len(),
        ..Synced::default()
    };
    for info in chosen.into_iter().filter(|i| !i.normalized) {
        if check {
            problems.push(Diagnostic::new(
                Code::LibraryInvalid,
                format!(
                    "{} is not in the normalized form that the VS Code extension reads; run `drawio-png-cli library sync`",
                    info.location.as_deref().unwrap_or(&info.name.to_string())
                ),
            ));
        } else if let Some(library) = store.load(&info.name)? {
            store.save(&library)?;
        }
        synced.normalized.push(info.name);
    }
    problems.into_result(synced)
}
