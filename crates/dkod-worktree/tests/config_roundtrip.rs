use dkod_worktree::{Config, Paths};
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
