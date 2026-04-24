use dkod_worktree::{Paths, SessionId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Per-process server context. One `ServerCtx` per `dkod-mcp` process.
///
/// `active_session` holds the id of the in-flight session, if any. It is set
/// by `dkod_execute_begin`, cleared by `dkod_abort` and a successful
/// `dkod_pr`. Fresh processes recover it by scanning `.dkod/sessions/` for a
/// manifest with status `Executing` (rebuilt from disk at startup, implemented in M2-3).
pub struct ServerCtx {
    pub repo_root: PathBuf,
    pub paths: Paths,
    pub active_session: Mutex<Option<SessionId>>,
    /// Per-file locks guarding `dkod_write_symbol`. Keys are the absolute
    /// paths supplied by the caller; the caller is responsible for
    /// canonicalising (`std::fs::canonicalize` or equivalent) before invoking
    /// `file_lock` so that symlinks and `./` prefixes do not create duplicate
    /// entries. Entries are created on first write to a file and live until
    /// the session ends (we intentionally do not GC mid-session).
    pub file_locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl ServerCtx {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            paths: Paths::new(repo_root),
            active_session: Mutex::new(None),
            file_locks: Mutex::new(HashMap::new()),
        }
    }

    /// Fetch or create the lock for `abs_path`. The caller must pass a path
    /// that is already in whatever canonical form they will use consistently
    /// — two calls with different representations of the same file (symlink
    /// vs target, absolute vs relative) will receive distinct locks. Returns
    /// an `Arc<Mutex<()>>` that the caller can `.lock().await` independently
    /// of the map.
    pub async fn file_lock(&self, abs_path: &Path) -> Arc<Mutex<()>> {
        let mut map = self.file_locks.lock().await;
        map.entry(abs_path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn file_lock_returns_same_arc_for_same_path() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ServerCtx::new(tmp.path());
        let a = ctx.file_lock(Path::new("/tmp/x")).await;
        let b = ctx.file_lock(Path::new("/tmp/x")).await;
        assert!(Arc::ptr_eq(&a, &b), "same path should share a lock");
    }

    #[tokio::test]
    async fn file_lock_returns_distinct_arcs_for_different_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ServerCtx::new(tmp.path());
        let a = ctx.file_lock(Path::new("/tmp/x")).await;
        let b = ctx.file_lock(Path::new("/tmp/y")).await;
        assert!(
            !Arc::ptr_eq(&a, &b),
            "different paths should have distinct locks"
        );
    }
}
