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

    // Phase 3 — actual abort work. We track whether the on-disk manifest
    // got persisted as `Aborted` (Phase 4 uses that to decide whether to
    // restore `active_session` on a destroy failure — a session whose
    // manifest already says Aborted must NOT be re-opened, otherwise
    // recovery would diverge from the in-memory view).
    let mut manifest_aborted_persisted = false;
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
        // Only tolerate "manifest not found on disk" (a legitimate
        // retry-after-partial-abort case). Permission errors, malformed
        // JSON, or any other I/O failure must propagate so we don't
        // silently destroy the branch with an inconsistent on-disk
        // record.
        match Manifest::load(&ctx.paths, &sid) {
            Ok(mut m) => {
                m.status = SessionStatus::Aborted;
                m.save(&ctx.paths)?;
                manifest_aborted_persisted = true;
            }
            Err(dkod_worktree::Error::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                eprintln!(
                    "dkod-mcp abort: manifest for {sid} not found, continuing destroy"
                );
            }
            Err(e) => return Err(Error::from(e)),
        }

        // Discard any uncommitted writes that landed on the dk-branch
        // while abort was waiting in Phase 2's drain. Without this,
        // `git checkout main` (inside `destroy_dk_branch`) would carry
        // those modifications onto `main` as uncommitted changes — a
        // silent leak of aborted work. `git reset --hard HEAD` resets the
        // working tree to the dk-branch's tip; the subsequent checkout
        // then runs against a clean tree.
        let reset = std::process::Command::new("git")
            .args(["reset", "--hard", "HEAD"])
            .current_dir(&ctx.repo_root)
            .output()
            .map_err(Error::Io)?;
        if !reset.status.success() {
            return Err(Error::InvalidArg(format!(
                "git reset --hard HEAD failed: {}",
                String::from_utf8_lossy(&reset.stderr)
            )));
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
        // Only restore `active_session` if the manifest is still in its
        // pre-abort state on disk. If we already persisted `Aborted`
        // (Phase 3 got past `m.save()` but failed afterwards), the
        // session is logically aborted — re-opening it would let a
        // future `dkod_write_symbol` mutate a session that recovery and
        // any later `dkod_status` call would treat as closed. The
        // operator can retry the destroy by hand.
        if !manifest_aborted_persisted {
            *ctx.active_session.lock().await = Some(sid);
        } else {
            eprintln!(
                "dkod-mcp abort: manifest already persisted as Aborted; \
                 active_session left cleared. Caller should clean up the \
                 dk-branch by hand."
            );
        }
        return Err(e);
    }

    Ok(AbortResponse {
        session_id: sid.to_string(),
    })
}
