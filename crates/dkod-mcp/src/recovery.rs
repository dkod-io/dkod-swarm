//! Restart-time recovery of an in-flight session.
//!
//! Design `§State` says: "if the orchestrator or Claude Code crashes,
//! `dkod status` re-reads state and resumes." The MCP server achieves that
//! by scanning `.dkod/sessions/` at startup for a manifest whose status is
//! `Executing` and re-adopting it as the in-memory active session.

use crate::{Error, Result, ServerCtx};
use dkod_worktree::{Manifest, Paths, SessionId, SessionStatus};

/// Scan `.dkod/sessions/<id>/manifest.json` and return the id of the first
/// session whose status is `Executing`. `None` if no such session exists.
///
/// If multiple executing sessions are found (should never happen in
/// practice — it would only occur from external tampering), returns the
/// first one encountered in directory-scan order. The orchestrator
/// invariant is "at most one active session per repo"; violating it is
/// caller error.
///
/// Corrupt or mid-write manifests are silently skipped: recovery is
/// best-effort, and a truncated JSON blob is indistinguishable from a
/// never-finished write. The caller (usually `ServerCtx::recover` on
/// startup) gets either a usable session id or `None`, never a partial
/// state.
pub fn scan_executing_session(paths: &Paths) -> Result<Option<SessionId>> {
    let dir = paths.sessions_dir();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        // A fresh repo has no `sessions/` dir yet — that's a clean
        // "nothing to recover" signal, not an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::Io(e)),
    };
    for entry in rd {
        let entry = entry.map_err(Error::Io)?;
        let ft = entry.file_type().map_err(Error::Io)?;
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let sid = SessionId::from_raw(name);
        match Manifest::load(paths, &sid) {
            Ok(m) if matches!(m.status, SessionStatus::Executing) => {
                return Ok(Some(sid));
            }
            Ok(_) => {}
            Err(_) => {
                // Corrupt or mid-write manifests are skipped — recovery is
                // best-effort. A future `dkod status` surface can surface
                // these for operator attention.
            }
        }
    }
    Ok(None)
}

impl ServerCtx {
    /// Best-effort recovery: pick up any on-disk Executing session as the
    /// current in-memory session. Called once at startup from `main.rs`
    /// before the server begins accepting MCP requests.
    ///
    /// Async because populating `active_session` requires the tokio mutex.
    /// Intentionally not called from `ServerCtx::new` — `new` must stay
    /// synchronous and side-effect-free so tests can construct a context
    /// outside an async runtime.
    pub async fn recover(&self) -> Result<()> {
        if let Some(sid) = scan_executing_session(&self.paths)? {
            let mut active = self.active_session.lock().await;
            if active.is_none() {
                *active = Some(sid);
            }
        }
        Ok(())
    }
}
