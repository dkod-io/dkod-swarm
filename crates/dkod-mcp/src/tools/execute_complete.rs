//! Pure helper for the `dkod_execute_complete` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this helper lives in `tools/mod.rs`
//! per the project's code-placement convention. This helper is `async`
//! solely because it awaits `ctx.active_session.lock()`; the on-disk work
//! is a single small JSON read + write of the group spec.
//!
//! Summary persistence: GroupSpec has no dedicated `summary` field in M2,
//! so we record the agent's summary by appending ` — summary: <summary>`
//! to `agent_prompt`. This keeps the M2 diff entirely inside `dkod-mcp`.
//! When `dkod-worktree` later grows a real field, switch this wrapper to
//! write it instead.

use crate::schema::{ExecuteCompleteRequest, ExecuteCompleteResponse};
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{GroupSpec, GroupStatus};

pub async fn execute_complete(
    ctx: &ServerCtx,
    req: ExecuteCompleteRequest,
) -> Result<ExecuteCompleteResponse> {
    let sid = ctx
        .active_session
        .lock()
        .await
        .clone()
        .ok_or(Error::NoActiveSession)?;

    // `GroupSpec::load` returns `Error::Io { source: NotFound, .. }` for an
    // unknown group id. We deliberately collapse every load failure to
    // `UnknownGroup` here so the MCP client sees a single "this group is not
    // part of the active session" signal instead of a leaky I/O error path.
    let mut spec = GroupSpec::load(&ctx.paths, &sid, &req.group_id)
        .map_err(|_| Error::UnknownGroup(req.group_id.clone()))?;

    spec.status = GroupStatus::Done;
    // `trim_end` keeps the appended summary readable when `agent_prompt`
    // already ends with whitespace; a fresh `agent_prompt` from
    // `execute_begin` does not, but agents may have edited it.
    spec.agent_prompt = format!(
        "{} — summary: {}",
        spec.agent_prompt.trim_end(),
        req.summary
    );
    spec.save(&ctx.paths, &sid)?;

    Ok(ExecuteCompleteResponse {
        group_id: req.group_id,
        new_status: "done".into(),
    })
}
