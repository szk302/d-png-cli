//! Library versions and the ranges that dependencies allow (spec 9.5, 9.6):
//! `MAJOR.MINOR.PATCH`, without pre-releases.
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl FromStr for Version {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let invalid = || format!("invalid version {text:?}: expected MAJOR.MINOR.PATCH");
        let parts: Vec<&str> = text.split('.').collect();
        let [major, minor, patch] = parts.as_slice() else {
            return Err(invalid());
        };
        let number = |part: &str| {
            let valid = !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part == "0" || !part.starts_with('0'));
            valid.then(|| part.parse::<u64>().ok()).flatten()
        };
        Ok(Self {
            major: number(major).ok_or_else(invalid)?,
            minor: number(minor).ok_or_else(invalid)?,
            patch: number(patch).ok_or_else(invalid)?,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The versions a dependency allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Range {
    /// `1.2.3`: that version only.
    Exact(Version),
    /// `^1.2.3`: from 1.2.3 below 2.0.0; for `0.y.z`, below `0.(y+1).0`.
    Caret(Version),
    /// `~1.2.3`: from 1.2.3 below 1.3.0.
    Tilde(Version),
    /// `*`: any version.
    Any,
}

impl Range {
    pub fn matches(&self, version: &Version) -> bool {
        match *self {
            Range::Exact(v) => *version == v,
            Range::Caret(v) => {
                let below = if v.major > 0 {
                    Version {
                        major: v.major + 1,
                        minor: 0,
                        patch: 0,
                    }
                } else {
                    Version {
                        major: 0,
                        minor: v.minor + 1,
                        patch: 0,
                    }
                };
                *version >= v && *version < below
            }
            Range::Tilde(v) => {
                *version >= v
                    && *version
                        < Version {
                            major: v.major,
                            minor: v.minor + 1,
                            patch: 0,
                        }
            }
            Range::Any => true,
        }
    }

    /// The newest of `versions` the range allows.
    pub fn best<'a>(&self, versions: impl IntoIterator<Item = &'a Version>) -> Option<Version> {
        versions
            .into_iter()
            .filter(|v| self.matches(v))
            .max()
            .copied()
    }
}

impl FromStr for Range {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let range = if text == "*" {
            Range::Any
        } else if let Some(rest) = text.strip_prefix('^') {
            Range::Caret(rest.parse()?)
        } else if let Some(rest) = text.strip_prefix('~') {
            Range::Tilde(rest.parse()?)
        } else {
            Range::Exact(text.parse().map_err(|_| {
                format!("invalid version range {text:?}: expected 1.2.3, ^1.2.3, ~1.2.3 or *")
            })?)
        };
        Ok(range)
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Range::Exact(v) => write!(f, "{v}"),
            Range::Caret(v) => write!(f, "^{v}"),
            Range::Tilde(v) => write!(f, "~{v}"),
            Range::Any => f.write_str("*"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        text.parse().unwrap()
    }

    #[test]
    fn versions_parse_strictly_and_order_numerically() {
        assert!(v("1.10.0") > v("1.9.9"));
        for bad in [
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.2.x",
            "1.2.3-beta",
            "",
            "v1.2.3",
        ] {
            assert!(bad.parse::<Version>().is_err(), "{bad}");
        }
    }

    #[test]
    fn ranges_allow_what_npm_allows() {
        let r = |text: &str| text.parse::<Range>().unwrap();
        assert!(r("^1.2.3").matches(&v("1.9.0")));
        assert!(!r("^1.2.3").matches(&v("2.0.0")));
        assert!(!r("^1.2.3").matches(&v("1.2.2")));
        assert!(r("^0.2.3").matches(&v("0.2.9")));
        assert!(!r("^0.2.3").matches(&v("0.3.0")));
        assert!(r("~1.2.3").matches(&v("1.2.9")));
        assert!(!r("~1.2.3").matches(&v("1.3.0")));
        assert!(r("1.2.3").matches(&v("1.2.3")) && !r("1.2.3").matches(&v("1.2.4")));
        assert!(r("*").matches(&v("9.0.0")));
        let all = [v("1.0.0"), v("1.4.0"), v("2.0.0")];
        assert_eq!(r("^1.0.0").best(&all), Some(v("1.4.0")));
        assert_eq!(r("^3.0.0").best(&all), None);
        assert_eq!(r("^1.0.0").to_string(), "^1.0.0");
        assert!(">1.0.0".parse::<Range>().is_err());
    }
}
