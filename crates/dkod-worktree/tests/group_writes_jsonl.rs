use dkod_worktree::{GroupSpec, GroupStatus, Paths, SessionId, SymbolRef, WriteLog, WriteRecord};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn group_spec_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from_raw("sess-abc");
    let gid = "g1";

    let spec = GroupSpec {
        id: gid.into(),
        symbols: vec![
            SymbolRef {
                qualified_name: "auth::login".into(),
                file_path: PathBuf::from("src/auth.rs"),
                kind: "function".into(),
            },
        ],
        agent_prompt: "rewrite these as passkeys".into(),
        status: GroupStatus::Pending,
    };
    spec.save(&paths, &sid).unwrap();

    let loaded = GroupSpec::load(&paths, &sid, gid).unwrap();
    assert_eq!(loaded.symbols.len(), 1);
    assert_eq!(loaded.symbols[0].qualified_name, "auth::login");
    assert_eq!(loaded.status, GroupStatus::Pending);
}

#[test]
fn write_log_appends_as_jsonl_and_reads_back_in_order() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from_raw("sess-abc");
    let gid = "g1";

    let log = WriteLog::open(&paths, &sid, gid).unwrap();
    log.append(&WriteRecord {
        symbol: "auth::login".into(),
        file_path: PathBuf::from("src/auth.rs"),
        timestamp: "2026-04-24T12:00:00Z".into(),
    }).unwrap();
    log.append(&WriteRecord {
        symbol: "auth::logout".into(),
        file_path: PathBuf::from("src/auth.rs"),
        timestamp: "2026-04-24T12:00:01Z".into(),
    }).unwrap();

    // Verify the on-disk file is genuinely newline-delimited JSON, not some
    // other serialization form that happens to roundtrip.
    let raw_path = paths.group_writes(sid.as_str(), gid).unwrap();
    let raw = std::fs::read_to_string(&raw_path).unwrap();
    let lines: Vec<&str> = raw.lines().collect();
    assert_eq!(lines.len(), 2, "expected exactly two JSONL lines, got: {raw:?}");
    for line in &lines {
        assert!(line.starts_with('{') && line.ends_with('}'),
            "JSONL line is not a JSON object: {line:?}");
    }

    let rows = log.read_all().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].symbol, "auth::login");
    assert_eq!(rows[1].symbol, "auth::logout");
}

#[test]
fn group_spec_load_rejects_mismatched_id() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from_raw("sess-abc");

    // Save as g1, then attempt to load as g2 by relocating the on-disk file.
    let spec = GroupSpec {
        id: "g1".into(),
        symbols: vec![],
        agent_prompt: String::new(),
        status: GroupStatus::Pending,
    };
    spec.save(&paths, &sid).unwrap();

    // Move g1/spec.json to g2/spec.json to simulate the mismatch.
    let g1_path = paths.group_spec(sid.as_str(), "g1").unwrap();
    let g2_path = paths.group_spec(sid.as_str(), "g2").unwrap();
    std::fs::create_dir_all(g2_path.parent().unwrap()).unwrap();
    std::fs::rename(&g1_path, &g2_path).unwrap();

    let err = GroupSpec::load(&paths, &sid, "g2").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("group id mismatch"), "unexpected error: {msg}");
    assert!(msg.contains("\"g1\"") && msg.contains("\"g2\""), "error lacks ids: {msg}");
}

#[test]
fn write_log_read_all_returns_empty_when_log_missing() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from_raw("sess-abc");
    let gid = "g1";

    // Open creates the group directory but does not create the writes.jsonl
    // file until the first append. read_all on this fresh log must return
    // Ok(empty) — a never-written log is semantically empty.
    let log = WriteLog::open(&paths, &sid, gid).unwrap();
    let rows = log.read_all().unwrap();
    assert!(rows.is_empty(), "expected empty Vec, got {rows:?}");
}
