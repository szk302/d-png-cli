use d_png_cli::{
    adapters::fs as storage,
    domain::{codec, diagram as document, drawio_png as png_data},
};
use std::io::Cursor;

const MODEL: &str = include_str!("fixtures/model.xml");
const MIXED: &str = include_str!("fixtures/mixed-pages.xml");
const PLAIN: &[u8] = include_bytes!("fixtures/plain.png");

#[test]
fn independent_png_fixtures_extract_and_expand() {
    for data in [
        include_bytes!("fixtures/text.drawio.png").as_slice(),
        include_bytes!("fixtures/zlib.drawio.png").as_slice(),
        include_bytes!("fixtures/double.drawio.png").as_slice(),
    ] {
        let raw = png_data::extract(data).unwrap();
        assert_eq!(raw, MIXED);
        let xml = document::normalize(&raw).unwrap();
        document::validate(&xml).unwrap();
        assert!(xml.contains("compressed=\"false\""));
        assert!(xml.contains("custom=\"retain\""));
        assert!(xml.contains("<extension value=\"preserve\"/>"));
        assert_eq!(xml.matches("<mxGraphModel").count(), 2);
        assert!(xml.contains("日本語 &amp; &lt;b&gt;HTML&lt;/b&gt;"));
    }
    let legacy = png_data::extract(include_bytes!("fixtures/legacy.drawio.png")).unwrap();
    assert_eq!(legacy, MODEL.trim());
}

#[test]
fn validation_checks_every_page() {
    document::validate(MODEL).unwrap();
    document::validate(MIXED).unwrap();
    for invalid in [
        "",
        "<mxfile/>",
        "<wrong/>",
        "<mxGraphModel><root><mxCell id=\"0\"/></root></mxGraphModel>",
        "<mxfile><diagram>broken!</diagram></mxfile>",
        "<mxGraphModel><root/></mxfile>",
        "<!DOCTYPE mxfile><mxfile/>",
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?><mxfile/>",
    ] {
        assert!(document::validate(invalid).is_err(), "accepted {invalid}");
    }
    let invalid = format!(
        "<mxfile><diagram>{MODEL}</diagram><diagram name=\"broken\"><mxGraphModel><root><mxCell id=\"0\"/></root></mxGraphModel></diagram></mxfile>"
    );
    let error = format!("{:#}", document::validate(&invalid).unwrap_err());
    assert!(error.contains("page 2 (broken)"));
    assert!(error.contains("id=\"1\""));
}

#[test]
fn source_extensions_and_whitespace_survive() {
    let xml = format!(
        "<?xml version=\"1.0\"?><mxfile xmlns:c=\"urn:custom\" compressed = 'true' c:test=\"yes\"><!--keep--><diagram name='a'>{MODEL}</diagram><c:extra/></mxfile>"
    );
    let expected = xml.replace("compressed = 'true'", "compressed=\"false\"");
    assert_eq!(document::normalize(&xml).unwrap(), expected);
}

#[test]
fn metadata_update_preserves_all_other_chunk_bytes() {
    let first = png_data::embed(PLAIN, MODEL).unwrap();
    let edited = MODEL.replace("日本語", "編集済み");
    let second = png_data::embed(&first, &edited).unwrap();
    assert_eq!(png_data::extract(&second).unwrap(), edited);
    assert_eq!(strip_diagram_chunks(&first), PLAIN);
    assert_eq!(strip_diagram_chunks(&second), PLAIN);
    assert_eq!(second.windows(7).filter(|w| *w == b"mxfile\0").count(), 1);
}

fn strip_diagram_chunks(data: &[u8]) -> Vec<u8> {
    let mut result = data[..8].to_vec();
    let mut pos = 8;
    while pos < data.len() {
        let size = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize + 12;
        if &data[pos + 4..pos + 8] != b"tEXt" || !data[pos + 8..].starts_with(b"mxfile\0") {
            result.extend_from_slice(&data[pos..pos + size]);
        }
        pos += size;
    }
    result
}

#[test]
fn missing_metadata_corruption_and_truncation_are_errors() {
    assert!(png_data::extract(PLAIN).is_err());
    for length in [0, 7, 8, 20, PLAIN.len() - 1] {
        assert!(png_data::validate(&PLAIN[..length]).is_err());
    }
    let mut corrupted = PLAIN.to_vec();
    corrupted[20] ^= 1;
    assert!(
        png_data::validate(&corrupted)
            .unwrap_err()
            .to_string()
            .contains("CRC")
    );
    let mut trailing = PLAIN.to_vec();
    trailing.push(0);
    assert!(png_data::validate(&trailing).is_err());
}

#[test]
fn duplicate_metadata_must_agree() {
    let first = png_data::embed(PLAIN, MODEL).unwrap();
    let second = png_data::embed(PLAIN, &MODEL.replace("日本語", "changed")).unwrap();
    let mut start = 33;
    while !first[start + 8..].starts_with(b"mxfile\0") {
        start += 12 + u32::from_be_bytes(first[start..start + 4].try_into().unwrap()) as usize;
    }
    let end = start + 12 + u32::from_be_bytes(first[start..start + 4].try_into().unwrap()) as usize;
    let mut equal = first.clone();
    equal.splice(33..33, first[start..end].iter().copied());
    assert_eq!(png_data::extract(&equal).unwrap(), MODEL);
    let mut conflict = second;
    conflict.splice(33..33, first[start..end].iter().copied());
    assert!(
        png_data::extract(&conflict)
            .unwrap_err()
            .to_string()
            .contains("conflicting")
    );
}

