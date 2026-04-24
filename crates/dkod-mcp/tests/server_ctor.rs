use dkod_mcp::{McpServer, ServerCtx};
use std::sync::Arc;

#[test]
fn server_constructs_with_ctx() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Arc::new(ServerCtx::new(tmp.path()));
    let _srv = McpServer::new(ctx);
}
