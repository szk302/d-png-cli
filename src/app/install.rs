//! The use cases for scoped libraries (spec 9.12-9.15): install, update and
//! uninstall from the scopes' sources, and pack a version for a source.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    app::{
        ports::{DependencyStore, InstalledStore, LockStore, Publication, Registry},
        register::Written,
    },
    domain::{
        diagnostic::{Code, Diagnostic, Diagnostics},
        library::{Entry, Library},
        library_file::{self, Source},
        lock::Locked,
        reference::{EntryId, LibraryName, Ref},
        registry::{Release, Versions, integrity},
        settings::{Scope, Settings},
        version::{Range, Version},
    },
};

/// The adapters `library install`, `update` and `remove` work with.
pub struct Stores<'a> {
    pub dependencies: &'a dyn DependencyStore,
    pub lock: &'a dyn LockStore,
    pub installed: &'a dyn InstalledStore,
    pub registry: &'a dyn Registry,
}

/// What happened to one library.
#[derive(Clone, Debug)]
pub enum Change {
    Installed {
        name: LibraryName,
        version: Version,
        resolved: String,
        entries: Vec<Written>,
    },
    Updated {
        name: LibraryName,
        from: Version,
        to: Version,
        range: Range,
        added: Vec<EntryId>,
        removed: Vec<EntryId>,
    },
    Removed {
        name: LibraryName,
    },
    Unchanged {
        name: LibraryName,
        version: Version,
    },
}

fn usage(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::Usage, message)
}

/// A scoped name that can be installed.
fn installable(name: &LibraryName) -> Result<(), Diagnostic> {
    if name.scope().is_none() {
        return Err(usage(format!(
            "{name} has no scope: project libraries are registered with `library add`"
        )));
    }
    if name.is_builtin() {
        return Err(usage(format!("{name} is built in; it needs no installing")));
    }
    Ok(())
}

fn scope_of<'s>(settings: &'s Settings, name: &LibraryName) -> Result<&'s Scope, Diagnostic> {
    let scope = name.scope().expect("a scoped name");
    settings.scopes.get(scope).ok_or_else(|| {
        Diagnostic::new(
            Code::ScopeUnknown,
            format!(
                "the scope @{scope} of {name} has no source; add it to [scopes] in config.toml"
            ),
        )
    })
}

/// Checks a fetched file: the integrity it should have and the normalized
/// form, which the VS Code extension reads (spec 9.3).
fn checked(name: &LibraryName, bytes: &[u8], expected: &str) -> Result<Library, Diagnostic> {
    let actual = integrity(bytes);
    if actual != expected {
        return Err(Diagnostic::new(
            Code::LibraryIntegrity,
            format!("{name} does not match its integrity: expected {expected}, got {actual}"),
        ));
    }
    let library = library_file::parse(name.clone(), bytes)
        .map_err(|e| Diagnostic::new(Code::LibraryInvalid, format!("{name}: {e:#}")))?;
    if library_file::serialize(&library).as_bytes() != bytes {
        return Err(Diagnostic::new(
            Code::LibraryInvalid,
            format!(
                "{name} is not in the normalized form that the VS Code extension reads; publish it with `library pack`"
            ),
        ));
    }
    Ok(library)
}

fn written(library: &Library) -> Vec<Written> {
    library
        .entries
        .iter()
        .map(|entry| Written {
            reference: Ref {
                library: library.name.clone(),
                entry: entry.id.clone(),
            },
            kind: entry.kind(),
            title: entry.title.clone(),
            width: entry.width,
            height: entry.height,
        })
        .collect()
}

pub struct InstallRequest {
    /// Libraries to add to `[dependencies]`; without a range, the latest
    /// version is installed and recorded as `^<version>`.
    pub add: Vec<(LibraryName, Option<Range>)>,
    /// Change neither `config.toml` nor `lock.json` (`--frozen`).
    pub frozen: bool,
}

/// One library to install, decided before anything is written.
struct Plan {
    name: LibraryName,
    locked: Locked,
    /// The file to write, when the installed one differs.
    bytes: Option<Vec<u8>>,
    change: Change,
}

