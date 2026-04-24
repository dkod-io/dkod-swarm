use crate::schema::{PlanGroup, PlanRequest, PlanResponse, PlanSymbol};
use crate::{Error, Result, ServerCtx};
use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::partition::partition;
use dkod_orchestrator::symbols::extract_rust_file;

/// Pure helper used by both the MCP wrapper and unit tests.
pub fn build_plan(ctx: &ServerCtx, req: PlanRequest) -> Result<PlanResponse> {
    if req.target_groups == 0 {
        return Err(Error::InvalidArg("target_groups must be >= 1".into()));
    }
    let mut all_symbols = Vec::new();
    let mut all_edges = Vec::new();
    for rel in &req.files {
        let abs = ctx.repo_root.join(rel);
        let bytes = std::fs::read(&abs).map_err(Error::Io)?;
        let (syms, edges) = extract_rust_file(&bytes, &abs)?;
        all_symbols.extend(syms);
        all_edges.extend(edges);
    }
    let graph = CallGraph::build(&all_symbols, &all_edges);
    let part = partition(&req.in_scope, &graph, req.target_groups)?;

    let groups = part
        .groups
        .into_iter()
        .map(|g| PlanGroup {
            id: g.id,
            symbols: g
                .symbols
                .into_iter()
                .map(|s| PlanSymbol {
                    qualified_name: s.qualified_name,
                    file_path: s.file_path,
                    kind: s.kind,
                })
                .collect(),
        })
        .collect();
    let warnings = part
        .warnings
        .into_iter()
        .map(|w| format!("{w:?}"))
        .collect();

    Ok(PlanResponse {
        session_preview_id: None,
        groups,
        warnings,
        unresolved_edges: graph.unresolved_count(),
    })
}

/// Map `dkod_mcp::Error` to `rmcp::ErrorData` preserving the message.
pub(crate) fn to_rmcp_error(e: Error) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(e.to_string(), None)
}