#[test]
fn transparent_base_is_one_pixel_rgba() {
    let bytes = png_data::transparent().unwrap();
    png_data::validate(&bytes).unwrap();
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut pixels = [255; 4];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (1, 1));
    assert_eq!(pixels, [0; 4]);
}

#[test]
fn bounded_reads_and_strict_percent_decoding() {
    assert!(storage::read_limited(std::io::repeat(0)).is_err());
    for invalid in ["%", "%0", "%xx", "%ff"] {
        assert!(document::percent_decode(invalid).is_err());
    }
    assert_eq!(document::percent_decode("a+b%20c").unwrap(), "a+b c");
}

#[test]
fn compressed_data_requires_complete_stream_and_bounded_expansion() {
    use flate2::{
        Compression,
        write::{DeflateEncoder, ZlibEncoder},
    };
    use std::io::Write;
    let mut raw = DeflateEncoder::new(Vec::new(), Compression::fast());
    raw.write_all(b"diagram content").unwrap();
    let raw = raw.finish().unwrap();
    assert_eq!(codec::inflate(&raw, false).unwrap(), b"diagram content");
    assert!(codec::inflate(&raw[..raw.len() - 1], false).is_err());
    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(codec::inflate(&trailing, false).is_err());
    let mut zlib = ZlibEncoder::new(Vec::new(), Compression::fast());
    zlib.write_all(b"diagram content").unwrap();
    let zlib = zlib.finish().unwrap();
    assert!(codec::inflate(&zlib[..zlib.len() - 1], true).is_err());
    let mut bomb = DeflateEncoder::new(Vec::new(), Compression::fast());
    for _ in 0..1025 {
        bomb.write_all(&[0; 65536]).unwrap();
    }
    let bomb = bomb.finish().unwrap();
    assert!(
        codec::inflate(&bomb, false)
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
}

#[test]
fn bad_pixel_stream_and_metadata_encoding_are_rejected() {
    fn replace_chunk(data: &[u8], kind: &[u8; 4], replacement: &[u8]) -> Vec<u8> {
        let mut result = data[..8].to_vec();
        let mut pos = 8;
        while pos < data.len() {
            let end =
                pos + 12 + u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            if &data[pos + 4..pos + 8] == kind {
                result.extend_from_slice(&(replacement.len() as u32).to_be_bytes());
                let start = result.len();
                result.extend_from_slice(kind);
                result.extend_from_slice(replacement);
                result.extend_from_slice(&crc32fast::hash(&result[start..]).to_be_bytes());
            } else {
                result.extend_from_slice(&data[pos..end]);
            }
            pos = end;
        }
        result
    }
    let bad_image = replace_chunk(PLAIN, b"IDAT", b"not zlib");
    assert!(png_data::validate(&bad_image).is_err());
    let bad_method = replace_chunk(
        include_bytes!("fixtures/zlib.drawio.png"),
        b"zTXt",
        b"mxfile\0\x01bad",
    );
    assert!(
        png_data::extract(&bad_method)
            .unwrap_err()
            .to_string()
            .contains("compression method")
    );
    let bad_url = replace_chunk(
        include_bytes!("fixtures/text.drawio.png"),
        b"tEXt",
        b"mxfile\0%not-hex",
    );
    assert!(png_data::extract(&bad_url).is_err());
}

#[test]
fn compressed_page_comments_cdata_and_processing_instructions_survive() {
    let doc = roxmltree::Document::parse(MIXED).unwrap();
    let compressed = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("diagram"))
        .unwrap()
        .text()
        .unwrap();
    let input = format!(
        "<?xml-stylesheet encoding='UTF-16'?><mxfile><diagram><!--before--><![CDATA[{compressed}]]><!--after--></diagram></mxfile>"
    );
    let expanded = document::normalize(&input).unwrap();
    document::validate(&expanded).unwrap();
    assert!(expanded.contains("<!--before-->"));
    assert!(expanded.contains("<!--after-->"));
    assert!(expanded.starts_with("<?xml-stylesheet encoding='UTF-16'?>"));
    assert!(!expanded.contains("<![CDATA["));
}

#[test]
fn compressed_page_split_across_text_and_cdata_expands_fully() {
    let doc = roxmltree::Document::parse(MIXED).unwrap();
    let page = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("diagram"))
        .unwrap();
    let compressed = page.text().unwrap();
    let expected =
        document::normalize(&format!("<mxfile><diagram>{compressed}</diagram></mxfile>")).unwrap();
    let (head, tail) = compressed.split_at(compressed.len() / 2);
    for content in [
        format!("\n  <![CDATA[{compressed}]]>\n"),
        format!("<![CDATA[{head}]]><![CDATA[{tail}]]>"),
        format!("{head}<![CDATA[{tail}]]>"),
        format!("<![CDATA[{head}]]>{tail}"),
        format!(" <!--a--> <![CDATA[{head}]]>{tail} <?pi x?> "),
    ] {
        let input = format!("<mxfile><diagram>{content}</diagram></mxfile>");
        let expanded = document::normalize(&input).unwrap();
        document::validate(&input).unwrap();
        document::validate(&expanded).unwrap();
        assert!(!expanded.contains("CDATA"), "{expanded}");
        let without_extras = expanded.replace("<!--a-->", "").replace("<?pi x?>", "");
        assert_eq!(without_extras, expected, "{content}");
        if content.contains("<!--a-->") {
            assert!(expanded.contains("<!--a-->") && expanded.contains("<?pi x?>"));
        }
        // The extract -> embed path must accept the expanded document again.
        let png = png_data::embed(PLAIN, &input).unwrap();
        let extracted = document::normalize(&png_data::extract(&png).unwrap()).unwrap();
        document::validate(&extracted).unwrap();
        assert_eq!(extracted, expanded);
    }
}
