//! The library use cases that read libraries (spec 9.7-9.10): list, search,
//! show and preview.
use crate::{
    app::{
        libraries::{Hit, Libraries, SearchQuery},
        ports::{LibraryInfo, Renderer},
    },
    domain::{
        codec,
        diagnostic::{Code, Diagnostic, Diagnostics},
        library::{Description, EntryKind, ImageInfo, Origin},
        preview,
        reference::{LibraryName, Ref, fingerprint},
    },
};

/// Libraries in search order, optionally of one scope.
pub fn list(
    libraries: &Libraries<'_>,
    scope: Option<&str>,
) -> Result<Vec<(Origin, LibraryInfo)>, Diagnostic> {
    Ok(libraries
        .list()?
        .into_iter()
        .filter(|(_, info)| scope.is_none_or(|s| info.name.scope() == Some(s)))
        .collect())
}

#[derive(Clone, Debug)]
pub struct Found {
    pub hits: Vec<Hit>,
    /// Hits before the limit.
    pub total: usize,
}

/// Search results, at most `limit` of them (0 for all).
pub fn search(
    libraries: &Libraries<'_>,
    query: &SearchQuery,
    limit: usize,
) -> Result<Found, Diagnostic> {
    let mut hits = libraries.search(query)?;
    let total = hits.len();
    if limit > 0 {
        hits.truncate(limit);
    }
    Ok(Found { hits, total })
}

/// What `library show` prints: a library, or one of its entries.
#[derive(Clone, Debug)]
pub enum Shown {
    Library(Origin, LibraryInfo),
    Entry(EntryDetails),
}

#[derive(Clone, Debug)]
pub struct EntryDetails {
    pub reference: Ref,
    pub origin: Origin,
    pub title: String,
    pub kind: EntryKind,
    pub width: f64,
    pub height: f64,
    pub description: Option<Description>,
    pub image: Option<(String, ImageInfo)>,
    /// A placeholder to copy into a diagram.
    pub example: String,
}

/// `target` is a library name or a reference string.
pub fn show(libraries: &Libraries<'_>, target: &str) -> Result<Shown, Diagnostics> {
    if let Ok(reference) = Ref::parse(target) {
        let entry = libraries.entry(&reference)?.ok_or_else(|| {
            Diagnostic::new(Code::RefNotFound, format!("{reference} is not found"))
                .reference(target)
                .candidates(libraries.candidates(&reference))
        })?;
        let kind = entry.kind();
        let image = entry
            .image()
            .ok()
            .flatten()
            .and_then(|data| Some((fingerprint(&data), ImageInfo::of(&data)?)));
        return Ok(Shown::Entry(EntryDetails {
            example: example(&reference, kind, entry.width, entry.height),
            origin: reference.library.origin(),
            title: entry.title.clone(),
            kind,
            width: entry.width,
            height: entry.height,
            description: entry.describe().ok(),
            image,
            reference,
        }));
    }
    let name = LibraryName::parse(target).map_err(|d| d.reference(target))?;
    let (origin, info) = libraries
        .list()?
        .into_iter()
        .find(|(_, info)| info.name == name)
        .ok_or_else(|| {
            Diagnostic::new(Code::RefNotFound, format!("no library named {name}")).reference(target)
        })?;
    Ok(Shown::Library(origin, info))
}

/// A placeholder for the kind of entry: a size for images and shapes, ends
/// for edges.
fn example(reference: &Ref, kind: EntryKind, width: f64, height: f64) -> String {
    let libref = codec::attribute(&reference.to_string());
    match kind {
        EntryKind::Edge => format!(
            r#"<mxCell id="ID" parent="1" source="SOURCE" target="TARGET" libref="{libref}"><mxGeometry relative="1" as="geometry"/></mxCell>"#
        ),
        EntryKind::Image | EntryKind::Shape => format!(
            r#"<mxCell id="ID" parent="1" libref="{libref}"><mxGeometry x="0" y="0" width="{}" height="{}" as="geometry"/></mxCell>"#,
            codec::number(width),
            codec::number(height)
        ),
        _ => format!(
            r#"<mxCell id="ID" parent="1" libref="{libref}"><mxGeometry x="0" y="0" as="geometry"/></mxCell>"#
        ),
    }
}

/// What to preview: reference strings (a library name takes all its entries)
/// and search words.
#[derive(Clone, Debug, Default)]
pub struct PreviewRequest {
    pub targets: Vec<String>,
    pub query: SearchQuery,
}

/// A PNG of the chosen entries, each labelled with its reference.
pub fn preview(
    libraries: &Libraries<'_>,
    request: &PreviewRequest,
    renderer: &dyn Renderer,
    allow_network: bool,
) -> Result<Vec<u8>, Diagnostics> {
    let mut chosen: Vec<Ref> = Vec::new();
    let mut problems = Diagnostics::default();
    let mut words = request.query.words.clone();
    for target in &request.targets {
        if !(target.contains('/') || target.starts_with('@')) {
            words.push(target.clone());
            continue;
        }
        if let Ok(reference) = Ref::parse(target) {
            chosen.push(reference);
            continue;
        }
        match LibraryName::parse(target) {
            Ok(name) => match libraries.load(&name)? {
                Some(library) => chosen.extend(library.entries.iter().map(|e| Ref {
                    library: name.clone(),
                    entry: e.id.clone(),
                })),
                None => problems.push(
                    Diagnostic::new(Code::RefNotFound, format!("no library named {name}"))
                        .reference(target.clone()),
                ),
            },
            Err(d) => problems.push(d.reference(target.clone())),
        }
    }
    let searching = !words.is_empty()
        || request.query.library.is_some()
        || request.query.scope.is_some()
        || request.query.kind.is_some();
    if searching {
        let query = SearchQuery {
            words,
            ..request.query.clone()
        };
        chosen.extend(libraries.search(&query)?.into_iter().map(|h| h.reference));
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    if chosen.is_empty() {
        return Err(Diagnostic::new(
            Code::RefNotFound,
            "no entries match; see `drawio-png-cli library search`",
        )
        .into());
    }
    if chosen.len() > preview::PREVIEW_LIMIT {
        return Err(Diagnostic::new(
            Code::Usage,
            format!(
                "{} entries match; narrow the query, --library or --kind to at most {}",
                chosen.len(),
                preview::PREVIEW_LIMIT
            ),
        )
        .into());
    }
    let mut entries = Vec::new();
    for reference in chosen {
        match libraries.entry(&reference)? {
            Some(entry) => entries.push((reference, entry)),
            None => problems.push(
                Diagnostic::new(Code::RefNotFound, format!("{reference} is not found"))
                    .reference(reference.to_string())
                    .candidates(libraries.candidates(&reference)),
            ),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let borrowed: Vec<_> = entries.iter().map(|(r, e)| (r.clone(), e)).collect();
    let xml = preview::preview(&borrowed)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::RefKind, &e)))?;
    Ok(renderer.render(&xml, 1, allow_network)?)
}
