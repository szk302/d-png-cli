//! The diagram use cases (spec 5-8): `read`, `write`, `expand` and
//! `validate`. Input arrives as bytes, results leave as values; reading
//! files, printing and exit codes belong to the caller.
use crate::{
    app::{libraries::Libraries, ports::Renderer},
    domain::{
        check,
        compact::{self, Compacted, KeptImage},
        diagnostic::{Code, Diagnostic, Diagnostics},
        diagram, drawio_png,
        expand::{self, ExpandedRef},
        fonts::FontPolicy,
    },
};

/// A diagram with its references expanded and checked.
#[derive(Clone, Debug)]
pub struct Expanded {
    pub xml: String,
    pub pages: usize,
    pub cells: usize,
    pub expanded: Vec<ExpandedRef>,
}

/// Expands the references of a diagram given as XML or `.drawio.png`, checks
/// the result, and applies the font policy (spec 6).
pub fn expand(
    input: &[u8],
    fonts: Option<&FontPolicy>,
    libraries: &Libraries<'_>,
) -> Result<Expanded, Diagnostics> {
    let xml = diagram::decode(input)?;
    let references = expand::references(&xml)?;
    let resolved = libraries.resolve(&references)?;
    let expansion = expand::expand(&xml, &resolved)?;
    let summary = check::check(&expansion.xml)?;
    let xml = match fonts {
        Some(policy) => policy
            .apply(&expansion.xml)
            .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::XmlStructure, &e)))?,
        None => expansion.xml,
    };
    Ok(Expanded {
        xml,
        pages: summary.pages,
        cells: summary.cells,
        expanded: expansion.expanded,
    })
}

/// What `validate` found (spec 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validation {
    pub pages: usize,
    pub cells: usize,
    pub refs: usize,
}

pub fn validate(input: &[u8], libraries: &Libraries<'_>) -> Result<Validation, Diagnostics> {
    let expanded = expand(input, None, libraries)?;
    Ok(Validation {
        pages: expanded.pages,
        cells: expanded.cells,
        refs: expanded.expanded.len(),
    })
}

/// Where the pixels of a written PNG come from.
pub enum Pixels<'a> {
    /// Render a page.
    Render {
        renderer: &'a dyn Renderer,
        allow_network: bool,
    },
    /// Keep these PNG pixels, or a transparent 1x1 image, without rendering
    /// (`--metadata-only`).
    Keep(Option<&'a [u8]>),
}

pub struct WriteRequest<'a> {
    pub input: &'a [u8],
    /// 1-based page to render.
    pub page: usize,
    pub fonts: Option<&'a FontPolicy>,
    pub pixels: Pixels<'a>,
}

#[derive(Clone, Debug)]
pub struct Written {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub page: usize,
    pub pages: usize,
    pub expanded: Vec<ExpandedRef>,
    pub warnings: Vec<Diagnostic>,
}

/// Expands, checks, renders and embeds a diagram (spec 5).
pub fn write(request: WriteRequest<'_>, libraries: &Libraries<'_>) -> Result<Written, Diagnostics> {
    let expanded = expand(request.input, request.fonts, libraries)?;
    if request.page == 0 || request.page > expanded.pages {
        return Err(Diagnostic::new(
            Code::Usage,
            format!(
                "--page {} is out of range: the diagram has {} page(s)",
                request.page, expanded.pages
            ),
        )
        .into());
    }
    let mut warnings = Vec::new();
    let pixels = match request.pixels {
        Pixels::Render {
            renderer,
            allow_network,
        } => renderer.render(&expanded.xml, request.page, allow_network)?,
        Pixels::Keep(base) => {
            warnings.push(Diagnostic::new(
                Code::MetadataOnly,
                "the PNG pixels are not rendered from this diagram (--metadata-only)",
            ));
            match base {
                Some(bytes) => {
                    drawio_png::validate(bytes).map_err(|e| {
                        Diagnostics::from(Diagnostic::from_error(Code::PngInvalid, &e))
                    })?;
                    bytes.to_vec()
                }
                None => drawio_png::transparent().map_err(|e| {
                    Diagnostics::from(Diagnostic::from_error(Code::RenderFailed, &e))
                })?,
            }
        }
    };
    let png = drawio_png::embed(&pixels, &expanded.xml)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::PngInvalid, &e)))?;
    let (width, height) = drawio_png::dimensions(&png)
        .map_err(|e| Diagnostics::from(Diagnostic::from_error(Code::PngInvalid, &e)))?;
    Ok(Written {
        png,
        width,
        height,
        page: request.page,
        pages: expanded.pages,
        expanded: expanded.expanded,
        warnings,
    })
}

/// The XML for an agent to edit (spec 7).
#[derive(Clone, Debug)]
pub struct Read {
    pub xml: String,
    pub pages: usize,
    pub compacted: Vec<Compacted>,
    pub kept: Vec<KeptImage>,
}

/// Library images in the diagram become fingerprinted references; `raw`
/// returns the stored XML unchanged instead.
pub fn read(input: &[u8], raw: bool, libraries: &Libraries<'_>) -> Result<Read, Diagnostics> {
    if raw {
        let xml = diagram::decode_raw(input)?;
        return Ok(Read {
            pages: diagram::page_count(&xml),
            xml,
            compacted: Vec::new(),
            kept: Vec::new(),
        });
    }
    let xml = diagram::decode(input)?;
    let compaction = compact::compact(&xml, &libraries.images()?)?;
    Ok(Read {
        pages: diagram::page_count(&compaction.xml),
        xml: compaction.xml,
        compacted: compaction.compacted,
        kept: compaction.kept,
    })
}
