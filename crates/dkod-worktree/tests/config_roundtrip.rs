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

#[test]
fn config_saves_cleanly_to_plain_filename_with_no_parent() {
    // Path::parent() of a single-component relative path returns Some("");
    // the save() guard must handle that without calling create_dir_all("").
    let tmp = TempDir::new().unwrap();
    let prev = std::env::current_dir().unwrap();

    struct CwdGuard(std::path::PathBuf);
    impl Drop for CwdGuard {
        fn drop(&mut self) { let _ = std::env::set_current_dir(&self.0); }
    }
    let _guard = CwdGuard(prev);
    std::env::set_current_dir(tmp.path()).unwrap();

    let cfg = Config { main_branch: "main".into(), verify_cmd: None };
    // Bare filename — parent() is Some("").
    let path = std::path::Path::new("config.toml");
    assert_eq!(path.parent().map(|p| p.as_os_str().is_empty()), Some(true),
        "test premise: bare filename must have empty parent");
    cfg.save(path).expect("save to plain filename must succeed");
    assert!(tmp.path().join("config.toml").is_file(), "file was not written");
}
