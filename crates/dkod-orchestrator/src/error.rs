use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("worktree error: {0}")]
    Worktree(#[from] dkod_worktree::Error),

    #[error("engine parser error: {0}")]
    Engine(String),

    #[error("io at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("symbol {name} not found in {file}")]
    SymbolNotFound { name: String, file: PathBuf },

    #[error("partition input invalid: {0}")]
    InvalidPartition(String),

    #[error("replace failed: {0}")]
    ReplaceFailed(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
