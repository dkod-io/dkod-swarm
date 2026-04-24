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
/// manifest with status `Executing` (see `recovery.rs`).
pub struct ServerCtx {
    pub repo_root: PathBuf,
    pub paths: Paths,
    pub active_session: Mutex<Option<SessionId>>,
    /// Per-file locks guarding `dkod_write_symbol`. Keyed by canonicalized
    /// absolute path. Entries are created on first write to a file and live
    /// until the session ends (we intentionally do not GC mid-session).
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

    /// Fetch or create the lock for `abs_path`. Returns an `Arc<Mutex<()>>`
    /// that the caller can `.lock().await` independently of the map.
    pub async fn file_lock(&self, abs_path: &Path) -> Arc<Mutex<()>> {
        let mut map = self.file_locks.lock().await;
        map.entry(abs_path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
