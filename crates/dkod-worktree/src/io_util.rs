use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Write bytes to `path` atomically: write to a sibling `.tmp` file first,
/// then `rename` onto the destination. A crash during the write leaves the
/// original file untouched (there is no partial-content window the way there
/// would be with `fs::write`, which truncates in place).
///
/// Single-writer per path is the caller's responsibility — two concurrent
/// writers would race on the same tmp filename. For dkod-swarm, session
/// manifests and group specs are written by the orchestrator process only.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp_os = path.as_os_str().to_owned();
    tmp_os.push(".tmp");
    let tmp = PathBuf::from(tmp_os);
    std::fs::write(&tmp, bytes)
        .map_err(|e| Error::Io { path: tmp.clone(), source: e })?;
    std::fs::rename(&tmp, path)
        .map_err(|e| Error::Io { path: path.to_path_buf(), source: e })
}
