//! The use cases of docs/usecase.md: the XML each case passes
//! to the CLI expands to the XML it would take without the CLI.
mod support;

use drawio_png_cli::{
    app::{diagram, libraries::Libraries},
    domain::{
        library::{self, Library, Origin},
        reference::LibraryName,
    },
};
use std::{fs, path::PathBuf};
use support::{Memory, assert_same_cells};

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/examples")
}

fn load(name: &str, path: PathBuf) -> Library {
    library::parse(LibraryName::parse(name).unwrap(), &fs::read(path).unwrap()).unwrap()
}

/// The example project: three libraries of its own and @acme/brand installed
/// from examples/acme-drawio-libraries.
fn repositories() -> (Memory, Memory) {
    let project = examples().join("project/.drawio-png/libraries");
    (
        Memory {
            origin: Origin::Project,
            libraries: vec![
                load("components", project.join("components.xml")),
                load("shapes", project.join("shapes.xml")),
                load("simple-icons", project.join("simple-icons.xml")),
            ],
        },
        Memory {
            origin: Origin::Installed,
            libraries: vec![load(
                "@acme/brand",
                examples().join("acme-drawio-libraries/@acme/brand/1.0.0.xml"),
            )],
        },
    )
}

fn read(case: &str, file: &str) -> Vec<u8> {
    fs::read(examples().join(case).join(file)).unwrap()
}

#[test]
fn every_case_expands_to_the_xml_without_the_cli() {
    let (project, installed) = repositories();
    let libraries = Libraries::new(vec![&project, &installed]);
    for case in [
        "a-drawio-shape",
        "b-library-shape",
        "c-image-icon",
        "d-data-image",
        "e-multi-cell",
        "f-edge",
        "g-keep-images",
        "h-new-diagram",
    ] {
        let expanded = diagram::expand(&read(case, "cli.xml"), None, &libraries)
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        let expected = String::from_utf8(read(case, "no-cli.xml")).unwrap();
        assert_same_cells(&expanded.xml, &expected);
        // The expected XML has no references: it expands to itself.
        let again = diagram::expand(expected.as_bytes(), None, &libraries).unwrap();
        assert_same_cells(&again.xml, &expected);
        assert!(again.expanded.is_empty(), "{case}");
    }
}

#[test]
fn read_turns_library_images_into_references_and_back() {
    let (project, installed) = repositories();
    let libraries = Libraries::new(vec![&project, &installed]);
    let before = read("g-keep-images", "before.xml");
    let read_back = diagram::read(&before, false, &libraries).unwrap();
    let expected = String::from_utf8(read("g-keep-images", "read-output.xml")).unwrap();
    assert_same_cells(&read_back.xml, &expected);
    assert_eq!(read_back.compacted.len(), 1);
    assert_eq!(
        read_back.compacted[0].reference,
        "simple-icons/postgresql@224afa6bb926"
    );
    assert_eq!(read_back.compacted[0].replaced_chars, 167);
    let expanded = diagram::expand(read_back.xml.as_bytes(), None, &libraries).unwrap();
    assert_same_cells(&expanded.xml, std::str::from_utf8(&before).unwrap());

    // Installed images compact too.
    let logo = read("d-data-image", "no-cli.xml");
    let read_logo = diagram::read(&logo, false, &libraries).unwrap();
    assert_eq!(
        read_logo.compacted[0].reference,
        "@acme/brand/logo@37483e4b7f30"
    );
}

#[test]
fn the_written_png_holds_the_expanded_xml() {
    let (project, installed) = repositories();
    let libraries = Libraries::new(vec![&project, &installed]);
    let written = diagram::write(
        diagram::WriteRequest {
            input: &read("h-new-diagram", "cli.xml"),
            page: 1,
            fonts: None,
            pixels: diagram::Pixels::Keep(None),
        },
        &libraries,
    )
    .unwrap();
    let stored = drawio_png_cli::domain::drawio_png::extract(&written.png).unwrap();
    let expected = String::from_utf8(read("h-new-diagram", "no-cli.xml")).unwrap();
    assert_same_cells(&stored, &expected);
    let types: Vec<_> = written
        .expanded
        .iter()
        .map(|e| e.reference.as_str())
        .collect();
    assert_eq!(
        types,
        [
            "@acme/brand/logo",
            "shapes/actor",
            "components/service",
            "simple-icons/postgresql"
        ]
    );
}
