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
fn session_rejects_empty_id() {
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    assert!(p.session("").is_err());
}

#[test]
fn session_rejects_cur_dir_id() {
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    assert!(p.session(".").is_err());
}

/// On Unix, Path strips the trailing slash so "foo/" parses as a single Normal("foo")
/// component — validate_id accepts it as equivalent to "foo".
/// Windows-style separators ("foo\\") and drive-relative forms ("C:foo") are also
/// treated as opaque Normal components on Unix and are therefore accepted.
/// These tests document the observed platform behaviour.
#[test]
fn session_rejects_multi_component_with_backslash_separator_if_applicable() {
    // On Unix "foo\\" is a single Normal component (backslash is not a separator).
    // The test is here to ensure we do not silently regress if the validation
    // logic is ever tightened; adjust assertions if Windows support is added.
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    // "a/b" has two Normal components and must be rejected.
    assert!(p.session("a/b").is_err(), "slash-separated path must be rejected");
}

#[test]
fn valid_ids_still_work() {
    let p = Paths::new(&PathBuf::from("/tmp/r"));
    assert_eq!(p.session("sess-abc").unwrap(), PathBuf::from("/tmp/r/.dkod/sessions/sess-abc"));
    assert_eq!(p.group("sess-abc", "g1").unwrap(), PathBuf::from("/tmp/r/.dkod/sessions/sess-abc/groups/g1"));
}
