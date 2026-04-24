use crate::ServerCtx;
use rmcp::{ServerHandler, handler::server::router::tool::ToolRouter, tool_handler, tool_router};
use std::sync::Arc;

#[derive(Clone)]
pub struct McpServer {
    // Every tool consults `ctx`; the first real tool added in PR M2-2 will
    // start reading it. Until then, the field is live-by-intent.
    #[allow(dead_code)]
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

// Tool methods are added by later PRs as `#[tool]` methods on this single
// `impl` block. Keeping every tool in one `#[tool_router]` block side-steps
// the per-rmcp-version split-across-files pitfall.
#[tool_router]
impl McpServer {}

#[tool_handler]
impl ServerHandler for McpServer {}
