use crate::{Error, Paths, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a new session id: `sess-<12 hex chars>`.
    ///
    /// Uses nanoseconds from the system clock. No crypto guarantees;
    /// session ids are not secrets. Collision-safe for single-user sessions
    /// that are not created in rapid succession.
    pub fn generate() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        // 12 hex chars = 48 bits — collision-safe for single-user sessions.
        let s = format!("sess-{:012x}", nanos & 0xffff_ffff_ffff);
        Self(s)
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Planned,
    Executing,
    Committed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub session_id: SessionId,
    pub task_prompt: String,
    pub created_at: String,          // ISO-8601, opaque to this crate
    pub status: SessionStatus,
    pub group_ids: Vec<String>,
}

impl Manifest {
    pub fn save(&self, paths: &Paths) -> Result<()> {
        let path = paths.manifest(self.session_id.as_str())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| Error::Json { path: path.clone(), source: e })?;
        std::fs::write(&path, json)
            .map_err(|e| Error::Io { path, source: e })
    }

    pub fn load(paths: &Paths, sid: &SessionId) -> Result<Self> {
        let path = paths.manifest(sid.as_str())?;
        let bytes = std::fs::read(&path)
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Json { path, source: e })
    }
}
