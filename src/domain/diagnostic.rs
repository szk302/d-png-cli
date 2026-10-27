//! Problems reported to the user, each with a stable code (spec 2.3) so that
//! agents can choose what to do without parsing messages.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    Usage,
    InputRead,
    LimitExceeded,
    PngInvalid,
    PngNoDiagram,
    XmlSyntax,
    XmlStructure,
    IdDuplicate,
    IdDangling,
    RefSyntax,
    RefNotFound,
    RefKind,
    RefOverride,
    RefIdConflict,
    ImageChanged,
    LibraryExists,
    LibraryInvalid,
    ProjectNotFound,
    RendererNotFound,
    RenderFailed,
    RenderTimeout,
    AssetMissing,
    NetworkBlocked,
    OutputWrite,
    LibraryManaged,
    ConfigInvalid,
    ScopeUnknown,
    SourceFetch,
    VersionNotFound,
    VersionExists,
    LibraryNotInstalled,
    LibraryIntegrity,
    LockOutdated,
    /// Warning: `library pack` removed entries without a new major version.
    EntryRemoved,
    /// Warning: `write --metadata-only` kept pixels that may not match.
    MetadataOnly,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Usage => "USAGE",
            Code::InputRead => "INPUT_READ",
            Code::LimitExceeded => "LIMIT_EXCEEDED",
            Code::PngInvalid => "PNG_INVALID",
            Code::PngNoDiagram => "PNG_NO_DIAGRAM",
            Code::XmlSyntax => "XML_SYNTAX",
            Code::XmlStructure => "XML_STRUCTURE",
            Code::IdDuplicate => "ID_DUPLICATE",
            Code::IdDangling => "ID_DANGLING",
            Code::RefSyntax => "REF_SYNTAX",
            Code::RefNotFound => "REF_NOT_FOUND",
            Code::RefKind => "REF_KIND",
            Code::RefOverride => "REF_OVERRIDE",
            Code::RefIdConflict => "REF_ID_CONFLICT",
            Code::ImageChanged => "IMAGE_CHANGED",
            Code::LibraryExists => "LIBRARY_EXISTS",
            Code::LibraryInvalid => "LIBRARY_INVALID",
            Code::ProjectNotFound => "PROJECT_NOT_FOUND",
            Code::RendererNotFound => "RENDERER_NOT_FOUND",
            Code::RenderFailed => "RENDER_FAILED",
            Code::RenderTimeout => "RENDER_TIMEOUT",
            Code::AssetMissing => "ASSET_MISSING",
            Code::NetworkBlocked => "NETWORK_BLOCKED",
            Code::OutputWrite => "OUTPUT_WRITE",
            Code::LibraryManaged => "LIBRARY_MANAGED",
            Code::ConfigInvalid => "CONFIG_INVALID",
            Code::ScopeUnknown => "SCOPE_UNKNOWN",
            Code::SourceFetch => "SOURCE_FETCH",
            Code::VersionNotFound => "VERSION_NOT_FOUND",
            Code::VersionExists => "VERSION_EXISTS",
            Code::LibraryNotInstalled => "LIBRARY_NOT_INSTALLED",
            Code::LibraryIntegrity => "LIBRARY_INTEGRITY",
            Code::LockOutdated => "LOCK_OUTDATED",
            Code::EntryRemoved => "ENTRY_REMOVED",
            Code::MetadataOnly => "METADATA_ONLY",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where in a diagram a problem is: 1-based page, its name, and a cell ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub page: Option<usize>,
    pub page_name: Option<String>,
    pub cell: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Code,
    pub message: String,
    pub location: Location,
    /// The reference string involved, as written.
    pub reference: Option<String>,
    /// Close references that exist, best first (at most 5).
    pub candidates: Vec<String>,
}

impl Diagnostic {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            location: Location::default(),
            reference: None,
            candidates: Vec::new(),
        }
    }

    /// A diagnostic for an error from code that does not know the codes; the
    /// message keeps the whole chain of causes.
    pub fn from_error(code: Code, error: &anyhow::Error) -> Self {
        Self::new(code, format!("{error:#}"))
    }

    pub fn at(mut self, location: Location) -> Self {
        self.location = location;
        self
    }

    pub fn reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    pub fn candidates(mut self, candidates: Vec<String>) -> Self {
        self.candidates = candidates;
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// Every problem found in one pass, so all of them can be fixed at once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl Diagnostics {
    /// At most this many problems are reported for one input.
    pub const LIMIT: usize = 100;

    pub fn push(&mut self, diagnostic: Diagnostic) {
        if self.0.len() < Self::LIMIT {
            self.0.push(diagnostic);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `Ok(value)` when nothing was reported.
    pub fn into_result<T>(self, value: T) -> Result<T, Diagnostics> {
        if self.0.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

impl From<Diagnostic> for Diagnostics {
    fn from(diagnostic: Diagnostic) -> Self {
        Diagnostics(vec![diagnostic])
    }
}

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{diagnostic}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}
