use crate::{Error, Paths, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a new session id: `sess-<16 hex clock>-<16 hex counter>`.
    ///
    /// Combines `SystemTime` (low 64 bits of nanos — wraps only after ~584
    /// years) with a full-width process-local atomic counter, guaranteeing
    /// different values for two `generate()` calls within the same process
    /// even if they happen within a single clock tick. No crypto guarantees;
    /// session ids are not secrets.
    pub fn generate() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("sess-{nanos:016x}-{c:016x}"))
    }

    /// Wrap a raw string as a `SessionId` without validating it.
    ///
    /// Validation of id components (no path separators, no `..`, etc.) happens
    /// at the boundary where an id is turned into a filesystem path — inside
    /// `Paths::session`, `Paths::manifest`, and friends. This constructor
    /// deliberately does NOT validate because it has no `Paths` context and
    /// must be cheap for deserialization paths. Callers accepting ids from
    /// untrusted sources must pass the resulting id through a `Paths` method
    /// before joining it to anything filesystem-backed.
    pub fn from_raw(s: &str) -> Self { Self(s.to_string()) }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
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
        crate::io_util::write_atomic(&path, &json)
    }

    pub fn load(paths: &Paths, sid: &SessionId) -> Result<Self> {
        let path = paths.manifest(sid.as_str())?;
        let bytes = std::fs::read(&path)
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Json { path: path.clone(), source: e })?;
        if manifest.session_id.as_str() != sid.as_str() {
            return Err(Error::Invalid(format!(
                "session id mismatch at {}: expected {:?}, on-disk id is {:?}",
                path.display(), sid.as_str(), manifest.session_id.as_str()
            )));
        }
        Ok(manifest)
    }
}
