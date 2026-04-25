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
    /// Replacement source for the symbol's **entire span** — not just the
    /// inner block. `dkod_orchestrator::replace::replace_symbol` performs a
    /// full-span splice from the symbol's start byte to its end byte, so
    /// this string must be a complete replacement item. For a function
    /// rewrite, supply the full `pub fn name(...) -> T { ... }`, including
    /// signature, attributes, and braces — not just the body inside `{}`.
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteCompleteRequest {
    /// Group whose status should transition to `done`. Must belong to the
    /// active session.
    pub group_id: String,
    /// Free-form summary written by the calling agent. Persisted by appending
    /// ` — summary: <summary>` to the group's `agent_prompt` (GroupSpec has
    /// no dedicated summary field in M2; if a future PR adds one in
    /// `dkod-worktree`, this wrapper switches to that field).
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteCompleteResponse {
    pub group_id: String,
    /// Always `"done"` in M2 — `"failed"` is reserved for a future variant
    /// that records a non-recoverable agent error.
    pub new_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StatusResponse {
    /// `Some(sid)` when an `Executing` session is active in this process,
    /// `None` otherwise. `None` is not an error — callers use it to decide
    /// whether to issue `dkod_execute_begin` first.
    pub active_session_id: Option<String>,
    /// `Some("dk/<sid>")` mirroring `dkod_execute_begin`'s response, `None`
    /// when no session is active.
    pub dk_branch: Option<String>,
    /// One entry per group id on the manifest. Groups whose spec fails to
    /// load are silently skipped (they cannot meaningfully contribute a
    /// status row).
    pub groups: Vec<GroupStatusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommitResponse {
    /// Number of new commits produced on the dk-branch by this call —
    /// computed as the count of revisions between HEAD-before and
    /// HEAD-after `commit_per_group`. In the common path this equals the
    /// number of groups whose `writes.jsonl` had at least one record
    /// (groups with an empty log are silently skipped by
    /// `commit_per_group`), but the value is bound to the actual git
    /// rev-list result, not to a group-count assumption.
    pub commits_created: usize,
    /// `dk/<session-id>` — the branch the commits live on. Returned even when
    /// `commits_created == 0` so callers always know which branch to push.
    pub dk_branch: String,
    /// Short hex SHAs of each new commit, in chronological order (group order
    /// per `manifest.group_ids`). Empty when `commits_created == 0`.
    pub commit_shas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupStatusEntry {
    pub id: String,
    /// `"pending" | "in_progress" | "done" | "failed"` — stringified
    /// `dkod_worktree::GroupStatus` so MCP clients without the worktree
    /// schema can read it.
    pub status: String,
    /// Number of records appended to the group's `writes.jsonl`. A missing
    /// log file is treated as `0` (matching `WriteLog::read_all`).
    pub writes: usize,
    /// Currently echoes `agent_prompt` (which `dkod_execute_complete`
    /// appends a summary to). Optional only because future schema changes
    /// might decouple summary from prompt — for now it is always `Some`.
    pub agent_summary: Option<String>,
}
