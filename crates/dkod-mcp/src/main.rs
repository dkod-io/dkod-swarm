//! `dkod-mcp` binary — stdio MCP server.
//!
//! Runs in the current working directory; `ServerCtx::new` rebuilds `Paths`
//! under `<cwd>/.dkod`. The hosting Claude Code plugin is responsible for
//! invoking this binary from the repo root.

use dkod_mcp::{McpServer, ServerCtx};
use rmcp::{ServiceExt, transport::stdio};
use std::sync::Arc;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("dkod-mcp fatal: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;
    let ctx = Arc::new(ServerCtx::new(&repo_root));
    let service = McpServer::new(ctx).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
