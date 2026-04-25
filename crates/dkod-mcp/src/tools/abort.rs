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
    // Phase 1 — take and clear `active_session` immediately, holding only
    // that one mutex briefly. Clearing it here gates any subsequent writer:
    // a writer that captured `sid` before this call still has to re-check
    // `active_session` under its per-file lock (`tools/write_symbol.rs`
    // does that), and will see `None` and abort its own work.
    //
    // We restore the value on Phase 3 failure so the caller can retry the
    // abort cleanly instead of getting stuck with no active session and a
    // half-destroyed branch.
    let sid = {
        let mut active = ctx.active_session.lock().await;
        active.take().ok_or(Error::NoActiveSession)?
    };

    // Phase 2 — drain in-flight per-file writer locks WITHOUT holding any
    // outer guard. Snapshotting `Arc`s under the outer `file_locks` guard
    // gives us a stable list of locks that existed at this moment; we then
    // drop the outer guard before awaiting each inner lock so a writer
    // doing its re-check (which takes `active_session`) cannot deadlock
    // against us. Any writer that arrives AFTER our snapshot has already
    // observed `active_session = None` (Phase 1) and bails out.
    let drained: Vec<_> = {
        let locks = ctx.file_locks.lock().await;
        locks.values().map(std::sync::Arc::clone).collect()
    };
    for lock in drained {
        let _flush = lock.lock().await;
    }

    // Phase 3 — actual abort work. Encapsulated in an async block so we can
    // restore `active_session` on failure with a single `?` site.
    let result = async {
        // Prefer the main-branch name recorded in `.dkod/config.toml` at
        // `init_repo` time. `branch::detect_main` is unreliable here
        // because HEAD is currently the dk-branch — calling destroy with
        // that as "main" would try to check out, then delete, the very
        // branch we are on.
        let main = ctx.resolve_main()?;

        // Mark the manifest Aborted BEFORE destroying the dk-branch. That
        // way a crash or git failure during branch destruction still
        // leaves a consistent on-disk "this session is aborted" record —
        // the dead branch can be cleaned up later without misleading
        // restart-recovery into thinking the session is still Executing.
        //
        // A missing or malformed manifest is tolerated (this flow may be
        // a retry after a prior partial abort). A save failure IS fatal:
        // if we cannot persist `Aborted` to disk we must not destroy the
        // branch.
        match Manifest::load(&ctx.paths, &sid) {
            Ok(mut m) => {
                m.status = SessionStatus::Aborted;
                m.save(&ctx.paths)?;
            }
            Err(e) => {
                eprintln!(
                    "dkod-mcp abort: could not load manifest for {sid} (continuing): {e}"
                );
            }
        }

        branch::destroy_dk_branch(&ctx.repo_root, &main, sid.as_str())?;

        // Clear the lock table now that no in-flight writer can be
        // referencing entries (drained in Phase 2; new writers gated by
        // `active_session = None`).
        ctx.file_locks.lock().await.clear();
        Ok::<_, Error>(())
    }
    .await;

    if let Err(e) = result {
        // Restore active_session so the caller can retry the abort.
        *ctx.active_session.lock().await = Some(sid);
        return Err(e);
    }

    Ok(AbortResponse {
        session_id: sid.to_string(),
    })
}
