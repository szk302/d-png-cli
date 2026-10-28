//! Renderer child processes: discovery checks, cleanup and logs.
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::Child,
    time::Duration,
};

pub const TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) struct ChildGuard {
    pub(crate) child: Child,
    pub(crate) finished: bool,
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.finished {
            #[cfg(unix)]
            // The child starts a fresh process group; kill its helpers on timeout too.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub(crate) fn log_excerpt(log: &mut File) -> String {
    let _ = log.seek(SeekFrom::Start(0));
    let mut bytes = Vec::new();
    let _ = log.take(8192).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).trim().to_owned()
}
