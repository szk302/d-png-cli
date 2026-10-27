//! Reference strings (spec 4.2): `<library>/<entry>` for a project library and
//! `@<scope>/<library>/<entry>` for an installed or built-in one, plus the
//! fingerprinted image references that `read` writes.
use sha2::{Digest, Sha256};
use std::fmt;

use crate::domain::diagnostic::{Code, Diagnostic};

/// The scope of the built-in libraries.
pub const BUILTIN_SCOPE: &str = "drawio";

/// Hex digits of a fingerprint.
pub const FINGERPRINT_LEN: usize = 12;

/// `^[a-z0-9][a-z0-9-]{0,62}$`, for library and scope names.
pub fn is_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// A library name with its optional scope: `components`, `@acme/brand`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LibraryName {
    scope: Option<String>,
    name: String,
}

impl LibraryName {
    pub fn new(scope: Option<&str>, name: &str) -> Result<Self, Diagnostic> {
        let invalid = |what: &str, text: &str| {
            Diagnostic::new(
                Code::RefSyntax,
                format!(
                    "invalid {what} {text:?}: use lowercase letters, digits and '-' (at most 63)"
                ),
            )
        };
        if let Some(scope) = scope
            && !is_name(scope)
        {
            return Err(invalid("scope", scope));
        }
        if !is_name(name) {
            return Err(invalid("library name", name));
        }
        Ok(Self {
            scope: scope.map(str::to_owned),
            name: name.to_owned(),
        })
    }

    /// Parses `components` or `@acme/brand`.
    pub fn parse(text: &str) -> Result<Self, Diagnostic> {
        if let Some(rest) = text.strip_prefix('@') {
            let (scope, name) = rest.split_once('/').ok_or_else(|| {
                Diagnostic::new(
                    Code::RefSyntax,
                    format!("invalid library name {text:?}: expected @<scope>/<name>"),
                )
            })?;
            Self::new(Some(scope), name)
        } else {
            Self::new(None, text)
        }
    }

    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_builtin(&self) -> bool {
        self.scope.as_deref() == Some(BUILTIN_SCOPE)
    }
}

impl fmt::Display for LibraryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.scope {
            Some(scope) => write!(f, "@{scope}/{}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// An entry ID (spec 9.2): lowercase letters, digits, other letters and '-',
/// not starting with '-'.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(String);

impl EntryId {
    pub fn parse(text: &str) -> Result<Self, Diagnostic> {
        if Self::is_valid(text) {
            Ok(Self(text.to_owned()))
        } else {
            Err(Diagnostic::new(
                Code::RefSyntax,
                format!(
                    "invalid entry ID {text:?}: use lowercase letters, digits, other letters and '-'"
                ),
            ))
        }
    }

    pub fn is_valid(text: &str) -> bool {
        !text.is_empty()
            && !text.starts_with('-')
            && text
                .chars()
                .all(|c| c == '-' || (c.is_alphanumeric() && !c.is_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The ID derived from a title: NFKC, lowercase, runs of anything but
    /// letters and digits become '-', no leading or trailing '-', `entry` if
    /// nothing is left. Callers make it unique within a library.
    pub fn derive(title: &str) -> Self {
        use unicode_normalization::UnicodeNormalization;
        let mut id = String::new();
        let mut gap = false;
        for c in title.nfkc().flat_map(char::to_lowercase) {
            if c.is_alphanumeric() && !c.is_uppercase() {
                if gap && !id.is_empty() {
                    id.push('-');
                }
                gap = false;
                id.push(c);
            } else {
                gap = true;
            }
        }
        if id.is_empty() {
            id.push_str("entry");
        }
        Self(id)
    }

    /// This ID with a `-<n>` suffix, for the n-th entry deriving the same ID.
    pub fn numbered(&self, n: usize) -> Self {
        Self(format!("{}-{n}", self.0))
    }
}

impl fmt::Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A reference to one entry of one library.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ref {
    pub library: LibraryName,
    pub entry: EntryId,
}

impl Ref {
    /// Parses `<library>/<entry>` or `@<scope>/<library>/<entry>`.
    pub fn parse(text: &str) -> Result<Self, Diagnostic> {
        let syntax = || {
            Diagnostic::new(
                Code::RefSyntax,
                format!(
                    "invalid reference {text:?}: expected <library>/<entry> or @<scope>/<library>/<entry>"
                ),
            )
            .reference(text)
        };
        let parts: Vec<&str> = text.split('/').collect();
        let (library, entry) = match (text.starts_with('@'), parts.as_slice()) {
            (true, [scope, library, entry]) => {
                (LibraryName::new(Some(&scope[1..]), library), *entry)
            }
            (false, [library, entry]) => (LibraryName::new(None, library), *entry),
            _ => return Err(syntax()),
        };
        Ok(Self {
            library: library.map_err(|d| d.reference(text))?,
            entry: EntryId::parse(entry).map_err(|d| d.reference(text))?,
        })
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.library, self.entry)
    }
}

/// The value of `image=lib:<ref>[@<fingerprint>]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageRef {
    pub reference: Ref,
    pub fingerprint: Option<String>,
}

impl ImageRef {
    /// The prefix of an image value that is a reference.
    pub const PREFIX: &'static str = "lib:";

    /// Parses the text after `lib:`. Entry IDs never contain '@', so a
    /// fingerprint is whatever follows the last '@' after the last '/'.
    pub fn parse(text: &str) -> Result<Self, Diagnostic> {
        let last = text.rfind('/').unwrap_or(0);
        let (reference, fingerprint) = match text[last..].rfind('@') {
            Some(at) => {
                let at = last + at;
                let fingerprint = &text[at + 1..];
                if fingerprint.len() != FINGERPRINT_LEN
                    || !fingerprint
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(Diagnostic::new(
                        Code::RefSyntax,
                        format!(
                            "invalid image fingerprint {fingerprint:?}: expected {FINGERPRINT_LEN} lowercase hex digits"
                        ),
                    )
                    .reference(text));
                }
                (&text[..at], Some(fingerprint.to_owned()))
            }
            None => (text, None),
        };
        Ok(Self {
            reference: Ref::parse(reference)?,
            fingerprint,
        })
    }
}

impl fmt::Display for ImageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.fingerprint {
            Some(fingerprint) => write!(f, "{}@{fingerprint}", self.reference),
            None => write!(f, "{}", self.reference),
        }
    }
}

