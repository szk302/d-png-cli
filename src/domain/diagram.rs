use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use roxmltree::{Document, Node};
use std::ops::Range;

use crate::{MAX_BYTES, domain::codec::inflate};

pub fn utf8(bytes: &[u8]) -> Result<&str> {
    Ok(std::str::from_utf8(bytes)
        .context("XML must be UTF-8")?
        .trim_start_matches('\u{feff}'))
}

pub(crate) fn parse(xml: &str) -> Result<Document<'_>> {
    ensure!(xml.len() <= MAX_BYTES, "XML exceeds 64 MiB limit");
    let doc = Document::parse(xml).context("invalid XML (DTD is not supported)")?;
    // roxmltree parses Rust strings without checking the declared byte encoding.
    if let Some(decl) = xml
        .strip_prefix("<?xml")
        .filter(|s| s.starts_with([' ', '\t', '\r', '\n']))
        .and_then(|s| s.split_once("?>").map(|p| p.0))
        && let Some((_, rest)) = decl.split_once("encoding")
    {
        let value = rest
            .trim_start()
            .strip_prefix('=')
            .context("invalid XML encoding declaration")?
            .trim_start();
        let quote = value.chars().next().context("empty encoding")?;
        let encoding = value[1..].split(quote).next().unwrap_or("");
        ensure!(
            encoding.eq_ignore_ascii_case("utf-8"),
            "XML must declare UTF-8 encoding"
        );
    }
    Ok(doc)
}

pub fn percent_decode(text: &str) -> Result<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            ensure!(
                i + 2 < bytes.len()
                    && bytes[i + 1].is_ascii_hexdigit()
                    && bytes[i + 2].is_ascii_hexdigit(),
                "invalid percent encoding"
            );
            i += 3;
        } else {
            i += 1;
        }
    }
    Ok(urlencoding::decode(text)
        .context("percent-decoded data is not UTF-8")?
        .into_owned())
}

fn root_check(doc: &Document<'_>) -> Result<()> {
    let root = doc.root_element();
    ensure!(
        root.has_tag_name("mxfile") || root.has_tag_name("mxGraphModel"),
        "root must be <mxfile> or <mxGraphModel>"
    );
    Ok(())
}

fn page_label(page: Node<'_, '_>, index: usize) -> String {
    format!(
        "page {} ({})",
        index + 1,
        page.attribute("name")
            .or_else(|| page.attribute("id"))
            .unwrap_or("unnamed")
    )
}

/// Expand only compressed page bodies; retain all other XML source text.
pub fn normalize(xml: &str) -> Result<String> {
    let doc = parse(xml)?;
    root_check(&doc)?;
    let root = doc.root_element();
    if root.has_tag_name("mxGraphModel") {
        return Ok(xml.to_owned());
    }
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();
    let mut output_size = xml.len();
    for (index, page) in root
        .children()
        .filter(|n| n.has_tag_name("diagram"))
        .enumerate()
    {
        if page.children().any(|n| n.is_element()) {
            continue;
        }
        let label = page_label(page, index);
        let text: String = page
            .children()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect();
        let text = text.trim();
        ensure!(!text.is_empty(), "{label}: empty diagram");
        let decoded = (|| -> Result<String> {
            let bytes = STANDARD.decode(text).context("invalid page Base64")?;
            let inflated = inflate(&bytes, false).context("invalid compressed page")?;
            let model = percent_decode(utf8(&inflated)?)?;
            let inner = parse(&model)?;
            ensure!(
                inner.root_element().has_tag_name("mxGraphModel"),
                "page root must be <mxGraphModel>"
            );
            // An XML declaration cannot be inserted inside <diagram>.
            Ok(model[inner.root_element().range()].to_owned())
        })()
        .with_context(|| label.clone())?;
        // roxmltree merges adjacent text and CDATA into one node whose range covers
        // only the first section. Replace everything in the page content except
        // comments and processing instructions, whose ranges are exact.
        let start = page.first_child().context("empty diagram")?.range().start;
        let end = start
            + xml[start..page.range().end]
                .rfind("</")
                .with_context(|| format!("{label}: missing closing tag"))?;
        let mut gaps = Vec::new();
        let mut cursor = start;
        for kept in page
            .children()
            .filter(|n| n.is_comment() || n.is_pi())
            .map(|n| n.range())
            .chain(std::iter::once(end..end))
        {
            if cursor < kept.start {
                gaps.push(cursor..kept.start);
            }
            cursor = kept.end;
        }
        let removed: usize = gaps.iter().map(|r| r.len()).sum();
        output_size = output_size - removed + decoded.len();
        ensure!(
            output_size <= MAX_BYTES,
            "expanded XML exceeds 64 MiB limit"
        );
        for (index, range) in gaps.into_iter().enumerate() {
            replacements.push((
                range,
                if index == 0 {
                    decoded.clone()
                } else {
                    String::new()
                },
            ));
        }
    }
    if let Some(attr) = root
        .attributes()
        .find(|a| a.name() == "compressed" && a.namespace().is_none())
    {
        replacements.push((attr.range(), "compressed=\"false\"".into()));
    }
    replacements.sort_by_key(|(range, _)| range.start);
    let mut output = xml.to_owned();
    for (range, replacement) in replacements.into_iter().rev() {
        output.replace_range(range, &replacement);
    }
    ensure!(
        output.len() <= MAX_BYTES,
        "expanded XML exceeds 64 MiB limit"
    );
    Ok(output)
}

pub fn validate(xml: &str) -> Result<()> {
    let normalized = normalize(xml)?;
    let doc = parse(&normalized)?;
    let root = doc.root_element();
    if root.has_tag_name("mxGraphModel") {
        return validate_model(root).context("diagram");
    }
    let pages: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("diagram"))
        .collect();
    ensure!(
        !pages.is_empty(),
        "<mxfile> must contain at least one <diagram>"
    );
    for (index, page) in pages.into_iter().enumerate() {
        let models: Vec<_> = page.children().filter(|n| n.is_element()).collect();
        if models.len() != 1 || !models[0].has_tag_name("mxGraphModel") {
            bail!("{}: expected one <mxGraphModel>", page_label(page, index));
        }
        ensure!(
            !page.children().filter(|n| n.is_text()).any(|n| !n
                .text()
                .unwrap_or("")
                .trim()
                .is_empty()),
            "{}: unexpected text beside graph model",
            page_label(page, index)
        );
        validate_model(models[0]).with_context(|| page_label(page, index))?;
    }
    Ok(())
}

fn validate_model(model: Node<'_, '_>) -> Result<()> {
    let roots: Vec<_> = model
        .children()
        .filter(|n| n.has_tag_name("root"))
        .collect();
    ensure!(roots.len() == 1, "<mxGraphModel> must contain one <root>");
    for id in ["0", "1"] {
        ensure!(
            roots[0]
                .children()
                .any(|n| n.has_tag_name("mxCell") && n.attribute("id") == Some(id)),
            "missing essential <mxCell id=\"{id}\">"
        );
    }
    Ok(())
}