/// Installs the dependencies, adding the requested ones (spec 9.12).
/// Everything is fetched and checked before any file changes.
pub fn install(stores: &Stores<'_>, request: &InstallRequest) -> Result<Vec<Change>, Diagnostics> {
    for (name, _) in &request.add {
        installable(name)?;
    }
    if request.frozen && !request.add.is_empty() {
        return Err(
            usage("--frozen installs what lock.json records; add libraries without it").into(),
        );
    }
    let settings = stores.dependencies.settings()?;
    let mut lock = stores.lock.load()?;
    let present = stores.installed.installed()?;
    // `None`: the latest version, recorded as ^<version>.
    let mut wanted: BTreeMap<LibraryName, Option<Range>> = settings
        .dependencies
        .iter()
        .map(|(name, range)| (name.clone(), Some(*range)))
        .collect();
    for (name, range) in &request.add {
        wanted.insert(name.clone(), *range);
    }
    let mut problems = Diagnostics::default();
    if request.frozen {
        for (name, range) in &settings.dependencies {
            match lock.libraries.get(name) {
                Some(locked) if range.matches(&locked.version) => {}
                Some(locked) => problems.push(Diagnostic::new(
                    Code::LockOutdated,
                    format!("lock.json has {name}@{}, outside {range}", locked.version),
                )),
                None => problems.push(Diagnostic::new(
                    Code::LockOutdated,
                    format!("{name} is a dependency but not in lock.json"),
                )),
            }
        }
        for name in lock.libraries.keys() {
            if !settings.dependencies.contains_key(name) {
                problems.push(Diagnostic::new(
                    Code::LockOutdated,
                    format!("lock.json has {name}, which is not a dependency"),
                ));
            }
        }
        if !problems.is_empty() {
            problems.push(Diagnostic::new(
                Code::LockOutdated,
                "run `drawio-png-cli library install` and commit lock.json",
            ));
            return Err(problems);
        }
    }
    let mut plans = Vec::new();
    for (name, range) in &wanted {
        match plan(stores, &settings, &lock, &present, name, *range) {
            Ok(plan) => plans.push(plan),
            Err(d) => problems.push(d),
        }
    }
    if !problems.is_empty() {
        problems.push(Diagnostic::new(
            Code::SourceFetch,
            "nothing was installed or changed",
        ));
        return Err(problems);
    }
    // Everything is in hand: write the files, then lock.json and config.toml.
    let mut changes = Vec::new();
    for plan in &plans {
        if let Some(bytes) = &plan.bytes {
            stores.installed.install(&plan.name, bytes)?;
        }
    }
    let gone: BTreeSet<LibraryName> = present
        .keys()
        .chain(lock.libraries.keys())
        .filter(|name| !wanted.contains_key(*name))
        .cloned()
        .collect();
    for name in gone {
        stores.installed.uninstall(&name)?;
        lock.libraries.remove(&name);
        changes.push(Change::Removed { name });
    }
    for plan in &plans {
        lock.libraries
            .insert(plan.name.clone(), plan.locked.clone());
    }
    if !request.frozen {
        stores.lock.save(&lock)?;
        let added: Vec<_> = request
            .add
            .iter()
            .map(|(name, range)| {
                let locked = &lock.libraries[name];
                (
                    name.clone(),
                    Some(range.unwrap_or(Range::Caret(locked.version))),
                )
            })
            .collect();
        if !added.is_empty() {
            stores.dependencies.change(&added)?;
        }
    }
    changes.extend(plans.into_iter().map(|p| p.change));
    Ok(changes)
}

