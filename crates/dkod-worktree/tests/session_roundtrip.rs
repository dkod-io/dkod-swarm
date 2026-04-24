use dkod_worktree::{Manifest, Paths, SessionId, SessionStatus};
use tempfile::TempDir;

#[test]
fn session_id_is_stable_short_string() {
    let a = SessionId::generate();
    let b = SessionId::generate();
    assert_ne!(a.as_str(), b.as_str());
    for (label, id) in [("a", &a), ("b", &b)] {
        assert!(id.as_str().len() >= 8, "{label}: session id too short: {:?}", id.as_str());
        assert!(
            id.as_str().chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "{label}: session id must be filesystem-safe: got {:?}", id.as_str()
        );
    }
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
    assert_eq!(loaded.session_id, sid);
    assert_eq!(loaded.created_at, "2026-04-24T12:00:00Z");
    assert_eq!(loaded.task_prompt, "refactor auth to passkeys");
    assert_eq!(loaded.status, SessionStatus::Planned);
    assert_eq!(loaded.group_ids, vec!["g1", "g2"]);
}

#[test]
fn manifest_load_rejects_mismatched_session_id() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());

    // Save a manifest claiming sid=sess-a, then move the file to sess-b's
    // expected location and try to load as sess-b.
    let sid_a = SessionId::from_raw("sess-a");
    let sid_b = SessionId::from_raw("sess-b");
    let m = Manifest {
        session_id: sid_a.clone(),
        task_prompt: "t".into(),
        created_at: "2026-04-24T12:00:00Z".into(),
        status: SessionStatus::Planned,
        group_ids: vec![],
    };
    m.save(&paths).unwrap();

    let src = paths.manifest(sid_a.as_str()).unwrap();
    let dst = paths.manifest(sid_b.as_str()).unwrap();
    std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
    std::fs::rename(&src, &dst).unwrap();

    let err = Manifest::load(&paths, &sid_b).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("session id mismatch"), "unexpected error: {msg}");
    assert!(msg.contains("\"sess-a\"") && msg.contains("\"sess-b\""), "error lacks ids: {msg}");
}
