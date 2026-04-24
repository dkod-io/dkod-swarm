use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("toml decode error in {path}: {source}")]
    TomlDecode {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("toml encode error: {0}")]
    TomlEncode(#[from] toml::ser::Error),

    #[error("json error in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("git command failed: {cmd}: {stderr}")]
    Git { cmd: String, stderr: String },

    #[error("invalid state: {0}")]
    Invalid(String),

    #[error("not initialised: .dkod/ missing at {0}")]
    NotInitialised(PathBuf),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
