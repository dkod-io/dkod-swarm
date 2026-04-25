pub mod abort;
pub mod commit;
pub mod execute_begin;
pub mod execute_complete;
pub mod path;
pub mod plan;
pub mod pr;
pub mod status;
pub mod write_symbol;

use crate::ServerCtx;
use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    tool, tool_handler, tool_router,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct McpServer {
    // Every tool consults `ctx`; read by `#[tool]` methods below.
    pub(crate) ctx: Arc<ServerCtx>,
    // The `#[tool_router]` macro generates a `Self::tool_router()` ctor that
    // populates this field; the dead-code pass does not track usage through
    // the macro expansion.
    #[allow(dead_code)]
    tool_router: ToolRouter<McpServer>,
}

impl McpServer {
    pub fn new(ctx: Arc<ServerCtx>) -> Self {
        Self {
            ctx,
            tool_router: Self::tool_router(),
        }
    }
}

// Every `#[tool]` method lives in this single `#[tool_router] impl McpServer`
// block. Submodules (`plan.rs`, …) expose pure-function helpers only — this
// keeps the router attribute surface single-block and side-steps the
// per-rmcp-version split-across-files pitfall.
#[tool_router]
impl McpServer {
    #[tool(
        description = "Plan a task: reads files, builds call graph, partitions in-scope symbols into disjoint groups by call coupling."
    )]
    pub async fn dkod_plan(
        &self,
        Parameters(req): Parameters<crate::schema::PlanRequest>,
    ) -> std::result::Result<Json<crate::schema::PlanResponse>, rmcp::ErrorData> {
        // `build_plan` reads files from disk and runs tree-sitter +
        // partition algorithms — both synchronous, potentially slow on a
        // real-world codebase. `spawn_blocking` keeps the tokio executor
        // thread free while that work runs.
        let ctx = self.ctx.clone();
        tokio::task::spawn_blocking(move || plan::build_plan(&ctx, req))
            .await
            .map_err(|e| {
                rmcp::ErrorData::internal_error(format!("spawn_blocking join error: {e}"), None)
            })?
            .map(Json)
            .map_err(Into::into)
    }

    #[tool(
        description = "Begin execution: mint a session id, create dk/<sid> branch off main, and persist manifest + per-group specs under .dkod/sessions/."
    )]
    pub async fn dkod_execute_begin(
        &self,
        Parameters(req): Parameters<crate::schema::ExecuteBeginRequest>,
    ) -> std::result::Result<Json<crate::schema::ExecuteBeginResponse>, rmcp::ErrorData> {
        // `execute_begin` is light sync I/O plus a single `git checkout -b`;
        // run it directly on the async path. The heavier tree-sitter work
        // is what justifies `spawn_blocking` for `dkod_plan`; this tool is
        // not in that class for M2.
        execute_begin::execute_begin(&self.ctx, req)
            .await
            .map(Json)
            .map_err(Into::into)
    }

    #[tool(
        description = "Abort the active session: destroy dk/<sid>, mark the manifest Aborted, and clear the in-memory session + file-lock state."
    )]
    pub async fn dkod_abort(
        &self,
    ) -> std::result::Result<Json<crate::schema::AbortResponse>, rmcp::ErrorData> {
        // Same rationale as `dkod_execute_begin`: brief sync git + I/O, no
        // need for `spawn_blocking` in M2.
        abort::abort(&self.ctx).await.map(Json).map_err(Into::into)
    }

    #[tool(
        description = "AST-level symbol replacement: holds a per-file lock, replaces the named symbol with new_body, re-parses, and appends to writes.jsonl."
    )]
    pub async fn dkod_write_symbol(
        &self,
        Parameters(req): Parameters<crate::schema::WriteSymbolRequest>,
    ) -> std::result::Result<Json<crate::schema::WriteSymbolResponse>, rmcp::ErrorData> {
        // No `spawn_blocking` here — see the module-level comment in
        // `write_symbol.rs`. The helper holds a `tokio::sync::Mutex` guard
        // across the read-modify-write, and that guard cannot cross a
        // thread boundary cleanly. Future optimisation: hoist the
        // `replace_symbol` parse onto a blocking thread via a held-guard
        // channel pattern if profiling shows it matters.
        write_symbol::write_symbol(&self.ctx, req)
            .await
            .map(Json)
            .map_err(Into::into)
    }

    #[tool(description = "Mark a group done; records the agent's summary on the group spec.")]
    pub async fn dkod_execute_complete(
        &self,
        Parameters(req): Parameters<crate::schema::ExecuteCompleteRequest>,
    ) -> std::result::Result<Json<crate::schema::ExecuteCompleteResponse>, rmcp::ErrorData> {
        // No `spawn_blocking`: the helper performs only a brief mutex
        // acquire plus a single small JSON read + atomic write of the
        // group spec. Same rationale as `dkod_execute_begin` /
        // `dkod_abort` above.
        execute_complete::execute_complete(&self.ctx, req)
            .await
            .map(Json)
            .map_err(Into::into)
    }

    #[tool(
        description = "Return the active session id, dk-branch, and per-group status + write count. No-op-safe when no session is active."
    )]
    pub async fn dkod_status(
        &self,
    ) -> std::result::Result<Json<crate::schema::StatusResponse>, rmcp::ErrorData> {
        // Read-only: no `spawn_blocking`, no per-file lock. Cost is one
        // mutex acquire plus N small JSON reads (one manifest + one spec
        // and one writes log per group); negligible vs. the executor cost
        // of switching to a blocking thread.
        status::status(&self.ctx)
            .await
            .map(Json)
            .map_err(Into::into)
    }

    #[tool(
        description = "Finalize the active session by writing one commit per group (with writes) on the dk-branch. Identity is forced to Haim Ari <haimari1@gmail.com>. Marks the manifest Committed."
    )]
    pub async fn dkod_commit(
        &self,
    ) -> std::result::Result<Json<crate::schema::CommitResponse>, rmcp::ErrorData> {
        // The session-id read is async (tokio Mutex), but every subsequent
        // step shells out to git: `rev-parse`, `rev-list`, plus the
        // per-group `git add` + `git commit` chain that `commit_per_group`
        // runs internally. Hand the sid off to `commit_inner` on a blocking
        // thread so the tokio executor can keep driving other tool calls
        // while git runs. Pattern: same split as `dkod_plan`'s sync helper +
        // `spawn_blocking` wrapper.
        let sid = self
            .ctx
            .active_session
            .lock()
            .await
            .clone()
            .ok_or(crate::Error::NoActiveSession)
            .map_err(rmcp::ErrorData::from)?;
        let ctx = self.ctx.clone();
        tokio::task::spawn_blocking(move || commit::commit_inner(&ctx.repo_root, &ctx.paths, sid))
            .await
            .map_err(|e| {
                rmcp::ErrorData::internal_error(format!("spawn_blocking join error: {e}"), None)
            })?
            .map(Json)
            .map_err(Into::into)
    }

    #[tool(
        description = "Run verify_cmd, push dk/<sid> with --force-with-lease, and create a PR via gh. Idempotent: if a PR already exists for the dk-branch, returns its URL with was_existing=true and skips push + create."
    )]
    pub async fn dkod_pr(
        &self,
        Parameters(req): Parameters<crate::schema::PrRequest>,
    ) -> std::result::Result<Json<crate::schema::PrResponse>, rmcp::ErrorData> {
        // `pr::pr` (which delegates to `pr_with_shim` with `path_prefix:
        // None`) handles the async/sync split itself: it captures the
        // session id on the async path, hands the subprocess work to
        // `tokio::task::spawn_blocking`, then re-acquires the
        // `active_session` lock to clear it on success — matching the
        // M2-6 `dkod_commit` pattern.
        pr::pr(&self.ctx, req).await.map(Json).map_err(Into::into)
    }
}

#[tool_handler]
impl ServerHandler for McpServer {}
