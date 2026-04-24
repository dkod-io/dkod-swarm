use crate::{Error, Result};
use std::path::Path;
use std::process::Command;

const AUTHOR_NAME: &str = "Haim Ari";
const AUTHOR_EMAIL: &str = "haimari1@gmail.com";

/// Return the `dk/<session_id>` branch name for a given session id.
pub fn dk_branch_name(session_id: &str) -> String {
    format!("dk/{session_id}")
}

/// Run a git command in `repo`, returning trimmed stdout on success or
/// `Error::Git` on non-zero exit / spawn failure.
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| Error::Git {
            cmd: format!("git {}", args.join(" ")),
            stderr: e.to_string(),
        })?;
    if !out.status.success() {
        return Err(Error::Git {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run a git command with the enforced author/committer identity set via env
/// vars. Identity env vars override any value already in the environment or
/// the repo-local git config.
fn git_with_identity(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", AUTHOR_NAME)
        .env("GIT_AUTHOR_EMAIL", AUTHOR_EMAIL)
        .env("GIT_COMMITTER_NAME", AUTHOR_NAME)
        .env("GIT_COMMITTER_EMAIL", AUTHOR_EMAIL)
        .output()
        .map_err(|e| Error::Git {
            cmd: format!("git {}", args.join(" ")),
            stderr: e.to_string(),
        })?;
    if !out.status.success() {
        return Err(Error::Git {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Detect the repo's default branch using a three-tier fallback:
///
/// 1. The current symbolic HEAD — if HEAD is on a named branch, that's it.
/// 2. `refs/remotes/origin/HEAD` — set automatically by `git clone`/`git remote
///    set-head`. Strips the `origin/` prefix.
/// 3. The literal string `"main"` — safe last-resort for bare or freshly-init
///    repos where neither of the above is available (e.g. CI checkout in
///    detached-HEAD mode without a remote).
pub fn detect_main(repo: &Path) -> Result<String> {
    // Tier 1: symbolic HEAD → named branch.
    if let Ok(head) = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        && head != "HEAD"
    {
        return Ok(head);
    }
    // Tier 2: origin/HEAD.
    if let Ok(sym) = git(repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        && let Some(stripped) = sym.strip_prefix("origin/")
    {
        return Ok(stripped.to_string());
    }
    // Tier 3: literal fallback.
    Ok("main".to_string())
}

/// Create `dk/<session_id>` off `main` and check it out.
pub fn create_dk_branch(repo: &Path, main: &str, session_id: &str) -> Result<()> {
    let name = dk_branch_name(session_id);
    git(repo, &["checkout", "-b", &name, main])?;
    Ok(())
}

/// Check out `main` and delete `dk/<session_id>` with `-D` (force-delete, as
/// dk-branches are ephemeral and may not be merged into the default branch).
pub fn destroy_dk_branch(repo: &Path, main: &str, session_id: &str) -> Result<()> {
    let name = dk_branch_name(session_id);
    // Move off the dk-branch before deleting it.
    git(repo, &["checkout", main])?;
    // `-D` is correct here: aborting a session leaves an unmerged branch by
    // design, and `-d` would refuse to delete it.
    git(repo, &["branch", "-D", &name])?;
    Ok(())
}

/// Stage `paths` with `git add -- <paths>` and produce a single commit on the
/// current branch using the enforced author/committer identity.
///
/// Using `--` separates the path operands from git flags, preventing
/// filenames that start with `-` from being interpreted as options.
///
/// Security note: `paths` are passed directly to `git add` via
/// `std::process::Command`; no shell interpolation occurs, so shell-injection
/// is not possible. Callers are still responsible for supplying paths that
/// belong to the worktree.
pub fn commit_paths(repo: &Path, paths: &[&Path], msg: &str) -> Result<()> {
    // Build the `git add -- <path> ...` arg list.
    let mut add_args: Vec<&str> = vec!["add", "--"];
    // Collect owned strings for paths that need temporary storage.
    let path_strs: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    for s in &path_strs {
        add_args.push(s.as_str());
    }
    git_with_identity(repo, &add_args)?;
    git_with_identity(repo, &["commit", "-m", msg])?;
    Ok(())
}
