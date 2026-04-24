use crate::schema::{PlanGroup, PlanRequest, PlanResponse, PlanSymbol};
use crate::{Error, Result, ServerCtx};
use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::partition::partition;
use dkod_orchestrator::symbols::extract_rust_file;
use std::path::{Path, PathBuf};

/// Canonicalise `rel` against an already-canonicalised `canonical_repo` and
/// reject anything that escapes the repo. Defends against three shapes of
/// malicious input from an MCP caller: (a) absolute paths, (b) `..`
/// traversal, (c) symlinks pointing outside the repo. The caller is
/// expected to pass `ctx.repo_root` through `canonicalize` once per
/// `build_plan` invocation — hoisting the canonicalise call out of this
/// per-file path avoids N redundant `realpath()` syscalls on large requests.
fn resolve_under_repo(canonical_repo: &Path, rel: &Path) -> Result<PathBuf> {
    if rel.is_absolute() {
        return Err(Error::InvalidArg(format!(
            "path must be relative to the repo root, got absolute: {}",
            rel.display()
        )));
    }
    let canonical_target = std::fs::canonicalize(canonical_repo.join(rel)).map_err(|e| {
        Error::InvalidArg(format!(
            "cannot resolve {} under repo root: {e}",
            rel.display()
        ))
    })?;
    if !canonical_target.starts_with(canonical_repo) {
        return Err(Error::InvalidArg(format!(
            "path escapes repo root: {} resolves to {}",
            rel.display(),
            canonical_target.display()
        )));
    }
    Ok(canonical_target)
}

/// Pure helper used by both the MCP wrapper and unit tests.
///
/// Synchronous on purpose — every call site (the MCP `#[tool]` wrapper in
/// `tools/mod.rs` and the unit tests) drives it through
/// `tokio::task::spawn_blocking` so the tokio executor thread is never held
/// while tree-sitter parses or while the partitioner walks the call graph.
pub fn build_plan(ctx: &ServerCtx, req: PlanRequest) -> Result<PlanResponse> {
    if req.target_groups == 0 {
        return Err(Error::InvalidArg("target_groups must be >= 1".into()));
    }
    let canonical_repo = std::fs::canonicalize(&ctx.repo_root).map_err(Error::Io)?;
    let mut all_symbols = Vec::new();
    let mut all_edges = Vec::new();
    for rel in &req.files {
        let abs = resolve_under_repo(&canonical_repo, rel)?;
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
