use dkod_worktree::{Config, Paths};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn config_roundtrips_through_disk() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    std::fs::create_dir_all(paths.root()).unwrap();

    let cfg = Config {
        main_branch: "main".into(),
        verify_cmd: Some("cargo check && cargo test --workspace".into()),
    };
    cfg.save(&paths.config()).unwrap();

    let loaded = Config::load(&paths.config()).unwrap();
    assert_eq!(loaded.main_branch, "main");
    assert_eq!(loaded.verify_cmd.as_deref(), Some("cargo check && cargo test --workspace"));
}

#[test]
fn config_defaults_when_verify_absent() {
    let tmp = TempDir::new().unwrap();
    let cfg_path = tmp.path().join("config.toml");
    std::fs::write(&cfg_path, "main_branch = \"trunk\"\n").unwrap();

    let loaded = Config::load(&cfg_path).unwrap();
    assert_eq!(loaded.main_branch, "trunk");
    assert!(loaded.verify_cmd.is_none());
}

#[test]
fn config_saves_cleanly_to_plain_filename_with_no_parent() {
    let tmp = TempDir::new().unwrap();
    // Use an absolute path inside the tempdir to avoid any CWD dependency.
    let cfg_path = tmp.path().join("config.toml");
    let cfg = Config { main_branch: "main".into(), verify_cmd: None };
    // Path::parent() of a single-component path (no directory) returns Some("").
    // The save implementation must not call create_dir_all("") in that case.
    let result = cfg.save(Path::new(&cfg_path));
    assert!(result.is_ok(), "save to plain filename failed: {:?}", result);
}
