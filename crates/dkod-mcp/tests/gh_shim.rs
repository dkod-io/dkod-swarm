//! Direct tests for `dkod_mcp::gh` using a `PATH`-shimmed `gh`.
//!
//! The shim is a tiny shell script written into a temp `bin/` directory; we
//! pass that directory as `path_prefix` to the helpers so the production
//! `gh` (if installed) is shadowed for the duration of the test only — no
//! global `set_var` mutation, so concurrent tests cannot collide.

use std::path::{Path, PathBuf};

/// Drop a `gh` shell script under `<tmp>/bin/` and return that directory.
/// `body` is interpolated verbatim into the script body.
fn make_shim(tmp: &Path, body: &str) -> PathBuf {
    let bin_dir = tmp.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("gh");
    std::fs::write(&shim, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&shim).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&shim, perm).unwrap();
    }
    bin_dir
}

#[test]
fn pr_exists_via_shim_returns_url() {
    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = make_shim(tmp.path(), r#"echo "https://github.com/x/y/pull/42""#);
    let repo = tempfile::tempdir().unwrap();
    let url = dkod_mcp::gh::pr_exists(repo.path(), "dk/x", Some(&bin_dir)).unwrap();
    assert_eq!(url.as_deref(), Some("https://github.com/x/y/pull/42"));
}

#[test]
fn pr_exists_via_shim_returns_none() {
    let tmp = tempfile::tempdir().unwrap();
    // Shim emits empty stdout — same as `gh pr list … --jq '.[0].url // empty'`
    // would for a branch that has no PR open.
    let bin_dir = make_shim(tmp.path(), "");
    let repo = tempfile::tempdir().unwrap();
    let url = dkod_mcp::gh::pr_exists(repo.path(), "dk/x", Some(&bin_dir)).unwrap();
    assert_eq!(url, None);
}

#[test]
fn pr_exists_via_shim_propagates_error_status() {
    // A non-zero exit from `gh` should surface as `Error::Gh` rather than
    // being silently treated as `None`.
    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = make_shim(tmp.path(), "echo 'no auth token' >&2\nexit 1");
    let repo = tempfile::tempdir().unwrap();
    let err = dkod_mcp::gh::pr_exists(repo.path(), "dk/x", Some(&bin_dir)).unwrap_err();
    assert!(
        matches!(err, dkod_mcp::Error::Gh { .. }),
        "expected Error::Gh, got {err:?}"
    );
}

#[test]
fn create_pr_via_shim_returns_url() {
    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = make_shim(tmp.path(), r#"echo "https://github.com/x/y/pull/7""#);
    let repo = tempfile::tempdir().unwrap();
    let url =
        dkod_mcp::gh::create_pr(repo.path(), "dk/x", "title", "body", Some(&bin_dir)).unwrap();
    assert_eq!(url, "https://github.com/x/y/pull/7");
}
