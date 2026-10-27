//! The library use cases that read libraries (spec 9.7-9.10): list, search,
//! show and preview.
use std::collections::BTreeMap;

use crate::{
    app::{
        libraries::{Hit, Libraries, SearchQuery},
        ports::{LibraryInfo, Renderer},
    },
    domain::{
        codec,
        diagnostic::{Code, Diagnostic, Diagnostics},
        library::{Description, EntryKind, ImageInfo, Origin},
        lock::Lock,
        preview,
        reference::{LibraryName, Ref, fingerprint},
        version::{Range, Version},
    },
};

/// One library of `library list` (spec 9.7).
#[derive(Clone, Debug)]
pub struct Listed {
    pub origin: Origin,
    pub info: LibraryInfo,
    /// Its range in `[dependencies]`.
    pub range: Option<Range>,
    /// False for a dependency whose file is not installed; its `info` then
    /// has only the name and the locked version.
    pub installed: bool,
}

/// Libraries in search order, optionally of one scope, with the
/// dependencies that are not installed among the installed ones.
pub fn list(
    libraries: &Libraries<'_>,
    dependencies: &BTreeMap<LibraryName, Range>,
    lock: &Lock,
    scope: Option<&str>,
) -> Result<Vec<Listed>, Diagnostic> {
    let mut listed: Vec<Listed> = libraries
        .list()?
        .into_iter()
        .map(|(origin, info)| Listed {
            range: dependencies.get(&info.name).copied(),
            origin,
            info,
            installed: true,
        })
        .collect();
    for (name, range) in dependencies {
        if listed.iter().any(|l| l.info.name == *name) {
            continue;
        }
        listed.push(Listed {
            origin: Origin::Installed,
            info: LibraryInfo {
                name: name.clone(),
                title: None,
                entries: 0,
                location: None,
                normalized: false,
                version: lock.libraries.get(name).map(|l| l.version),
            },
            range: Some(*range),
            installed: false,
        });
    }
    // Project, then installed; each in name order.
    listed.sort_by(|a, b| {
        let rank = |l: &Listed| matches!(l.origin, Origin::Installed);
        (rank(a), &a.info.name).cmp(&(rank(b), &b.info.name))
    });
    listed.retain(|l| scope.is_none_or(|s| l.info.name.scope() == Some(s)));
    Ok(listed)
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
    Library(Listed),
    Entry(EntryDetails),
}

#[derive(Clone, Debug)]
pub struct EntryDetails {
    pub reference: Ref,
    pub origin: Origin,
    /// The installed version, for an installed library's entry.
    pub version: Option<Version>,
    pub title: String,
    pub kind: EntryKind,
    pub width: f64,
    pub height: f64,
    pub description: Option<Description>,
    pub image: Option<(String, ImageInfo)>,
    /// A placeholder to copy into a diagram.
    pub example: String,
}

/// `target` is a library name or a reference string. `dependencies` and
/// `lock` describe a library as `library list` does.
pub fn show(
    libraries: &Libraries<'_>,
    dependencies: &BTreeMap<LibraryName, Range>,
    lock: &Lock,
    target: &str,
) -> Result<Shown, Diagnostics> {
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
        let mut description = entry.describe().ok();
        if kind == EntryKind::Image
            && let Some(style) = description.as_mut().and_then(|d| d.style.as_mut())
        {
            *style = without_image_data(style, &reference);
        }
        let version = libraries
            .list()?
            .into_iter()
            .find(|(_, info)| info.name == reference.library)
            .and_then(|(_, info)| info.version);
        return Ok(Shown::Entry(EntryDetails {
            example: example(&reference, kind, entry.width, entry.height),
            origin: reference.library.origin(),
            version,
            title: entry.title.clone(),
            kind,
            width: entry.width,
            height: entry.height,
            description,
            image,
            reference,
        }));
    }
    let name = LibraryName::parse(target).map_err(|d| d.reference(target))?;
    let listed = list(libraries, dependencies, lock, None)?
        .into_iter()
        .find(|l| l.info.name == name)
        .ok_or_else(|| {
            Diagnostic::new(Code::RefNotFound, format!("no library named {name}")).reference(target)
        })?;
    Ok(Shown::Library(listed))
}

/// An image entry's style without the image data (spec 9.9): `image` names
/// the entry instead, so the style can be written into a diagram as it is.
fn without_image_data(style: &str, reference: &Ref) -> String {
    let image = format!("image=lib:{reference}");
    let mut items: Vec<String> = style
        .split(';')
        .filter(|item| !item.is_empty())
        .map(|item| match item.split_once('=') {
            Some(("image", _)) => image.clone(),
            _ => item.to_owned(),
        })
        .collect();
    // draw.io uses the last `image`; one is enough where the first was.
    let mut seen = false;
    items.retain(|item| *item != image || !std::mem::replace(&mut seen, true));
    let mut style = items.join(";");
    style.push(';');
    style
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
            "no entries match; see `d-png-cli library search`",
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
