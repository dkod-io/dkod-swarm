use dkod_worktree::{GroupSpec, GroupStatus, Paths, SessionId, SymbolRef, WriteLog, WriteRecord};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn group_spec_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from("sess-abc");
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
    let sid = SessionId::from("sess-abc");
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

    let rows = WriteLog::read_all(&paths, &sid, gid).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].symbol, "auth::login");
    assert_eq!(rows[1].symbol, "auth::logout");
}
