use dkod_worktree::Paths;
use std::path::PathBuf;

#[test]
fn paths_resolve_under_dkod_dir() {
    let repo = PathBuf::from("/tmp/fake-repo");
    let p = Paths::new(&repo);
    assert_eq!(p.root(), repo.join(".dkod"));
    assert_eq!(p.config(), repo.join(".dkod/config.toml"));
    assert_eq!(p.sessions_dir(), repo.join(".dkod/sessions"));
    assert_eq!(p.session("abc").unwrap(), repo.join(".dkod/sessions/abc"));
    assert_eq!(p.manifest("abc").unwrap(), repo.join(".dkod/sessions/abc/manifest.json"));
    assert_eq!(p.groups_dir("abc").unwrap(), repo.join(".dkod/sessions/abc/groups"));
    assert_eq!(p.group("abc", "g1").unwrap(), repo.join(".dkod/sessions/abc/groups/g1"));
    assert_eq!(p.group_spec("abc", "g1").unwrap(), repo.join(".dkod/sessions/abc/groups/g1/spec.json"));
    assert_eq!(p.group_writes("abc", "g1").unwrap(), repo.join(".dkod/sessions/abc/groups/g1/writes.jsonl"));
    assert_eq!(p.conflicts_dir("abc").unwrap(), repo.join(".dkod/sessions/abc/conflicts"));
}

#[test]
fn session_rejects_absolute_path() {
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    assert!(p.session("/absolute").is_err());
}

#[test]
fn session_rejects_path_traversal() {
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    assert!(p.session("..").is_err());
    assert!(p.session("a/b").is_err());
    assert!(p.session("../escape").is_err());
}

#[test]
fn group_rejects_bad_gid_even_with_valid_sid() {
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    assert!(p.group("sess-ok", "..").is_err());
    assert!(p.group("sess-ok", "a/b").is_err());
}

#[test]
fn valid_ids_still_work() {
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    assert_eq!(p.session("sess-abc").unwrap(), PathBuf::from("/tmp/r/.dkod/sessions/sess-abc"));
    assert_eq!(p.group("sess-abc", "g1").unwrap(), PathBuf::from("/tmp/r/.dkod/sessions/sess-abc/groups/g1"));
}
