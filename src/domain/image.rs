//! Image files that become library entries (spec 9.11): their media type and
//! size, read from the bytes themselves.

/// A recognized image file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageFile {
    pub media_type: &'static str,
    pub width: f64,
    pub height: f64,
}

/// The type and size of PNG, JPEG, GIF, WebP and SVG data; `None` for other
/// data or images whose size cannot be read.
pub fn inspect(bytes: &[u8]) -> Option<ImageFile> {
    let found = |media_type, width: u32, height: u32| {
        (width > 0 && height > 0).then_some(ImageFile {
            media_type,
            width: width as f64,
            height: height as f64,
        })
    };
    let be16 = |at: usize| Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as u32);
    let le16 = |at: usize| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as u32);
    let be32 = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let le24 = |at: usize| {
        let b = bytes.get(at..at + 3)?;
        Some(b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16)
    };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(12..16) == Some(b"IHDR") {
        return found("image/png", be32(16)?, be32(20)?);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return found("image/gif", le16(6)?, le16(8)?);
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        // Walk the segments to the first start-of-frame marker.
        let mut at = 2;
        while at + 4 <= bytes.len() {
            if bytes[at] != 0xFF {
                return None;
            }
            let marker = bytes[at + 1];
            if marker == 0xFF {
                at += 1;
                continue;
            }
            let length = be16(at + 2)? as usize;
            let frame = (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker);
            if frame {
                return found("image/jpeg", be16(at + 7)?, be16(at + 5)?);
            }
            at += 2 + length;
        }
        return None;
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return match bytes.get(12..16)? {
            b"VP8 " => found("image/webp", le16(26)? & 0x3FFF, le16(28)? & 0x3FFF),
            b"VP8L" => {
                let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
                found(
                    "image/webp",
                    (bits & 0x3FFF) + 1,
                    ((bits >> 14) & 0x3FFF) + 1,
                )
            }
            b"VP8X" => found("image/webp", le24(24)? + 1, le24(27)? + 1),
            _ => None,
        };
    }
    svg(bytes)
}

/// An SVG's size: its width and height, or else its viewBox.
fn svg(bytes: &[u8]) -> Option<ImageFile> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    if !text.contains("<svg") {
        return None;
    }
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return None;
    }
    let length = |key: &str| {
        let value = root.attribute(key)?.trim();
        let number = value.strip_suffix("px").unwrap_or(value);
        number
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0)
    };
    let (width, height) = match (length("width"), length("height")) {
        (Some(w), Some(h)) => (w, h),
        _ => {
            let view: Vec<f64> = root
                .attribute("viewBox")?
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|p| !p.is_empty())
                .map(|p| p.parse().ok())
                .collect::<Option<_>>()?;
            match view.as_slice() {
                [_, _, w, h] if *w > 0.0 && *h > 0.0 => (*w, *h),
                _ => return None,
            }
        }
    };
    Some(ImageFile {
        media_type: "image/svg+xml",
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_read_from_each_format() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&[0, 0, 0, 48, 0, 0, 0, 24]);
        assert_eq!(
            inspect(&png).unwrap(),
            ImageFile {
                media_type: "image/png",
                width: 48.0,
                height: 24.0
            }
        );
        let gif = b"GIF89a\x10\x00\x08\x00";
        assert_eq!(inspect(gif).unwrap().width, 16.0);
        // SOI, an APP0 segment, then SOF0 with height 30 and width 40.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 11, 8, 0, 30, 0, 40, 3,
        ];
        let image = inspect(&jpeg).unwrap();
        assert_eq!(
            (image.media_type, image.width, image.height),
            ("image/jpeg", 40.0, 30.0)
        );
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        webp.extend_from_slice(&[99, 0, 0, 49, 0, 0]);
        assert_eq!(inspect(&webp).unwrap().height, 50.0);
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120px"/>"#;
        assert_eq!(inspect(svg).unwrap().height, 120.0);
        let view = br#"<?xml version="1.0"?><svg width="100%" viewBox="0 0 32 16"></svg>"#;
        assert_eq!(inspect(view).unwrap().width, 32.0);
        assert!(inspect(b"<mxlibrary>[]</mxlibrary>").is_none());
        assert!(inspect(br#"<svg width="100%"/>"#).is_none());
    }
}
