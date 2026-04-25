//! Pure helper for the `dkod_status` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this helper lives in `tools/mod.rs`
//! per the project's code-placement convention. `dkod_status` is a
//! read-only snapshot: no mutation, no per-file lock, no manifest writes.
//! It is the cheapest tool in the surface.
//!
//! Behaviour summary:
//! - No active session: returns an empty `StatusResponse` (not an error).
//!   Callers use this to decide whether to issue `dkod_execute_begin`.
//! - Active session: enumerates `manifest.group_ids` in order, loading
//!   each `GroupSpec` and counting the entries in its `writes.jsonl`.
//!   A spec that fails to load is silently skipped — the alternative
//!   (returning a half-broken status row or surfacing an internal error)
//!   would be less useful to the caller.
//! - `WriteLog::open` / `read_all` errors degrade to `writes = 0`. A
//!   missing log file is already `Ok(vec![])` per the M1 contract; only
//!   a corrupt or unreadable log degrades, and it is not worth failing
//!   the entire status call over a single group's broken log.

use crate::schema::{GroupStatusEntry, StatusResponse};
use crate::{Result, ServerCtx};
use dkod_worktree::{GroupSpec, GroupStatus, Manifest, WriteLog, branch};

pub async fn status(ctx: &ServerCtx) -> Result<StatusResponse> {
    let active = ctx.active_session.lock().await.clone();
    let Some(sid) = active else {
        return Ok(StatusResponse {
            active_session_id: None,
            dk_branch: None,
            groups: Vec::new(),
        });
    };

    let manifest = Manifest::load(&ctx.paths, &sid)?;
    let mut groups = Vec::with_capacity(manifest.group_ids.len());
    for gid in &manifest.group_ids {
        let spec = match GroupSpec::load(&ctx.paths, &sid, gid) {
            Ok(s) => s,
            // Per the module-level note: skip rather than fail. A missing
            // spec for a manifest-listed group is a partial-write artefact
            // that recovery will reconcile; in the meantime we still want
            // status to return something useful for the other groups.
            Err(_) => continue,
        };
        let writes = WriteLog::open(&ctx.paths, &sid, gid)
            .and_then(|l| l.read_all())
            .map(|v| v.len())
            .unwrap_or(0);
        let status = match spec.status {
            GroupStatus::Pending => "pending",
            GroupStatus::InProgress => "in_progress",
            GroupStatus::Done => "done",
            GroupStatus::Failed => "failed",
        };
        groups.push(GroupStatusEntry {
            id: gid.clone(),
            status: status.into(),
            writes,
            agent_summary: Some(spec.agent_prompt),
        });
    }

    Ok(StatusResponse {
        active_session_id: Some(sid.to_string()),
        dk_branch: Some(branch::dk_branch_name(sid.as_str())),
        groups,
    })
}
