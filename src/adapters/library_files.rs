//! Library files on disk (`DRAWIO_PNG_LIBRARY_PATH`); the name of a library
//! is its file name without `.xml`.
use anyhow::{Context, Result, ensure};
use std::{ffi::OsStr, path::Path};

use crate::{
    adapters::fs,
    domain::library::{Library, parse_library},
};

/// Reads one library file.
pub fn open(path: &Path) -> Result<Library> {
    let (title, entries) = parse_library(&fs::read(path)?)
        .with_context(|| format!("invalid library {}", path.display()))?;
    let xml = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("xml"));
    let name = if xml {
        path.file_stem()
    } else {
        path.file_name()
    }
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_default();
    Ok(Library {
        name,
        title,
        entries,
    })
}

/// Reads the libraries on a search path such as `DRAWIO_PNG_LIBRARY_PATH`: files,
/// and the `*.xml` files directly in directories, in name order. Unreadable
/// files found in directories are skipped and reported as warnings.
pub fn search(path: &OsStr) -> Result<(Vec<Library>, Vec<String>)> {
    let mut libraries = Vec::new();
    let mut warnings = Vec::new();
    for item in std::env::split_paths(path) {
        if item.as_os_str().is_empty() {
            continue;
        }
        if item.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(&item)
                .with_context(|| format!("cannot read {}", item.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml"))
                })
                .collect();
            files.sort();
            for file in files {
                match open(&file) {
                    Ok(library) => libraries.push(library),
                    Err(error) => warnings.push(format!("{error:#}")),
                }
            }
        } else {
            ensure!(item.is_file(), "library not found: {}", item.display());
            libraries.push(open(&item)?);
        }
    }
    Ok((libraries, warnings))
}
