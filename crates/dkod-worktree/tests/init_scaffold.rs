use dkod_worktree::{init_repo, Config, Paths};
use std::process::Command;
use tempfile::TempDir;

fn init_git(dir: &std::path::Path) {
    let status = Command::new("git").args(["init", "-b", "main"]).current_dir(dir).status().unwrap();
    assert!(status.success(), "git init failed");
    std::fs::write(dir.join("README.md"), "hi").unwrap();
    let status = Command::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    assert!(status.success(), "git add failed");
    let status = Command::new("git")
        .args(["commit", "-m", "init"])
        .env("GIT_AUTHOR_NAME", "Haim Ari")
        .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
        .env("GIT_COMMITTER_NAME", "Haim Ari")
        .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git commit failed");
}

#[test]
fn init_scaffolds_dkod_dir_and_detects_main() {
    let tmp = TempDir::new().unwrap();
    init_git(tmp.path());

    init_repo(tmp.path(), None).unwrap();

    let paths = Paths::new(tmp.path());
    assert!(paths.root().is_dir(), ".dkod/ not created");
    assert!(paths.sessions_dir().is_dir(), ".dkod/sessions/ not created");
    assert!(paths.config().is_file(), ".dkod/config.toml not created");

    let cfg = Config::load(&paths.config()).unwrap();
    assert_eq!(cfg.main_branch, "main");
    assert!(cfg.verify_cmd.is_none());
}

#[test]
fn init_is_idempotent_and_preserves_user_verify_cmd() {
    let tmp = TempDir::new().unwrap();
    init_git(tmp.path());

    init_repo(tmp.path(), Some("cargo check".into())).unwrap();
    // Second run must not overwrite existing config.
    init_repo(tmp.path(), Some("different".into())).unwrap();

    let paths = Paths::new(tmp.path());
    let cfg = Config::load(&paths.config()).unwrap();
    assert_eq!(cfg.verify_cmd.as_deref(), Some("cargo check"));
}
