//! What a source holds for each library (spec 9.5): `versions.json`, listing
//! each version's file and integrity.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::domain::{reference::LibraryName, version::Version};

/// `sha256-<Base64>` of bytes, as npm lock files and SRI write it.
pub fn integrity(bytes: &[u8]) -> String {
    format!("sha256-{}", STANDARD.encode(Sha256::digest(bytes)))
}

/// One version of a library at its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The file of this version, next to `versions.json`.
    pub file: String,
    pub integrity: String,
    pub entries: usize,
}

/// `@<scope>/<name>/versions.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versions {
    pub name: LibraryName,
    pub title: Option<String>,
    pub versions: BTreeMap<Version, Release>,
}

impl Versions {
    pub fn new(name: LibraryName) -> Self {
        Self {
            name,
            title: None,
            versions: BTreeMap::new(),
        }
    }

    pub fn latest(&self) -> Option<Version> {
        self.versions.keys().next_back().copied()
    }

    pub fn parse(bytes: &[u8], expected: &LibraryName) -> Result<Self> {
        let value: Value = serde_json::from_slice(bytes).context("invalid versions.json")?;
        let name = value["name"]
            .as_str()
            .context("versions.json has no name")?;
        ensure!(
            name == expected.to_string(),
            "versions.json is for {name}, not {expected}"
        );
        let mut versions = BTreeMap::new();
        for (version, release) in value["versions"]
            .as_object()
            .context("versions.json has no versions")?
        {
            let parsed: Version = version.parse().map_err(anyhow::Error::msg)?;
            let file = release["file"]
                .as_str()
                .with_context(|| format!("version {version} has no file"))?;
            ensure!(
                !file.is_empty() && !file.contains(['/', '\\']) && file != "..",
                "version {version}: the file must be next to versions.json"
            );
            versions.insert(
                parsed,
                Release {
                    file: file.to_owned(),
                    integrity: release["integrity"]
                        .as_str()
                        .filter(|i| i.starts_with("sha256-"))
                        .with_context(|| format!("version {version} has no sha256 integrity"))?
                        .to_owned(),
                    entries: release["entries"].as_u64().unwrap_or(0) as usize,
                },
            );
        }
        Ok(Self {
            name: expected.clone(),
            title: value["title"].as_str().map(str::to_owned),
            versions,
        })
    }

    /// The file as `library pack` writes it: two-space indentation, versions
    /// in order.
    pub fn to_json(&self) -> String {
        let text = |s: &str| serde_json::to_string(s).expect("string");
        let mut out = format!("{{\n  \"name\": {}", text(&self.name.to_string()));
        if let Some(title) = &self.title {
            out.push_str(&format!(",\n  \"title\": {}", text(title)));
        }
        out.push_str(",\n  \"versions\": {");
        for (index, (version, release)) in self.versions.iter().enumerate() {
            out.push_str(if index == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "    \"{version}\": {{\n      \"file\": {},\n      \"integrity\": {},\n      \"entries\": {}\n    }}",
                text(&release.file),
                text(&release.integrity),
                release.entries
            ));
        }
        out.push_str(if self.versions.is_empty() {
            "}\n}\n"
        } else {
            "\n  }\n}\n"
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_is_the_base64_sha256() {
        assert_eq!(
            integrity(b""),
            "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
        );
    }

    #[test]
    fn versions_round_trip() {
        let name = LibraryName::parse("@acme/brand").unwrap();
        let mut versions = Versions::new(name.clone());
        versions.title = Some("ACME Brand".into());
        for (v, n) in [("1.10.0", 2), ("1.2.0", 1)] {
            versions.versions.insert(
                v.parse().unwrap(),
                Release {
                    file: format!("{v}.xml"),
                    integrity: integrity(v.as_bytes()),
                    entries: n,
                },
            );
        }
        let json = versions.to_json();
        assert!(json.find("1.2.0").unwrap() < json.find("1.10.0").unwrap());
        assert_eq!(Versions::parse(json.as_bytes(), &name).unwrap(), versions);
        assert_eq!(versions.latest().unwrap().to_string(), "1.10.0");
        let other = LibraryName::parse("@acme/other").unwrap();
        assert!(Versions::parse(json.as_bytes(), &other).is_err());
        let escape = json.replace("\"1.2.0.xml\"", "\"../x.xml\"");
        assert!(Versions::parse(escape.as_bytes(), &name).is_err());
    }
}
