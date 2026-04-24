use crate::{Error, Result};
use dkod_worktree::{branch, Paths, SessionId, WriteLog};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Write one commit per group id on the current branch. Caller is responsible
/// for having checked out the dk-branch.
///
/// `group_ids` is processed in the order given; the resulting `git log` has
/// group `g1` older than `g2` older than `g3`, etc.
///
/// Groups whose `writes.jsonl` is empty are silently skipped.
pub fn commit_per_group(
    repo_root: &Path,
    paths: &Paths,
    session_id: &SessionId,
    group_ids: &[String],
) -> Result<()> {
    for gid in group_ids {
        let log = WriteLog::open(paths, session_id, gid).map_err(Error::Worktree)?;
        let records = log.read_all().map_err(Error::Worktree)?;
        if records.is_empty() {
            continue;
        }

        // Stable, deduplicated file set.
        let files: BTreeSet<PathBuf> = records.iter().map(|r| r.file_path.clone()).collect();
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        let msg = format!("group {gid}: {} symbol writes", records.len());
        branch::commit_paths(repo_root, &file_refs, &msg).map_err(Error::Worktree)?;
    }
    Ok(())
}
