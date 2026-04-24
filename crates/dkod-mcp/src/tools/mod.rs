pub mod plan;

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
        plan::build_plan(&self.ctx, req).map(Json).map_err(Into::into)
    }
}

#[tool_handler]
impl ServerHandler for McpServer {}
