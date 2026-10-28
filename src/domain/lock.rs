//! `lock.json` (spec 9.6): the version, source and integrity of each
//! installed library.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::domain::{reference::LibraryName, version::Version};

pub const LOCKFILE_VERSION: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked {
    pub version: Version,
    /// The source as written in `[scopes]`; for Git, with the commit after `#`.
    pub resolved: String,
    pub integrity: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lock {
    pub libraries: BTreeMap<LibraryName, Locked>,
}

impl Lock {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let value: Value = serde_json::from_slice(bytes).context("invalid lock.json")?;
        ensure!(
            value["lockfileVersion"].as_u64() == Some(LOCKFILE_VERSION),
            "lock.json has lockfileVersion {}, expected {LOCKFILE_VERSION}",
            value["lockfileVersion"]
        );
        let mut libraries = BTreeMap::new();
        if let Some(entries) = value["libraries"].as_object() {
            for (name, locked) in entries {
                let parsed = LibraryName::parse(name)
                    .map_err(|d| anyhow::anyhow!("lock.json: {}", d.message))?;
                let text = |key: &str| {
                    locked[key]
                        .as_str()
                        .map(str::to_owned)
                        .with_context(|| format!("lock.json: {name} has no {key}"))
                };
                libraries.insert(
                    parsed,
                    Locked {
                        version: text("version")?.parse().map_err(anyhow::Error::msg)?,
                        resolved: text("resolved")?,
                        integrity: text("integrity")?,
                    },
                );
            }
        }
        Ok(Self { libraries })
    }

    /// The file as `library install` writes it: two-space indentation,
    /// libraries in name order.
    pub fn to_json(&self) -> String {
        let text = |s: &str| serde_json::to_string(s).expect("string");
        let mut out =
            format!("{{\n  \"lockfileVersion\": {LOCKFILE_VERSION},\n  \"libraries\": {{");
        for (index, (name, locked)) in self.libraries.iter().enumerate() {
            out.push_str(if index == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "    {}: {{\n      \"version\": \"{}\",\n      \"resolved\": {},\n      \"integrity\": {}\n    }}",
                text(&name.to_string()),
                locked.version,
                text(&locked.resolved),
                text(&locked.integrity)
            ));
        }
        out.push_str(if self.libraries.is_empty() {
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
    fn locks_round_trip() {
        let mut lock = Lock::default();
        assert_eq!(
            lock.to_json(),
            "{\n  \"lockfileVersion\": 1,\n  \"libraries\": {}\n}\n"
        );
        lock.libraries.insert(
            LibraryName::parse("@acme/brand").unwrap(),
            Locked {
                version: "1.0.0".parse().unwrap(),
                resolved: "file:../acme-d-png-libraries".into(),
                integrity: "sha256-x".into(),
            },
        );
        assert_eq!(Lock::parse(lock.to_json().as_bytes()).unwrap(), lock);
        assert!(Lock::parse(br#"{"lockfileVersion":2,"libraries":{}}"#).is_err());
    }
}
