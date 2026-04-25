//! Pure helper for the `dkod_execute_begin` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this lives in `tools/mod.rs`. This
//! helper is `async` because it awaits `ctx.active_session.lock()`; the
//! filesystem and git work it performs is otherwise synchronous and brief
//! (a single `git checkout -b` plus a handful of small JSON writes). That
//! matches M2's scope — if this ever grows into heavier per-group setup,
//! future work can lift the sync I/O into `spawn_blocking` the way `plan`
//! already does for the tree-sitter pass.

use crate::schema::{ExecuteBeginRequest, ExecuteBeginResponse};
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{
    GroupSpec, GroupStatus, Manifest, SessionId, SessionStatus, SymbolRef, branch,
};

pub async fn execute_begin(
    ctx: &ServerCtx,
    req: ExecuteBeginRequest,
) -> Result<ExecuteBeginResponse> {
    if req.groups.is_empty() {
        return Err(Error::InvalidArg("groups must be non-empty".into()));
    }

    // Guard the session-singleton invariant. We keep the guard for the full
    // function body — no other tokio task can take it anyway (the server
    // accepts only one active session per process), and holding it through
    // the sync I/O means a concurrent `execute_begin` caller observes either
    // "no session" or "session fully established", never a half-written one.
    let mut active = ctx.active_session.lock().await;
    if let Some(sid) = active.as_ref() {
        return Err(Error::SessionAlreadyActive(sid.to_string()));
    }

    let sid = SessionId::generate();
    // Use `ctx.resolve_main()` for consistency with `abort` / future
    // `commit` / `pr`, even though at this point HEAD is still on the true
    // main so `detect_main` would also work.
    let main = ctx.resolve_main()?;
    branch::create_dk_branch(&ctx.repo_root, &main, sid.as_str())?;

    // Once the branch exists, any I/O failure below must be unwound: since
    // `active_session` is not set until the very end, the caller cannot
    // reach the orphan branch via `dkod_abort`. The closure pattern groups
    // the fallible writes so we have one place to clean up on error.
    let group_ids: Vec<String> = req.groups.iter().map(|g| g.id.clone()).collect();
    let result = (|| -> Result<()> {
        // Persist all GroupSpec entries BEFORE writing the manifest. That
        // way a crash between spec writes and the manifest leaves no
        // `Executing` manifest on disk, so restart-recovery won't pick up
        // a half-written session.
        for g in req.groups {
            let spec = GroupSpec {
                id: g.id,
                symbols: g
                    .symbols
                    .into_iter()
                    .map(|s| SymbolRef {
                        qualified_name: s.qualified_name,
                        file_path: s.file_path,
                        kind: s.kind,
                    })
                    .collect(),
                agent_prompt: g.agent_prompt,
                status: GroupStatus::Pending,
            };
            spec.save(&ctx.paths, &sid)?;
        }

        let manifest = Manifest {
            session_id: sid.clone(),
            task_prompt: req.task_prompt,
            created_at: crate::time::iso8601_now(),
            status: SessionStatus::Executing,
            group_ids: group_ids.clone(),
        };
        manifest.save(&ctx.paths)?;
        Ok(())
    })();

    if let Err(e) = result {
        // Best-effort rollback — cleanup failures are logged but do not
        // replace the original error returned to the caller (a useful
        // error for the user is more important than an error about the
        // cleanup). Orphan `dk/<sid>` branches from a *crash* (not an
        // error return) between `create_dk_branch` and `manifest.save`
        // are a known limitation of M2-3: `recover()` only looks for
        // manifests with `Executing` status, so a crashed partial-begin
        // leaves a stale branch until the user cleans it up manually.
        // A future PR can add a "preparing" marker file that recovery
        // reconciles.
        if let Err(rollback_err) = branch::destroy_dk_branch(&ctx.repo_root, &main, sid.as_str()) {
            eprintln!(
                "dkod-mcp execute_begin: rollback destroy_dk_branch failed for {sid}: {rollback_err}"
            );
        }
        if let Ok(session_dir) = ctx.paths.session(sid.as_str())
            && session_dir.exists()
            && let Err(rm_err) = std::fs::remove_dir_all(&session_dir)
        {
            eprintln!(
                "dkod-mcp execute_begin: rollback remove session dir {} failed: {rm_err}",
                session_dir.display()
            );
        }
        return Err(e);
    }

    let resp = ExecuteBeginResponse {
        session_id: sid.to_string(),
        dk_branch: branch::dk_branch_name(sid.as_str()),
        group_ids,
    };
    *active = Some(sid);
    Ok(resp)
}
