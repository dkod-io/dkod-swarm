use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The upstream branch to base dk-branches on.
    pub main_branch: String,
    /// Optional shell command to run once before PR creation (M3+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_cmd: Option<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
        toml::from_str(&text)
            .map_err(|e| Error::TomlDecode { path: path.to_path_buf(), source: e })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
            }
        }
        std::fs::write(path, text)
            .map_err(|e| Error::Io { path: path.to_path_buf(), source: e })
    }
}
