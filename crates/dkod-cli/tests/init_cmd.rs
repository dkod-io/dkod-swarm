use std::path::PathBuf;

#[test]
fn init_creates_dkod_dir_with_config() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    // Init a git repo so `init_repo`'s `branch::detect_main` succeeds.
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());

    dkod_cli::cmd::init::run(&root, Some("cargo test".into())).expect("init::run");

    let dkod_dir = root.join(".dkod");
    assert!(dkod_dir.is_dir(), ".dkod/ should exist");
    let cfg: PathBuf = dkod_dir.join("config.toml");
    assert!(cfg.is_file(), ".dkod/config.toml should exist");
    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(body.contains("main_branch"));
    assert!(body.contains("verify_cmd"));
    assert!(body.contains("cargo test"));
    assert!(dkod_dir.join("sessions").is_dir());
}

#[test]
fn init_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());

    dkod_cli::cmd::init::run(&root, None).unwrap();
    // First run wrote no verify_cmd — second run with a different value
    // must NOT overwrite it (per `init_repo`'s idempotency contract).
    dkod_cli::cmd::init::run(&root, Some("ignored".into())).unwrap();
    let body = std::fs::read_to_string(root.join(".dkod/config.toml")).unwrap();
    assert!(
        !body.contains("ignored"),
        "init must not overwrite an existing config"
    );
}
