//! Shared path-escape guard for MCP tool inputs.
//!
//! Two tools (`dkod_plan` and `dkod_write_symbol`, with more to come) accept
//! caller-supplied repo-relative paths and need to refuse anything that
//! escapes the repo. The check defends against three malicious shapes:
//!
//! 1. Absolute paths (`/etc/passwd`).
//! 2. `..` traversal that climbs above the repo root.
//! 3. Symlinks pointing outside the repo (caught by `canonicalize`).
//!
//! Centralising this means a future tool (e.g. `dkod_read_file`) gets the
//! same guard for free instead of re-implementing it.
//!
//! The caller is expected to pass an already-canonicalised `canonical_repo`
//! so we do not redo `realpath()` on the repo root for every file in a
//! batched request — `dkod_plan` calls this in a loop over `req.files`.

use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Canonicalise `rel` against `canonical_repo` (which must itself already be
/// canonicalised) and reject anything that escapes the repo.
pub fn resolve_under_repo(canonical_repo: &Path, rel: &Path) -> Result<PathBuf> {
    if rel.is_absolute() {
        return Err(Error::InvalidArg(format!(
            "path must be relative to the repo root, got absolute: {}",
            rel.display()
        )));
    }
    let canonical_target = std::fs::canonicalize(canonical_repo.join(rel)).map_err(|e| {
        Error::InvalidArg(format!(
            "cannot resolve {} under repo root: {e}",
            rel.display()
        ))
    })?;
    if !canonical_target.starts_with(canonical_repo) {
        return Err(Error::InvalidArg(format!(
            "path escapes repo root: {} resolves to {}",
            rel.display(),
            canonical_target.display()
        )));
    }
    Ok(canonical_target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    /// Helper: create a tempdir, canonicalise its path so symlinked /var
    /// → /private/var on macOS does not break `starts_with` checks downstream.
    fn canonical_tempdir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let canon = std::fs::canonicalize(tmp.path()).expect("canonicalize tempdir");
        (tmp, canon)
    }

    #[test]
    fn resolves_valid_relative_path() {
        let (_tmp, repo) = canonical_tempdir();
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        std::fs::write(repo.join("sub/file.rs"), "fn x() {}").unwrap();

        let resolved = resolve_under_repo(&repo, Path::new("sub/file.rs")).expect("resolves");
        assert_eq!(resolved, repo.join("sub/file.rs"));
    }

    #[test]
    fn rejects_absolute_path() {
        let (_tmp, repo) = canonical_tempdir();
        let err = resolve_under_repo(&repo, Path::new("/etc/passwd"))
            .expect_err("absolute path must be rejected");
        assert!(
            matches!(err, Error::InvalidArg(ref m) if m.contains("absolute")),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn rejects_parent_traversal_escaping_repo() {
        // Set up an outer dir with a "secret" file and a nested repo that
        // would have to climb `..` to reach it.
        let (_outer, outer_canon) = canonical_tempdir();
        std::fs::write(outer_canon.join("secret.rs"), "leaked").unwrap();
        let repo = outer_canon.join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let err = resolve_under_repo(&repo, Path::new("../secret.rs"))
            .expect_err("escape must be rejected");
        assert!(
            matches!(err, Error::InvalidArg(ref m) if m.contains("escapes repo root")),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn rejects_nonexistent_path() {
        // canonicalize() fails for a path that does not exist; resolve_under_repo
        // surfaces that as InvalidArg rather than letting an io::Error leak
        // through. The error message comes from the canonicalize branch.
        let (_tmp, repo) = canonical_tempdir();
        let err = resolve_under_repo(&repo, Path::new("does/not/exist.rs"))
            .expect_err("non-existent path must error");
        assert!(
            matches!(err, Error::InvalidArg(ref m) if m.contains("cannot resolve")),
            "unexpected error: {err:?}"
        );
    }
}
