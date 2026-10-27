use anyhow::{Context, Result, bail, ensure};
use std::io::Cursor;

use crate::{
    MAX_BYTES,
    domain::{codec::inflate, diagram},
};

pub const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

struct Chunk<'a> {
    kind: &'a [u8],
    data: &'a [u8],
    raw: &'a [u8],
}

fn chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>> {
    ensure!(bytes.len() <= MAX_BYTES, "PNG exceeds 64 MiB limit");
    ensure!(bytes.starts_with(SIGNATURE), "invalid PNG signature");
    let mut chunks = Vec::new();
    let mut pos = 8usize;
    let mut ended = false;
    let mut saw_data = false;
    while pos < bytes.len() {
        ensure!(!ended, "data after PNG IEND");
        ensure!(bytes.len() - pos >= 12, "truncated PNG chunk");
        let length = u32::from_be_bytes(bytes[pos..pos + 4].try_into()?) as usize;
        ensure!(
            length <= bytes.len() - pos - 12,
            "PNG chunk exceeds file bounds"
        );
        let end = pos + 12 + length;
        let kind = &bytes[pos + 4..pos + 8];
        ensure!(
            kind.iter().all(u8::is_ascii_alphabetic) && kind[2].is_ascii_uppercase(),
            "invalid PNG chunk type"
        );
        let expected = u32::from_be_bytes(bytes[end - 4..end].try_into()?);
        ensure!(
            crc32fast::hash(&bytes[pos + 4..end - 4]) == expected,
            "PNG CRC mismatch in {}",
            String::from_utf8_lossy(kind)
        );
        if chunks.is_empty() {
            ensure!(kind == b"IHDR" && length == 13, "PNG must start with IHDR");
        } else {
            ensure!(kind != b"IHDR", "duplicate PNG IHDR");
        }
        saw_data |= kind == b"IDAT";
        if kind == b"IEND" {
            ensure!(length == 0, "invalid PNG IEND");
            ended = true;
        }
        chunks.push(Chunk {
            kind,
            data: &bytes[pos + 8..end - 4],
            raw: &bytes[pos..end],
        });
        pos = end;
    }
    ensure!(ended && saw_data, "PNG missing IDAT or IEND");
    Ok(chunks)
}

pub fn validate(bytes: &[u8]) -> Result<()> {
    let idat: Vec<u8> = chunks(bytes)?
        .iter()
        .filter(|c| c.kind == b"IDAT")
        .flat_map(|c| c.data.iter().copied())
        .collect();
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.ignore_checksums(false);
    decoder.set_limits(png::Limits { bytes: MAX_BYTES });
    decoder.set_ignore_text_chunk(true);
    let mut reader = decoder.read_info().context("invalid PNG header")?;
    let size = reader
        .output_buffer_size()
        .context("PNG image is too large")?;
    ensure!(size <= MAX_BYTES, "decoded PNG exceeds 64 MiB limit");
    reader
        .next_frame(&mut vec![0; size])
        .context("invalid PNG image data")?;
    reader.finish().context("invalid PNG stream")?;
    // The decoder stops once it has every scanline, so it accepts a zlib stream
    // whose Adler-32 trailer is missing. --no-render copies IDAT unchanged.
    zlib_complete(&idat).context("invalid PNG image data")
}

/// Check that the zlib stream reaches its end and checksum, discarding output.
fn zlib_complete(bytes: &[u8]) -> Result<()> {
    use flate2::{Decompress, FlushDecompress, Status};
    let mut decoder = Decompress::new(true);
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let (before_in, before_out) = (decoder.total_in(), decoder.total_out());
        let status = decoder
            .decompress(
                &bytes[before_in as usize..],
                &mut buffer,
                FlushDecompress::None,
            )
            .context("corrupt zlib stream")?;
        // Scanlines add one filter byte per row to the decoded size checked above.
        ensure!(
            decoder.total_out() <= 2 * MAX_BYTES as u64,
            "expanded data exceeds limit"
        );
        if status == Status::StreamEnd {
            return Ok(());
        }
        ensure!(
            decoder.total_in() != before_in || decoder.total_out() != before_out,
            "truncated zlib stream"
        );
    }
}

fn diagram_payload<'a>(chunk: &'a Chunk<'_>) -> Option<&'a [u8]> {
    if chunk.kind != b"tEXt" && chunk.kind != b"zTXt" {
        return None;
    }
    let separator = chunk.data.iter().position(|b| *b == 0)?;
    let key = &chunk.data[..separator];
    (key == b"mxfile" || key == b"mxGraphModel").then_some(&chunk.data[separator + 1..])
}

pub fn extract(bytes: &[u8]) -> Result<String> {
    validate(bytes)?;
    let mut result: Option<String> = None;
    for chunk in chunks(bytes)? {
        let Some(payload) = diagram_payload(&chunk) else {
            continue;
        };
        let compressed = chunk.kind == b"zTXt";
        let data = if compressed {
            ensure!(
                payload.first() == Some(&0),
                "unsupported zTXt compression method"
            );
            inflate(&payload[1..], true)
                .or_else(|_| inflate(&payload[1..], false))
                .context("invalid zTXt compressed diagram")?
        } else {
            payload.to_vec()
        };
        let mut xml = diagram::utf8(&data)?.to_owned();
        // Legacy Java exporters used application/x-www-form-urlencoded spaces.
        if compressed && xml.starts_with('%') {
            xml = xml.replace('+', " ");
        }
        for _ in 0..2 {
            if xml.starts_with('%') {
                xml = diagram::percent_decode(&xml)?;
            }
        }
        if let Some(previous) = &result {
            ensure!(previous == &xml, "conflicting draw.io metadata chunks");
        } else {
            result = Some(xml);
        }
    }
    result.context("PNG has no draw.io metadata")
}

fn write_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Result<()> {
    let length = u32::try_from(data.len())?;
    output.extend_from_slice(&length.to_be_bytes());
    let start = output.len();
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let crc = crc32fast::hash(&output[start..]);
    output.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

pub fn embed(bytes: &[u8], xml: &str) -> Result<Vec<u8>> {
    validate(bytes)?;
    let encoded = urlencoding::encode(xml);
    ensure!(
        encoded.len() <= MAX_BYTES,
        "encoded metadata exceeds 64 MiB limit"
    );
    let mut payload = b"mxfile\0".to_vec();
    payload.extend_from_slice(encoded.as_bytes());
    let mut output = SIGNATURE.to_vec();
    let mut inserted = false;
    for chunk in chunks(bytes)? {
        if diagram_payload(&chunk).is_some() {
            continue;
        }
        if chunk.kind == b"IDAT" && !inserted {
            write_chunk(&mut output, b"tEXt", &payload)?;
            inserted = true;
        }
        output.extend_from_slice(chunk.raw);
        ensure!(output.len() <= MAX_BYTES, "output PNG exceeds 64 MiB limit");
    }
    if !inserted {
        bail!("PNG missing IDAT");
    }
    Ok(output)
}

pub fn transparent() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&[0, 0, 0, 0])?;
    }
    Ok(bytes)
}

/// The width and height in the PNG header.
pub fn dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let header = chunks(bytes)?
        .into_iter()
        .next()
        .context("PNG has no IHDR")?;
    let number = |range: std::ops::Range<usize>| -> Result<u32> {
        Ok(u32::from_be_bytes(header.data[range].try_into()?))
    };
    Ok((number(0..4)?, number(4..8)?))
}
