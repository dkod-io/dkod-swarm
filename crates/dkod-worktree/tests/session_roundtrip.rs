use dkod_worktree::{Manifest, Paths, SessionId, SessionStatus};
use tempfile::TempDir;

#[test]
fn session_id_is_stable_short_string() {
    let a = SessionId::generate();
    let b = SessionId::generate();
    assert_ne!(a.as_str(), b.as_str());
    assert!(a.as_str().len() >= 8, "session id too short");
    assert!(
        a.as_str().chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "session id must be filesystem-safe: got {:?}", a.as_str()
    );
}

#[test]
fn manifest_roundtrips_through_disk() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from_raw("sess-abc");

    let m = Manifest {
        session_id: sid.clone(),
        task_prompt: "refactor auth to passkeys".into(),
        created_at: "2026-04-24T12:00:00Z".into(),
        status: SessionStatus::Planned,
        group_ids: vec!["g1".into(), "g2".into()],
    };
    m.save(&paths).unwrap();

    let loaded = Manifest::load(&paths, &sid).unwrap();
    assert_eq!(loaded.task_prompt, "refactor auth to passkeys");
    assert_eq!(loaded.status, SessionStatus::Planned);
    assert_eq!(loaded.group_ids, vec!["g1", "g2"]);
}
