#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("worktree error: {0}")]
    Worktree(#[from] dkod_worktree::Error),

    #[error("orchestrator error: {0}")]
    Orchestrator(#[from] dkod_orchestrator::Error),

    #[error("no active session — call dkod_execute_begin first")]
    NoActiveSession,

    #[error("session already active: {0}")]
    SessionAlreadyActive(String),

    #[error("group not found in active session: {0}")]
    UnknownGroup(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("gh subprocess failed: {cmd}: {stderr}")]
    Gh { cmd: String, stderr: String },

    #[error("git subprocess failed: {cmd}: {stderr}")]
    Git { cmd: String, stderr: String },

    #[error("verify_cmd failed (exit {exit}): {tail}")]
    VerifyFailed { exit: i32, tail: String },

    #[error("invalid argument: {0}")]
    InvalidArg(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Map our error variants onto MCP / JSON-RPC response codes. Variants that
/// describe bad client input get `invalid_params`; everything else (I/O,
/// wrapped sub-errors, subprocess failures) gets `internal_error`. Used by
/// every `#[tool]` wrapper's `.map_err(Into::into)`; defined here (rather
/// than in `tools/plan.rs`) so later tool modules don't need to reach into
/// the `plan` module to borrow a helper.
impl From<Error> for rmcp::ErrorData {
    fn from(e: Error) -> Self {
        let message = e.to_string();
        match e {
            Error::InvalidArg(_)
            | Error::UnknownGroup(_)
            | Error::NoActiveSession
            | Error::SessionAlreadyActive(_) => rmcp::ErrorData::invalid_params(message, None),
            _ => rmcp::ErrorData::internal_error(message, None),
        }
    }
}
