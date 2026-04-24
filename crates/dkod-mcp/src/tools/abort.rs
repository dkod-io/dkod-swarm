//! Pure helper for the `dkod_abort` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this lives in `tools/mod.rs`. Like
//! `execute_begin`, this helper is `async` because it awaits the tokio
//! mutex holding the active-session id; the git + filesystem work it
//! performs is synchronous and brief.

use crate::schema::AbortResponse;
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{Manifest, SessionStatus, branch};

pub async fn abort(ctx: &ServerCtx) -> Result<AbortResponse> {
    // Acquire BOTH guards up front so no other task can observe an
    // intermediate state where the branch is gone but `active_session` is
    // still set, or where a future writer sees "no active session" and
    // re-acquires a file lock from `ServerCtx`. Ordering: `active_session`
    // first, then `file_locks`, to match the acquisition order used by
    // other paths and avoid a deadlock.
    let mut active = ctx.active_session.lock().await;
    let mut locks = ctx.file_locks.lock().await;
    let sid = active.as_ref().ok_or(Error::NoActiveSession)?.clone();

    // Prefer the main-branch name recorded in `.dkod/config.toml` at
    // `init_repo` time. `branch::detect_main` is unreliable here because
    // HEAD is currently the dk-branch (tier 1 of the detection walk
    // reflects HEAD) — calling destroy with that as "main" would try to
    // check out, then delete, the very branch we are on.
    let main = ctx.resolve_main()?;

    // Mark the manifest Aborted BEFORE destroying the dk-branch. That way
    // a crash or git failure during branch destruction still leaves a
    // consistent on-disk "this session is aborted" record — the dead
    // branch can be cleaned up later without misleading restart-recovery
    // into thinking the session is still Executing.
    //
    // A missing or malformed manifest is tolerated (this flow may be a
    // retry after a prior partial abort). Save failures are logged but
    // non-fatal — the in-memory state must still be cleared downstream
    // so the session isn't stuck "active".
    if let Ok(mut m) = Manifest::load(&ctx.paths, &sid) {
        m.status = SessionStatus::Aborted;
        if let Err(e) = m.save(&ctx.paths) {
            eprintln!("dkod-mcp abort: failed to persist Aborted manifest for {sid}: {e}");
        }
    }

    branch::destroy_dk_branch(&ctx.repo_root, &main, sid.as_str())?;

    // Clear file locks BEFORE clearing `active_session`. Clearing locks
    // after clearing active would leave a TOCTOU window: a future M2-4
    // writer could observe "no active session", create a fresh lock in
    // `ServerCtx`, and then have it wiped out by the clear() below.
    locks.clear();
    *active = None;

    Ok(AbortResponse {
        session_id: sid.to_string(),
    })
}