fn plan(
    stores: &Stores<'_>,
    settings: &Settings,
    lock: &crate::domain::lock::Lock,
    present: &BTreeMap<LibraryName, String>,
    name: &LibraryName,
    range: Option<Range>,
) -> Result<Plan, Diagnostic> {
    let scope = scope_of(settings, name)?;
    let current = lock.libraries.get(name);
    let keep = current.filter(|locked| range.is_some_and(|r| r.matches(&locked.version)));
    if let Some(locked) = keep {
        if present.get(name) == Some(&locked.integrity) {
            return Ok(Plan {
                name: name.clone(),
                locked: locked.clone(),
                bytes: None,
                change: Change::Unchanged {
                    name: name.clone(),
                    version: locked.version,
                },
            });
        }
        let bytes = stores.registry.fetch(
            scope,
            name,
            &locked.version,
            &locked.resolved,
            &locked.integrity,
        )?;
        let library = checked(name, &bytes, &locked.integrity)?;
        return Ok(Plan {
            name: name.clone(),
            locked: locked.clone(),
            bytes: Some(bytes),
            change: Change::Installed {
                name: name.clone(),
                version: locked.version,
                resolved: locked.resolved.clone(),
                entries: written(&library),
            },
        });
    }
    let (versions, resolved) = stores.registry.versions(scope, name)?;
    let version = match range {
        Some(range) => range.best(versions.versions.keys()),
        None => versions.latest(),
    }
    .ok_or_else(|| {
        Diagnostic::new(
            Code::VersionNotFound,
            format!(
                "{name} has no version{} at {}; published: {}",
                range.map_or(String::new(), |r| format!(" in {r}")),
                scope.source,
                published(&versions)
            ),
        )
    })?;
    let release = &versions.versions[&version];
    let bytes = stores
        .registry
        .fetch(scope, name, &version, &resolved, &release.integrity)?;
    let library = checked(name, &bytes, &release.integrity)?;
    let locked = Locked {
        version,
        resolved: resolved.clone(),
        integrity: release.integrity.clone(),
    };
    let change = match current {
        Some(old) if old.version != version => Change::Updated {
            name: name.clone(),
            from: old.version,
            to: version,
            range: range.unwrap_or(Range::Caret(version)),
            added: Vec::new(),
            removed: Vec::new(),
        },
        _ => Change::Installed {
            name: name.clone(),
            version,
            resolved,
            entries: written(&library),
        },
    };
    Ok(Plan {
        name: name.clone(),
        locked,
        bytes: Some(bytes),
        change,
    })
}

