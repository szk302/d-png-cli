//! Layer boundaries of docs/architecture.md: dependencies point from adapters
//! to app to domain, and the domain does no I/O.
use std::{
    fs,
    path::{Path, PathBuf},
};

fn sources(directory: &str) -> Vec<(PathBuf, String)> {
    fn walk(path: &Path, out: &mut Vec<(PathBuf, String)>) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).unwrap();
                out.push((path, text));
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join(directory), &mut out);
    out
}

/// Source lines, without comments, that mention one of `needles`.
fn offending(directory: &str, needles: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for (path, text) in sources(directory) {
        for (number, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if needles.iter().any(|n| code.contains(n)) {
                found.push(format!("{}:{}: {}", path.display(), number + 1, line.trim()));
            }
        }
    }
    found
}

#[test]
fn the_domain_depends_on_nothing_outside_it() {
    let found = offending(
        "src/domain",
        &["crate::app", "crate::adapters", "super::super", "clap"],
    );
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_domain_does_no_io() {
    let found = offending(
        "src/domain",
        &[
            "std::fs",
            "std::env",
            "std::process",
            "std::net",
            "std::io::stdin",
            "std::io::stdout",
            "env::var",
            "File::",
            "eprintln!",
            "println!",
        ],
    );
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_application_does_not_depend_on_adapters() {
    let found = offending(
        "src/app",
        &[
            "crate::adapters",
            "clap",
            "std::fs",
            "std::env",
            "std::process",
            "eprintln!",
            "println!",
        ],
    );
    assert!(found.is_empty(), "{}", found.join("\n"));
}
