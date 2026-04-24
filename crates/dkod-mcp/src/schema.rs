use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanRequest {
    /// The user's natural-language task description. Stored on the session
    /// manifest when execute_begin fires; not used for partitioning.
    pub task_prompt: String,
    /// Qualified symbol names the caller wants to partition (typically the
    /// output of Claude's scoping pass). Names that do not resolve in the
    /// call graph surface as `ScopeSymbolUnknown` warnings.
    pub in_scope: Vec<String>,
    /// Rust source files to read for symbol/call extraction, relative to
    /// the repo root.
    pub files: Vec<PathBuf>,
    /// Desired number of groups. Mismatches produce warnings; the partition
    /// is never artificially inflated or deflated.
    pub target_groups: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanGroup {
    pub id: String,
    pub symbols: Vec<PlanSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanSymbol {
    pub qualified_name: String,
    pub file_path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanResponse {
    /// Reserved for a future "dry-run" flow where `dkod_plan` pre-allocates
    /// a session id. Always `None` in v0 — `dkod_execute_begin` mints the id.
    pub session_preview_id: Option<String>,
    pub groups: Vec<PlanGroup>,
    pub warnings: Vec<String>,
    /// Number of call edges whose caller or callee could not be resolved to
    /// a known symbol. Purely informational (normal for edges to external
    /// dependencies).
    pub unresolved_edges: usize,
}
