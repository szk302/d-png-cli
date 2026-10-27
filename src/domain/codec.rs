//! Byte-level codecs draw.io uses inside XML and PNG metadata.
use anyhow::{Context, Result, ensure};

use crate::MAX_BYTES;

/// Require an actual end marker; read adapters can otherwise accept truncated DEFLATE.
pub fn inflate(bytes: &[u8], zlib: bool) -> Result<Vec<u8>> {
    use flate2::{Decompress, FlushDecompress, Status};
    let mut decoder = Decompress::new(zlib);
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(
                &bytes[before_in as usize..],
                &mut buffer,
                FlushDecompress::None,
            )
            .context("invalid DEFLATE stream")?;
        let produced = (decoder.total_out() - before_out) as usize;
        ensure!(
            output.len() + produced <= MAX_BYTES,
            "expanded data exceeds 64 MiB limit"
        );
        output.extend_from_slice(&buffer[..produced]);
        if status == Status::StreamEnd {
            ensure!(
                decoder.total_in() as usize == bytes.len(),
                "trailing data after DEFLATE stream"
            );
            return Ok(output);
        }
        ensure!(
            decoder.total_in() != before_in || produced != 0,
            "truncated DEFLATE stream"
        );
    }
}
