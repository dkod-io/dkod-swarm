//! Pure helper for the `dkod_pr` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this helper lives in `tools/mod.rs`
//! per the project's code-placement convention. This file holds the
//! synchronous control-flow that turns a finalised dk-branch into a real
//! GitHub pull request.
//!
//! Behaviour summary (tasks 20–23 of the M2 plan):
//! 1. Acquire the active session id briefly, then drop the lock.
//! 2. Compute `dk/<sid>` via `dkod_worktree::branch::dk_branch_name`.
//! 3. If `Config::verify_cmd` is configured, run it via `sh -c` in the repo
//!    root. A non-zero exit short-circuits the whole flow with
//!    [`Error::VerifyFailed`] — no push, no PR, no manifest mutation.
//! 4. **Idempotency check 1** — `gh pr list --head dk/<sid>`. If a URL comes
//!    back, return `PrResponse { url, was_existing: true }` without pushing.
//!    This is what makes calling `dkod_pr` twice harmless.
//! 5. Push: `git push --force-with-lease --set-upstream origin dk/<sid>`.
//! 6. **Idempotency check 2** — re-query `gh pr list`. Catches the race in
//!    which a concurrent process opened a PR for the same branch in the
//!    window between our first check and the push.
//! 7. Create: `gh pr create --head dk/<sid> --title … --body …`. Return the
//!    URL with `was_existing: false`.
//!
//! `pr_with_shim` accepts an optional `path_prefix: &Path` so integration
//! tests can prepend a directory containing a shimmed `gh` (and/or `git`) to
//! `PATH` for the duration of the helper. Production callers go through
//! [`pr`], which always passes `None`.

use crate::gh;
use crate::schema::{PrRequest, PrResponse};
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{Config, Paths, SessionId, branch};
use std::path::Path;
use std::process::Command;

/// Production entry point — equivalent to `pr_with_shim(ctx, req, None)`.
pub async fn pr(ctx: &ServerCtx, req: PrRequest) -> Result<PrResponse> {
    pr_with_shim(ctx, req, None).await
}

/// Test-friendly variant of [`pr`] that accepts a `path_prefix` directory to
/// prepend to `PATH` for every `gh` / `git push` invocation made during this
/// call. Pass `None` in production. Tests typically point this at a tempdir
/// containing a tiny `gh` shell script that mimics `gh pr list` / `gh pr
/// create` without hitting GitHub.
pub async fn pr_with_shim(
    ctx: &ServerCtx,
    req: PrRequest,
    path_prefix: Option<&Path>,
) -> Result<PrResponse> {
    // Capture the session id on the async path. The subprocess work
    // (verify_cmd, gh, git push) all runs synchronously on a blocking
    // thread to keep the tokio executor free, mirroring `dkod_commit`.
    let sid = ctx
        .active_session
        .lock()
        .await
        .clone()
        .ok_or(Error::NoActiveSession)?;
    let repo_root = ctx.repo_root.clone();
    let path_prefix_buf = path_prefix.map(Path::to_path_buf);
    let sid_for_clear = sid.clone();

    // `Paths` (M1 type, not `Clone`) is reconstructed inside the blocking
    // closure from `repo_root`. `ServerCtx::new` constructs it the same
    // way, so the value is identical.
    let resp = tokio::task::spawn_blocking(move || {
        let paths = Paths::new(&repo_root);
        pr_inner(&repo_root, &paths, sid, req, path_prefix_buf.as_deref())
    })
    .await
    .map_err(|e| Error::InvalidArg(format!("spawn_blocking join error: {e}")))??;

    // Clear the active session on success — `ServerCtx::active_session`
    // documents that a successful `dkod_pr` ends the session lifecycle.
    // Use a compare-then-clear pattern so a concurrent abort that already
    // started a different session can't be wiped out by us.
    let mut active = ctx.active_session.lock().await;
    if active.as_ref().map(|s| s.as_str()) == Some(sid_for_clear.as_str()) {
        *active = None;
    }

    Ok(resp)
}

/// Synchronous core of `dkod_pr`. The async wrappers above offload this
/// to `tokio::task::spawn_blocking`, matching the M2-6 `dkod_commit`
/// split (sync inner + async wrapper). Subprocess calls would otherwise
/// pin the tokio executor thread for as long as `verify_cmd` / `gh` /
/// `git push` take.
pub(crate) fn pr_inner(
    repo_root: &Path,
    paths: &Paths,
    sid: SessionId,
    req: PrRequest,
    path_prefix: Option<&Path>,
) -> Result<PrResponse> {
    let branch_name = branch::dk_branch_name(sid.as_str());

    // 1. Run verify_cmd if configured. We tolerate a missing config file
    //    (init_repo always writes one in production, but some tests skip it)
    //    by treating "no config" as "no verify_cmd". Any *other* config
    //    error propagates normally.
    let config_path = paths.config();
    match Config::load(&config_path) {
        Ok(cfg) => {
            if let Some(cmd) = cfg.verify_cmd.as_deref() {
                run_verify(repo_root, cmd)?;
            }
        }
        Err(dkod_worktree::Error::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::from(e)),
    }

    // 2. Idempotency check BEFORE pushing. Cheap fast-path when a previous
    //    run already opened the PR.
    if let Some(url) = gh::pr_exists(repo_root, &branch_name, path_prefix)? {
        return Ok(PrResponse {
            url,
            was_existing: true,
        });
    }

    // 3. Push the dk-branch to origin.
    gh::push_branch(repo_root, &branch_name, path_prefix)?;

    // 4. Re-check after push to catch the race window where another caller
    //    already created the PR. (Cheap one extra RPC; saves us from
    //    duplicate PRs which `gh pr create` would otherwise refuse with a
    //    cryptic error.)
    if let Some(url) = gh::pr_exists(repo_root, &branch_name, path_prefix)? {
        return Ok(PrResponse {
            url,
            was_existing: true,
        });
    }

    // 5. Open the PR.
    let url = gh::create_pr(repo_root, &branch_name, &req.title, &req.body, path_prefix)?;
    Ok(PrResponse {
        url,
        was_existing: false,
    })
}

/// Run `verify_cmd` via `sh -c <cmd>` in the repo root. On a non-zero exit,
/// surface [`Error::VerifyFailed`] with the last 10 stderr lines (matches the
/// M2 plan's contract). On a successful exit, drop the captured stderr — we
/// don't surface verify output on the happy path.
fn run_verify(repo: &Path, cmd: &str) -> Result<()> {
    let out = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(repo)
        .output()
        .map_err(Error::Io)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // Keep only the last 10 lines so a noisy `cargo test` failure
        // doesn't drown the JSON-RPC payload. Naive but correct: collect
        // forward, then take the tail.
        let lines: Vec<&str> = stderr.lines().collect();
        let start = lines.len().saturating_sub(10);
        let tail = lines[start..].join("\n");
        return Err(Error::VerifyFailed {
            exit: out.status.code().unwrap_or(-1),
            tail,
        });
    }
    Ok(())
}
