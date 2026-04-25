//! `dkod --mcp` — stdio MCP server entry.
//!
//! This is a near-verbatim copy of the standalone `dkod-mcp` binary's
//! `main` body, exposed as a library function so the user-facing `dkod`
//! binary can launch the same `serve(stdio())` flow under the
//! `--mcp` flag.
//!
//! The standalone `dkod-mcp` binary stays in place for now (used by
//! tests and existing tooling). `dkod --mcp` is the entry the Claude
//! Code plugin will invoke once the plugin manifest lands in M4. The
//! decision to deprecate or rename the standalone binary is deferred
//! to a future milestone.
//!
//! Unlike the other `cmd::*` modules there is no `render` helper:
//! `--mcp` blocks until the client closes the stdio pair, so there is
//! nothing meaningful to materialise as a string for tests. Coverage
//! for the MCP surface itself comes from `dkod-mcp/tests/e2e_smoke.rs`
//! (M2-8), which exercises every tool through `McpServer` directly.

use dkod_mcp::{McpServer, ServerCtx};
use rmcp::{ServiceExt, transport::stdio};
use std::path::Path;
use std::sync::Arc;

/// Launch the stdio MCP server rooted at `repo_root`. Blocks until the
/// connected client (typically Claude Code) closes the stdio pair.
///
/// Mirrors `dkod-mcp`'s standalone binary: `ServerCtx::new` →
/// `recover` → `McpServer::new(ctx).serve(stdio()).await` →
/// `service.waiting().await`. Each `rmcp` boundary error is wrapped
/// with `anyhow::anyhow!` so the dispatch in `main.rs` can surface a
/// uniform error message.
pub async fn run(repo_root: &Path) -> anyhow::Result<()> {
    let ctx = Arc::new(ServerCtx::new(repo_root));
    ctx.recover()
        .await
        .map_err(|e| anyhow::anyhow!("ServerCtx::recover failed: {e}"))?;
    let service = McpServer::new(ctx)
        .serve(stdio())
        .await
        .map_err(|e| anyhow::anyhow!("MCP serve failed: {e}"))?;
    service
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("MCP wait failed: {e}"))?;
    Ok(())
}
