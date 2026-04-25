//! Subprocess helpers for `dkod_pr`.
//!
//! Three primitives:
//! - [`pr_exists`] — `gh pr list --head <branch> --state all --json url --jq …`
//!   returning `Some(url)` when a PR with that head already exists, `None`
//!   otherwise. Used twice by `dkod_pr` for idempotency: once before pushing
//!   (cheap short-circuit when a PR is already open) and once between push
//!   and create (catches a race where another process opened the PR in the
//!   gap).
//! - [`push_branch`] — `git push --force-with-lease --set-upstream origin
//!   <branch>`. We force-with-lease (rather than vanilla `--force`) so a
//!   concurrent commit on the remote dk-branch from another orchestrator
//!   makes the push fail loudly, instead of silently overwriting work.
//! - [`create_pr`] — `gh pr create --head … --title … --body …`. Returns the
//!   URL `gh` prints to stdout.
//!
//! Every helper accepts an optional `path_prefix: Option<&Path>` so tests can
//! inject a directory containing a shimmed `gh` (and/or `git`) ahead of the
//! real one on `PATH`. Production callers pass `None`. The mechanism prepends
//! `path_prefix` to the inherited `PATH` for that single subprocess only — no
//! global `set_var` mutation, so concurrent tests cannot stomp on each other.
//!
//! Errors from `gh` invocations wrap into `Error::Gh { cmd, stderr }`. Errors
//! from `git push` wrap into `Error::Git { cmd, stderr }` per M2-6's split.

use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Prepend `prefix` to the current process `PATH` value, returning the
/// `OsString` to pass via `Command::env("PATH", …)`. Pulled out so both the
/// `gh` and `git push` helpers share one definition of "shim".
fn path_with_prefix(prefix: &Path) -> std::ffi::OsString {
    let mut combined: std::ffi::OsString = PathBuf::from(prefix).into_os_string();
    if let Some(cur) = std::env::var_os("PATH") {
        // Use the platform path separator so this still works if the helper
        // is ever exercised on a non-Unix CI runner. macOS + Linux use ':',
        // which is what we need today.
        combined.push(std::ffi::OsString::from(if cfg!(windows) {
            ";"
        } else {
            ":"
        }));
        combined.push(cur);
    }
    combined
}

/// Run `gh <args>` in `repo`, optionally prepending `path_prefix` to `PATH`.
/// Returns trimmed stdout on success, [`Error::Gh`] otherwise.
fn gh(repo: &Path, args: &[&str], path_prefix: Option<&Path>) -> Result<String> {
    let cmd_label = format!("gh {}", args.join(" "));
    let mut cmd = Command::new("gh");
    cmd.args(args).current_dir(repo);
    if let Some(prefix) = path_prefix {
        cmd.env("PATH", path_with_prefix(prefix));
    }
    let out = cmd.output().map_err(|e| Error::Gh {
        cmd: cmd_label.clone(),
        stderr: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(Error::Gh {
            cmd: cmd_label,
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Return the URL of an existing PR whose head is `branch`, or `None` when
/// no PR exists. The `--state all` flag intentionally matches *closed* PRs
/// too: if a previous run already opened and merged a PR for this dk-branch,
/// re-running `dkod_pr` should hand the caller back that URL rather than
/// open a duplicate.
pub fn pr_exists(repo: &Path, branch: &str, path_prefix: Option<&Path>) -> Result<Option<String>> {
    let out = gh(
        repo,
        &[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "all",
            "--json",
            "url",
            "--jq",
            ".[0].url // empty",
        ],
        path_prefix,
    )?;
    if out.is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

/// Push `branch` to `origin` with `--force-with-lease` + `--set-upstream`.
/// Errors flow through [`Error::Git`] (per M2-6) — `git push` is plain git,
/// not the `gh` CLI.
pub fn push_branch(repo: &Path, branch: &str, path_prefix: Option<&Path>) -> Result<()> {
    let cmd_label = format!("git push --force-with-lease --set-upstream origin {branch}");
    let mut cmd = Command::new("git");
    cmd.args([
        "push",
        "--force-with-lease",
        "--set-upstream",
        "origin",
        branch,
    ])
    .current_dir(repo);
    if let Some(prefix) = path_prefix {
        cmd.env("PATH", path_with_prefix(prefix));
    }
    let out = cmd.output().map_err(|e| Error::Git {
        cmd: cmd_label.clone(),
        stderr: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(Error::Git {
            cmd: cmd_label,
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(())
}

/// Create a PR with `gh pr create` and return its URL (the value `gh` prints
/// to stdout on success).
///
/// `base` is passed to `gh` via `--base <name>` and overrides GitHub's
/// repo-level default. Pass `None` to use whatever `gh` infers from the
/// remote (the historical behaviour); production callers should pass
/// `Some(&config.main_branch)` so the PR targets the same branch
/// `init_repo` recorded — defending against the case where a repo's
/// configured default on GitHub drifts from the local `main_branch`
/// (renames, fork mismatch, etc.).
pub fn create_pr(
    repo: &Path,
    branch: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
    path_prefix: Option<&Path>,
) -> Result<String> {
    let mut args: Vec<&str> = vec![
        "pr", "create", "--head", branch, "--title", title, "--body", body,
    ];
    if let Some(b) = base {
        args.push("--base");
        args.push(b);
    }
    gh(repo, &args, path_prefix)
}
