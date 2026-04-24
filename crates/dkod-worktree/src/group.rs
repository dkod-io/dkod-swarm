use crate::{Error, Paths, Result, SessionId};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    pub qualified_name: String,
    pub file_path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupStatus {
    Pending,
    InProgress,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSpec {
    pub id: String,
    pub symbols: Vec<SymbolRef>,
    pub agent_prompt: String,
    pub status: GroupStatus,
}

impl GroupSpec {
    pub fn save(&self, paths: &Paths, sid: &SessionId) -> Result<()> {
        let path = paths.group_spec(sid.as_str(), &self.id)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| Error::Json { path: path.clone(), source: e })?;
        std::fs::write(&path, json)
            .map_err(|e| Error::Io { path, source: e })
    }

    pub fn load(paths: &Paths, sid: &SessionId, gid: &str) -> Result<Self> {
        let path = paths.group_spec(sid.as_str(), gid)?;
        let bytes = std::fs::read(&path)
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        let spec: Self = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Json { path: path.clone(), source: e })?;
        if spec.id != gid {
            return Err(Error::Invalid(format!(
                "group id mismatch at {}: expected {gid:?}, on-disk id is {:?}",
                path.display(), spec.id
            )));
        }
        Ok(spec)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteRecord {
    pub symbol: String,
    pub file_path: PathBuf,
    pub timestamp: String,
}

/// Append-only JSONL log of agent symbol writes.
pub struct WriteLog {
    path: PathBuf,
}

impl WriteLog {
    pub fn open(paths: &Paths, sid: &SessionId, gid: &str) -> Result<Self> {
        let path = paths.group_writes(sid.as_str(), gid)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        Ok(Self { path })
    }

    pub fn append(&self, rec: &WriteRecord) -> Result<()> {
        let line = serde_json::to_string(rec)
            .map_err(|e| Error::Json { path: self.path.clone(), source: e })?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| Error::Io { path: self.path.clone(), source: e })?;
        writeln!(f, "{line}")
            .map_err(|e| Error::Io { path: self.path.clone(), source: e })
    }

    pub fn read_all(&self) -> Result<Vec<WriteRecord>> {
        let f = match std::fs::File::open(&self.path) {
            Ok(f) => f,
            // A never-written log is semantically empty — callers resuming
            // a session should not care whether `append` ever ran.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::Io { path: self.path.clone(), source: e }),
        };
        let mut out = Vec::new();
        for line in BufReader::new(f).lines() {
            let line = line.map_err(|e| Error::Io { path: self.path.clone(), source: e })?;
            if line.trim().is_empty() { continue; }
            let rec: WriteRecord = serde_json::from_str(&line)
                .map_err(|e| Error::Json { path: self.path.clone(), source: e })?;
            out.push(rec);
        }
        Ok(out)
    }
}
