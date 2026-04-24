//! Pure helper for the `dkod_abort` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this lives in `tools/mod.rs`. Like
//! `execute_begin`, this helper is `async` because it awaits the tokio
//! mutex holding the active-session id; the git + filesystem work it
//! performs is synchronous and brief.

use crate::schema::AbortResponse;
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{Config, Manifest, SessionStatus, branch};

pub async fn abort(ctx: &ServerCtx) -> Result<AbortResponse> {
    // Take the guard for the whole body so a concurrent `execute_begin`
    // cannot observe a half-aborted state (branch gone, session field
    // still set).
    let mut active = ctx.active_session.lock().await;
    let sid = active.as_ref().ok_or(Error::NoActiveSession)?.clone();

    // Prefer the main-branch name recorded in `.dkod/config.toml` at
    // `init_repo` time. `branch::detect_main` is unreliable here because
    // HEAD is currently the dk-branch (tier 1 of the detection walk
    // reflects HEAD) — calling destroy with that as "main" would try to
    // check out, then delete, the very branch we are on.
    let main = resolve_main(ctx)?;
    branch::destroy_dk_branch(&ctx.repo_root, &main, sid.as_str())?;

    // Mark the manifest `Aborted` so restart-recovery won't re-adopt this
    // session. A missing / corrupt manifest is tolerated — the branch is
    // already gone, so there is nothing left to recover.
    if let Ok(mut m) = Manifest::load(&ctx.paths, &sid) {
        m.status = SessionStatus::Aborted;
        m.save(&ctx.paths)?;
    }

    *active = None;
    // Drop any file locks held for this session. `write_symbol` (lands in
    // M2-4) populates this table; clearing it here means a later session
    // starts with a clean slate and never contends with a stale `Arc`.
    ctx.file_locks.lock().await.clear();

    Ok(AbortResponse {
        session_id: sid.to_string(),
    })
}

/// Resolve the repo's `main` branch name for abort/commit purposes.
///
/// Reads `.dkod/config.toml` first — that value was recorded by `init_repo`
/// while HEAD was on the default branch, so it is trustworthy even after
/// `execute_begin` checks out `dk/<sid>`. Falls back to
/// `branch::detect_main` if the config file is missing (e.g. in tests that
/// bypass `init_repo`) — the fallback is inaccurate while HEAD is on a
/// dk-branch, but we surface it rather than refuse the abort.
pub(crate) fn resolve_main(ctx: &ServerCtx) -> Result<String> {
    match Config::load(&ctx.paths.config()) {
        Ok(cfg) => Ok(cfg.main_branch),
        Err(_) => Ok(branch::detect_main(&ctx.repo_root)?),
    }
}
