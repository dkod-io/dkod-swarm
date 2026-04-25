//! `dkod abort` — destroy the active dk-branch and clear session state.
//!
//! `render` and `run` are `async` because they await `ServerCtx::recover`
//! (rebuilds in-memory state from the on-disk manifest) and the
//! `dkod-mcp` `abort` helper (which awaits the active-session and
//! per-file tokio mutexes while it tears down the session).

use dkod_mcp::ServerCtx;
use dkod_mcp::tools::abort::abort;
use std::path::Path;
use std::sync::Arc;

/// Run `dkod_abort` against an on-disk session. Returns the JSON
/// response so tests can introspect it without capturing stdout.
pub async fn render(repo_root: &Path) -> anyhow::Result<String> {
    let ctx = Arc::new(ServerCtx::new(repo_root));
    ctx.recover()
        .await
        .map_err(|e| anyhow::anyhow!("ServerCtx::recover failed: {e}"))?;
    let resp = abort(&ctx)
        .await
        .map_err(|e| anyhow::anyhow!("abort helper failed: {e}"))?;
    serde_json::to_string_pretty(&resp)
        .map_err(|e| anyhow::anyhow!("serialise abort response: {e}"))
}

/// `dkod abort` entry — prints the rendered JSON to stdout.
pub async fn run(repo_root: &Path) -> anyhow::Result<()> {
    let json = render(repo_root).await?;
    println!("{json}");
    Ok(())
}
