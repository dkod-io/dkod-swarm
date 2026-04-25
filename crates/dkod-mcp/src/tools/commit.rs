//! Pure helper for the `dkod_commit` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this helper lives in `tools/mod.rs`
//! per the project's code-placement convention. `dkod_commit` is the M2
//! piece that turns each group's `writes.jsonl` into a real commit on the
//! dk-branch, leveraging the M1 `dkod_orchestrator::commit::commit_per_group`
//! routine for the actual git work.
//!
//! Behaviour summary:
//! - Acquires `active_session` briefly to copy the session id, then drops
//!   the lock — the rest of the helper does not touch session state.
//! - Loads the manifest to enumerate `group_ids` in their canonical order
//!   (so the resulting `git log` mirrors that order).
//! - Captures `git rev-parse HEAD` before and after the commit pass to
//!   compute the new SHAs without inspecting `commit_per_group`'s internals.
//! - When every group is empty, `before == after`, no commits exist, and
//!   we return `commits_created: 0` with an empty SHA list.
//! - On success, transitions the manifest to `SessionStatus::Committed`.
//!
//! Error mapping (`Error::from`) is identical to the rest of the M2 tool
//! surface; see `crate::error` for the variant → JSON-RPC code table.

use crate::schema::CommitResponse;
use crate::{Error, Result, ServerCtx};
use dkod_orchestrator::commit::commit_per_group;
use dkod_worktree::{Manifest, Paths, SessionId, SessionStatus, branch};
use std::path::Path;
use std::process::Command;

pub async fn commit(ctx: &ServerCtx) -> Result<CommitResponse> {
    // Mirror the other tools: hold the active-session lock only long enough
    // to clone the id. The git work below can take seconds on a real-world
    // commit, and we don't want to serialise unrelated read-only tools (e.g.
    // `dkod_status`) behind it.
    let sid = ctx
        .active_session
        .lock()
        .await
        .clone()
        .ok_or(Error::NoActiveSession)?;
    commit_inner(&ctx.repo_root, &ctx.paths, sid)
}

/// Synchronous core of `dkod_commit`. The MCP wrapper in `tools/mod.rs`
/// drives this through `tokio::task::spawn_blocking` because every step
/// shells out to git — `rev-parse`, `rev-list`, plus the per-group
/// `git add` + `git commit` chain `commit_per_group` runs internally.
/// Keeping the body sync (rather than `async`) matches the M2-2 plan
/// helper pattern and avoids the `spawn_blocking` + nested `block_on`
/// anti-pattern.
pub(crate) fn commit_inner(
    repo_root: &Path,
    paths: &Paths,
    sid: SessionId,
) -> Result<CommitResponse> {
    let manifest = Manifest::load(paths, &sid)?;
    // The before/after SHA pair assumes no other process commits on the
    // dk-branch worktree between our two `rev-parse` calls. In M2 the
    // single MCP-stdio caller is the only writer of this worktree; if the
    // surface ever multiplexes, the SHA delta below would silently
    // include foreign commits.
    let before = git_head_sha(repo_root)?;
    commit_per_group(repo_root, paths, &sid, &manifest.group_ids).map_err(Error::from)?;
    let after = git_head_sha(repo_root)?;

    // Collect every new SHA between `before` (exclusive) and `after`
    // (inclusive). When `before == after`, every group's `writes.jsonl` was
    // empty (commit_per_group skips empty groups silently) and no commits
    // were produced — return an empty list rather than running git rev-list,
    // which would error on an empty range.
    let shas = if before == after {
        Vec::new()
    } else {
        git_rev_list(repo_root, &format!("{before}..{after}"))?
    };

    // Mark the session Committed on disk. Reload the manifest rather than
    // mutating the copy above so we never overwrite a concurrent change to
    // any other field; in practice no other tool mutates the manifest after
    // execute_begin, but the reload-then-write pattern is cheap and matches
    // the rest of the M2 surface.
    let mut m = Manifest::load(paths, &sid)?;
    m.status = SessionStatus::Committed;
    m.save(paths)?;

    Ok(CommitResponse {
        commits_created: shas.len(),
        dk_branch: branch::dk_branch_name(sid.as_str()),
        commit_shas: shas,
    })
}

fn git_head_sha(repo: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .map_err(Error::Io)?;
    if !out.status.success() {
        return Err(Error::Git {
            cmd: "git rev-parse HEAD".into(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_rev_list(repo: &Path, range: &str) -> Result<Vec<String>> {
    let out = Command::new("git")
        .args(["rev-list", "--reverse", "--abbrev-commit", range])
        .current_dir(repo)
        .output()
        .map_err(Error::Io)?;
    if !out.status.success() {
        return Err(Error::Git {
            cmd: format!("git rev-list {range}"),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}
