use std::path::{Path, PathBuf};

/// Filesystem layout helper for `.dkod/`.
/// All paths are derived from the repo root passed to `::new`.
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(repo_root: &Path) -> Self {
        Self { root: repo_root.join(".dkod") }
    }

    pub fn root(&self) -> PathBuf { self.root.clone() }
    pub fn config(&self) -> PathBuf { self.root.join("config.toml") }
    pub fn sessions_dir(&self) -> PathBuf { self.root.join("sessions") }
    pub fn session(&self, sid: &str) -> PathBuf { self.sessions_dir().join(sid) }
    pub fn manifest(&self, sid: &str) -> PathBuf { self.session(sid).join("manifest.json") }
    pub fn groups_dir(&self, sid: &str) -> PathBuf { self.session(sid).join("groups") }
    pub fn group(&self, sid: &str, gid: &str) -> PathBuf { self.groups_dir(sid).join(gid) }
    pub fn group_spec(&self, sid: &str, gid: &str) -> PathBuf { self.group(sid, gid).join("spec.json") }
    pub fn group_writes(&self, sid: &str, gid: &str) -> PathBuf { self.group(sid, gid).join("writes.jsonl") }
    pub fn conflicts_dir(&self, sid: &str) -> PathBuf { self.session(sid).join("conflicts") }
}
