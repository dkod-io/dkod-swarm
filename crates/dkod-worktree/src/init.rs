use crate::Result;
use std::path::Path;

/// Scaffold `.dkod/` for a repo. Idempotent: if `config.toml` already exists,
/// it is left untouched even if the caller passes a different `verify_cmd`.
///
/// Creates:
/// - `.dkod/`
/// - `.dkod/sessions/`
/// - `.dkod/config.toml` (only if absent — see idempotency guarantee above)
pub fn init_repo(repo_root: &Path, verify_cmd: Option<String>) -> Result<()> {
    use crate::{branch, Config, Error, Paths};

    if !repo_root.exists() {
        return Err(Error::Invalid(format!(
            "repo root does not exist: {}",
            repo_root.display()
        )));
    }
    let paths = Paths::new(repo_root);
    std::fs::create_dir_all(paths.sessions_dir())
        .map_err(|e| Error::Io { path: paths.sessions_dir(), source: e })?;

    // Idempotent: do not overwrite an existing config, even if verify_cmd differs.
    if paths.config().exists() {
        return Ok(());
    }

    let main_branch = branch::detect_main(repo_root)?;
    let cfg = Config { main_branch, verify_cmd };
    cfg.save(&paths.config())?;
    Ok(())
}
