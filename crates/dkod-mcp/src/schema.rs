use rmcp::schemars;
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

/// Mirror of `dkod_worktree::SymbolRef` with a `JsonSchema` derive so it can
/// traverse the MCP boundary. Kept as a separate type (rather than deriving
/// `JsonSchema` on the worktree type) to avoid forcing a rmcp/schemars
/// dependency on `dkod-worktree`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SymbolRefSchema {
    pub qualified_name: String,
    pub file_path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupInput {
    pub id: String,
    pub symbols: Vec<SymbolRefSchema>,
    pub agent_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteBeginRequest {
    pub task_prompt: String,
    pub groups: Vec<GroupInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteBeginResponse {
    pub session_id: String,
    pub dk_branch: String,
    pub group_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbortResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WriteSymbolRequest {
    /// Group whose `writes.jsonl` we append to. Must belong to the active
    /// session.
    pub group_id: String,
    /// Source file containing `qualified_name`, relative to the repo root.
    /// Resolved through the same path-escape guard `dkod_plan` uses so an
    /// agent cannot smuggle absolute paths or `..` traversal past us.
    pub file: PathBuf,
    /// Symbol to replace. Same resolution rules as
    /// `dkod_orchestrator::replace::replace_symbol`: exact qualified-name
    /// first, then a unique short-name match.
    pub qualified_name: String,
    /// Replacement source for the symbol's span (just the symbol body,
    /// not the surrounding file).
    pub new_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WriteSymbolResponse {
    /// `"parsed_ok"` when the post-replace re-parse confirmed the symbol is
    /// still present, `"fallback"` otherwise (caller should treat as a soft
    /// warning per design §edge case #5).
    pub outcome: String,
    /// Populated when `outcome == "fallback"`. Mirrors
    /// `ReplaceOutcome::Fallback::reason`.
    pub fallback_reason: Option<String>,
    /// Number of bytes written to disk (i.e. the new file length).
    pub bytes_written: usize,
}