fn published(versions: &Versions) -> String {
    if versions.versions.is_empty() {
        return "none".into();
    }
    versions
        .versions
        .keys()
        .map(Version::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn ids(library: &Library) -> BTreeSet<EntryId> {
    library.entries.iter().map(|e| e.id.clone()).collect()
}

pub struct UpdateRequest {
    /// Libraries to update; all dependencies when empty.
    pub names: Vec<LibraryName>,
    /// Go past the range, and record `^<version>` (`--latest`).
    pub latest: bool,
    /// Report without writing (`--dry-run`).
    pub dry_run: bool,
}

/// Updates installed libraries within their ranges (spec 9.13).
pub fn update(stores: &Stores<'_>, request: &UpdateRequest) -> Result<Vec<Change>, Diagnostics> {
    let settings = stores.dependencies.settings()?;
    let mut lock = stores.lock.load()?;
    let mut problems = Diagnostics::default();
    let names: Vec<LibraryName> = if request.names.is_empty() {
        settings.dependencies.keys().cloned().collect()
    } else {
        for name in &request.names {
            if !settings.dependencies.contains_key(name) {
                problems.push(Diagnostic::new(
                    Code::RefNotFound,
                    format!("{name} is not a dependency; add it with `library install {name}`"),
                ));
            }
        }
        request.names.clone()
    };
    if !problems.is_empty() {
        return Err(problems);
    }
    struct Step {
        name: LibraryName,
        locked: Locked,
        bytes: Vec<u8>,
        range: Range,
    }
    let mut steps = Vec::new();
    let mut changes = Vec::new();
    for name in &names {
        let range = settings.dependencies[name];
        let outcome = (|| -> Result<Option<(Step, Change)>, Diagnostic> {
            let scope = scope_of(&settings, name)?;
            let (versions, resolved) = stores.registry.versions(scope, name)?;
            let target = if request.latest {
                versions.latest()
            } else {
                range.best(versions.versions.keys())
            }
            .ok_or_else(|| {
                Diagnostic::new(
                    Code::VersionNotFound,
                    format!(
                        "{name} has no version in {range}; published: {}",
                        published(&versions)
                    ),
                )
            })?;
            let current = lock.libraries.get(name);
            if current.is_some_and(|l| l.version == target) {
                return Ok(None);
            }
            let release = &versions.versions[&target];
            let bytes =
                stores
                    .registry
                    .fetch(scope, name, &target, &resolved, &release.integrity)?;
            let new = checked(name, &bytes, &release.integrity)?;
            let old = stores
                .installed
                .read(name)?
                .and_then(|b| library_file::parse(name.clone(), &b).ok());
            let (old_ids, new_ids) = (old.as_ref().map(ids).unwrap_or_default(), ids(&new));
            let range = if request.latest {
                Range::Caret(target)
            } else {
                range
            };
            let change = match current {
                Some(locked) => Change::Updated {
                    name: name.clone(),
                    from: locked.version,
                    to: target,
                    range,
                    added: new_ids.difference(&old_ids).cloned().collect(),
                    removed: old_ids.difference(&new_ids).cloned().collect(),
                },
                None => Change::Installed {
                    name: name.clone(),
                    version: target,
                    resolved: resolved.clone(),
                    entries: written(&new),
                },
            };
            Ok(Some((
                Step {
                    name: name.clone(),
                    locked: Locked {
                        version: target,
                        resolved,
                        integrity: release.integrity.clone(),
                    },
                    bytes,
                    range,
                },
                change,
            )))
        })();
        match outcome {
            Ok(Some((step, change))) => {
                steps.push(step);
                changes.push(change);
            }
            Ok(None) => changes.push(Change::Unchanged {
                name: name.clone(),
                version: lock.libraries[name].version,
            }),
            Err(d) => problems.push(d),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    if request.dry_run || steps.is_empty() {
        return Ok(changes);
    }
    for step in &steps {
        stores.installed.install(&step.name, &step.bytes)?;
        lock.libraries
            .insert(step.name.clone(), step.locked.clone());
    }
    stores.lock.save(&lock)?;
    if request.latest {
        let ranges: Vec<_> = steps
            .iter()
            .map(|s| (s.name.clone(), Some(s.range)))
            .collect();
        stores.dependencies.change(&ranges)?;
    }
    Ok(changes)
}

/// Removes an installed library from the dependencies, `lock.json` and the
/// project (spec 9.15).
pub fn uninstall(stores: &Stores<'_>, name: &LibraryName) -> Result<(), Diagnostics> {
    installable(name)?;
    let settings = stores.dependencies.settings()?;
    let mut lock = stores.lock.load()?;
    let listed = settings.dependencies.contains_key(name);
    let locked = lock.libraries.remove(name).is_some();
    let removed = stores.installed.uninstall(name)?;
    if !(listed || locked || removed) {
        return Err(Diagnostic::new(Code::RefNotFound, format!("{name} is not installed")).into());
    }
    if locked {
        stores.lock.save(&lock)?;
    }
    if listed {
        stores.dependencies.change(&[(name.clone(), None)])?;
    }
    Ok(())
}

pub struct PackRequest<'a> {
    pub sources: Vec<Source<'a>>,
    pub name: LibraryName,
    pub version: Version,
    pub title: Option<&'a str>,
    pub skip_invalid: bool,
}

#[derive(Clone, Debug)]
pub struct Packed {
    pub name: LibraryName,
    pub version: Version,
    pub file: String,
    pub entries: usize,
    pub added: Vec<EntryId>,
    pub removed: Vec<EntryId>,
    pub warnings: Vec<Diagnostic>,
}

/// Writes a new version of a library into a source directory (spec 9.14).
pub fn pack(target: &dyn Publication, request: PackRequest<'_>) -> Result<Packed, Diagnostics> {
    let name = &request.name;
    installable(name)?;
    let mut versions = target
        .versions(name)?
        .unwrap_or_else(|| Versions::new(name.clone()));
    if versions.versions.contains_key(&request.version) {
        return Err(Diagnostic::new(
            Code::VersionExists,
            format!(
                "{name}@{} is published; versions never change",
                request.version
            ),
        )
        .into());
    }
    // The newest earlier version, whose IDs the new one keeps.
    let previous = match versions
        .versions
        .range(..request.version)
        .next_back()
        .map(|(v, r)| (*v, r.clone()))
    {
        Some((version, release)) => {
            let bytes = target.file(name, &release.file)?;
            Some((
                version,
                library_file::parse(name.clone(), &bytes).map_err(|e| {
                    Diagnostic::new(Code::LibraryInvalid, format!("{name}@{version}: {e:#}"))
                })?,
            ))
        }
        None => None,
    };
    let (source_title, entries) =
        library_file::read_sources_after(&request.sources, previous.as_ref().map(|(_, l)| l))
            .map_err(|e| Diagnostic::from_error(Code::LibraryInvalid, &e))?;
    let mut problems = Diagnostics::default();
    let mut warnings = Vec::new();
    let mut kept: Vec<Entry> = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        match entry.problem() {
            None => kept.push(entry),
            Some(problem) => {
                let d = Diagnostic::new(
                    Code::LibraryInvalid,
                    format!("entry {} {:?}: {problem}", index + 1, entry.title),
                );
                if request.skip_invalid {
                    warnings.push(d);
                } else {
                    problems.push(d);
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    if kept.is_empty() {
        return Err(Diagnostic::new(Code::LibraryInvalid, "the sources have no entries").into());
    }
    let title = request
        .title
        .map(str::to_owned)
        .or_else(|| versions.title.clone())
        .or(source_title)
        .unwrap_or_else(|| name.to_string());
    let library = Library {
        name: name.clone(),
        title: Some(title.clone()),
        entries: kept,
    };
    let (old_ids, new_ids) = (
        previous.as_ref().map(|(_, l)| ids(l)).unwrap_or_default(),
        ids(&library),
    );
    let removed: Vec<EntryId> = old_ids.difference(&new_ids).cloned().collect();
    if let Some((version, _)) = &previous
        && !removed.is_empty()
        && request.version.major == version.major
    {
        warnings.push(Diagnostic::new(
            Code::EntryRemoved,
            format!(
                "{} removes entries of {version} without a new major version; references to them stop working",
                request.version
            ),
        ));
    }
    let bytes = library_file::serialize(&library);
    let file = format!("{}.xml", request.version);
    versions.title = Some(title);
    versions.versions.insert(
        request.version,
        Release {
            file: file.clone(),
            integrity: integrity(bytes.as_bytes()),
            entries: library.entries.len(),
        },
    );
    target.publish(name, &file, bytes.as_bytes(), &versions)?;
    Ok(Packed {
        name: name.clone(),
        version: request.version,
        file,
        entries: library.entries.len(),
        added: new_ids.difference(&old_ids).cloned().collect(),
        removed,
        warnings,
    })
}

/// Checks that the installed libraries are the ones `lock.json` records and
/// that every dependency is installed (`library sync --check`).
pub fn verify(stores: &Stores<'_>) -> Result<(), Diagnostics> {
    let settings = stores.dependencies.settings()?;
    let lock = stores.lock.load()?;
    let present = stores.installed.installed()?;
    let mut problems = Diagnostics::default();
    let install = "run `drawio-png-cli library install`";
    for name in settings.dependencies.keys() {
        if !lock.libraries.contains_key(name) {
            problems.push(Diagnostic::new(
                Code::LibraryNotInstalled,
                format!("{name} is a dependency but not in lock.json; {install}"),
            ));
        }
    }
    for (name, locked) in &lock.libraries {
        match present.get(name) {
            None => problems.push(Diagnostic::new(
                Code::LibraryNotInstalled,
                format!("{name}@{} is not installed; {install}", locked.version),
            )),
            Some(found) if *found != locked.integrity => problems.push(Diagnostic::new(
                Code::LibraryIntegrity,
                format!("{name} does not match lock.json; {install}"),
            )),
            Some(_) => {}
        }
    }
    for name in present.keys().filter(|n| !lock.libraries.contains_key(*n)) {
        problems.push(Diagnostic::new(
            Code::LibraryNotInstalled,
            format!("{name} is installed but not in lock.json; {install}"),
        ));
    }
    problems.into_result(())
}
