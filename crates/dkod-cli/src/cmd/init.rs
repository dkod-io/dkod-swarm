use std::path::Path;

/// Initialise `.dkod/` under `repo_root`. Idempotent — leaves an existing
/// `config.toml` untouched even if `verify_cmd` differs.
pub fn run(repo_root: &Path, verify_cmd: Option<String>) -> anyhow::Result<()> {
    dkod_worktree::init_repo(repo_root, verify_cmd)
        .map_err(|e| anyhow::anyhow!("dkod_worktree::init_repo failed: {e}"))?;
    println!("Initialised .dkod/ in {}", repo_root.display());
    Ok(())
}
