use dkod_worktree::Paths;
use std::path::PathBuf;

#[test]
fn paths_resolve_under_dkod_dir() {
    let repo = PathBuf::from("/tmp/fake-repo");
    let p = Paths::new(&repo);
    assert_eq!(p.root(), repo.join(".dkod"));
    assert_eq!(p.config(), repo.join(".dkod/config.toml"));
    assert_eq!(p.sessions_dir(), repo.join(".dkod/sessions"));
    assert_eq!(p.session("abc"), repo.join(".dkod/sessions/abc"));
    assert_eq!(p.manifest("abc"), repo.join(".dkod/sessions/abc/manifest.json"));
    assert_eq!(p.groups_dir("abc"), repo.join(".dkod/sessions/abc/groups"));
    assert_eq!(p.group("abc", "g1"), repo.join(".dkod/sessions/abc/groups/g1"));
    assert_eq!(p.group_spec("abc", "g1"), repo.join(".dkod/sessions/abc/groups/g1/spec.json"));
    assert_eq!(p.group_writes("abc", "g1"), repo.join(".dkod/sessions/abc/groups/g1/writes.jsonl"));
    assert_eq!(p.conflicts_dir("abc"), repo.join(".dkod/sessions/abc/conflicts"));
}
