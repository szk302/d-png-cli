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
