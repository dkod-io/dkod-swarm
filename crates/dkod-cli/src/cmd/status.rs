//! `dkod status` — print the current session as pretty JSON.
//!
//! `render` and `run` are `async` because they await `ServerCtx::recover`
//! (rebuilds in-memory state from the on-disk manifest) and the
//! `dkod-mcp` `status` helper (which awaits the active-session tokio
//! mutex).

use dkod_mcp::ServerCtx;
use dkod_mcp::tools::status::status;
use std::path::Path;
use std::sync::Arc;

/// Render the current session as pretty JSON. Pure helper — `run` calls
/// this and prints to stdout.
pub async fn render(repo_root: &Path) -> anyhow::Result<String> {
    let ctx = Arc::new(ServerCtx::new(repo_root));
    ctx.recover()
        .await
        .map_err(|e| anyhow::anyhow!("ServerCtx::recover failed: {e}"))?;
    let resp = status(&ctx)
        .await
        .map_err(|e| anyhow::anyhow!("status helper failed: {e}"))?;
    serde_json::to_string_pretty(&resp)
        .map_err(|e| anyhow::anyhow!("serialise status response: {e}"))
}

/// `dkod status` entry — prints the rendered JSON to stdout.
pub async fn run(repo_root: &Path) -> anyhow::Result<()> {
    let json = render(repo_root).await?;
    println!("{json}");
    Ok(())
}
