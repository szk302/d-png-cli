use anyhow::{Context, Result, ensure};
use std::{fs::File, io::Read, path::Path};

use crate::MAX_BYTES;

pub fn read_limited(reader: impl Read) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    reader.take(MAX_BYTES as u64 + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() <= MAX_BYTES,
        "input or expanded data exceeds 64 MiB limit"
    );
    Ok(data)
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    read_limited(File::open(path).with_context(|| format!("cannot open {}", path.display()))?)
        .with_context(|| format!("cannot read {}", path.display()))
}

/// Stage beside the destination so replacement never crosses filesystems.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::Builder::new()
        .prefix(".drawio-png-cli-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .with_context(|| format!("cannot stage output in {}", parent.display()))?;
    file.write_all(bytes)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        file.as_file().set_permissions(metadata.permissions())?;
    }
    file.as_file().sync_all()?;
    file.persist(path)
        .with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

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