/// The first hex digits of the SHA-256 of an image value (its UTF-8 bytes).
pub fn fingerprint(image: &str) -> String {
    let digest = Sha256::digest(image.as_bytes());
    let mut hex = String::with_capacity(FINGERPRINT_LEN);
    for byte in digest.iter().take(FINGERPRINT_LEN / 2) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_parse_with_and_without_scope() {
        let r = Ref::parse("simple-icons/postgresql").unwrap();
        assert_eq!(r.library.to_string(), "simple-icons");
        assert_eq!(r.entry.as_str(), "postgresql");
        let r = Ref::parse("@acme/brand/logo").unwrap();
        assert_eq!(r.library.scope(), Some("acme"));
        assert_eq!(r.to_string(), "@acme/brand/logo");
        assert!(
            Ref::parse("@drawio/aws4-compute/lambda")
                .unwrap()
                .library
                .is_builtin()
        );
        for bad in [
            "",
            "postgresql",
            "a/b/c",
            "@acme/brand",
            "Simple/x",
            "lib/Entry",
            "lib/-x",
            "lib/a b",
            "@/lib/x",
        ] {
            assert_eq!(Ref::parse(bad).unwrap_err().code, Code::RefSyntax, "{bad}");
        }
        assert_eq!(
            Ref::parse("logos/会社ロゴ").unwrap().entry.as_str(),
            "会社ロゴ"
        );
    }

    #[test]
    fn image_references_carry_an_optional_fingerprint() {
        let image = ImageRef::parse("@acme/brand/logo@37483e4b7f30").unwrap();
        assert_eq!(image.reference.to_string(), "@acme/brand/logo");
        assert_eq!(image.fingerprint.as_deref(), Some("37483e4b7f30"));
        assert_eq!(image.to_string(), "@acme/brand/logo@37483e4b7f30");
        assert!(ImageRef::parse("lib/x").unwrap().fingerprint.is_none());
        assert!(ImageRef::parse("lib/x@123").is_err());
        assert!(ImageRef::parse("lib/x@37483E4B7F30").is_err());
    }

    #[test]
    fn entry_ids_derive_from_titles() {
        for (title, id) in [
            ("PostgreSQL", "postgresql"),
            ("Lambda Function", "lambda-function"),
            ("会社ロゴ", "会社ロゴ"),
            ("Group\nB", "group-b"),
            ("007", "007"),
            ("", "entry"),
            ("  --  ", "entry"),
            ("ＡＢＣ　１", "abc-1"),
            ("CD / DVD", "cd-dvd"),
        ] {
            let derived = EntryId::derive(title);
            assert_eq!(derived.as_str(), id, "{title:?}");
            assert!(EntryId::is_valid(derived.as_str()));
        }
    }

    #[test]
    fn fingerprints_are_the_sha256_prefix() {
        assert_eq!(fingerprint(""), "e3b0c44298fc");
        assert_eq!(
            fingerprint(
                "data:image/svg+xml,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNDAiIGhlaWdodD0iMTIwIj48cGF0aCBmaWxsPSIjMzM2NzkxIiBkPSJNMCAwaDI0MHYxMjBIMHoiLz48L3N2Zz4="
            ),
            "37483e4b7f30"
        );
    }
}
