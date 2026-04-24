use crate::{Error, Result};
use std::path::{Component, Path, PathBuf};

/// Filesystem layout helper for `.dkod/`.
/// All paths are derived from the repo root passed to `::new`.
pub struct Paths {
    root: PathBuf,
}

/// Validates that `id` is a single, safe path component.
///
/// Rejects absolute paths, path separators, `..` traversals, and any string
/// whose only component is not a plain `Normal` segment.
pub(crate) fn validate_id(id: &str) -> Result<()> {
    let mut components = Path::new(id).components();
    match components.next() {
        Some(Component::Normal(_)) => {}
        _ => return Err(Error::InvalidComponent(id.to_owned())),
    }
    // There must be exactly one component (no separators, no trailing slash).
    if components.next().is_some() {
        return Err(Error::InvalidComponent(id.to_owned()));
    }
    Ok(())
}

impl Paths {
    pub fn new(repo_root: &Path) -> Self {
        Self { root: repo_root.join(".dkod") }
    }

    pub fn root(&self) -> PathBuf { self.root.clone() }
    pub fn config(&self) -> PathBuf { self.root.join("config.toml") }
    pub fn sessions_dir(&self) -> PathBuf { self.root.join("sessions") }

    pub fn session(&self, sid: &str) -> Result<PathBuf> {
        validate_id(sid)?;
        Ok(self.sessions_dir().join(sid))
    }

    pub fn manifest(&self, sid: &str) -> Result<PathBuf> {
        Ok(self.session(sid)?.join("manifest.json"))
    }

    pub fn groups_dir(&self, sid: &str) -> Result<PathBuf> {
        Ok(self.session(sid)?.join("groups"))
    }

    pub fn group(&self, sid: &str, gid: &str) -> Result<PathBuf> {
        validate_id(gid)?;
        Ok(self.groups_dir(sid)?.join(gid))
    }

    pub fn group_spec(&self, sid: &str, gid: &str) -> Result<PathBuf> {
        Ok(self.group(sid, gid)?.join("spec.json"))
    }

    pub fn group_writes(&self, sid: &str, gid: &str) -> Result<PathBuf> {
        Ok(self.group(sid, gid)?.join("writes.jsonl"))
    }

    pub fn conflicts_dir(&self, sid: &str) -> Result<PathBuf> {
        Ok(self.session(sid)?.join("conflicts"))
    }
}
