use dkod_orchestrator::commit::commit_per_group;
use dkod_worktree::{branch, GroupSpec, GroupStatus, Paths, SessionId, SymbolRef, WriteLog, WriteRecord};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn init_repo(dir: &Path) {
    let st = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(st.success(), "git init failed");
    std::fs::write(dir.join("README.md"), "hi").unwrap();
    let st = Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(st.success(), "git add failed");
    let st = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Haim Ari")
        .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
        .env("GIT_COMMITTER_NAME", "Haim Ari")
        .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
        .status()
        .unwrap();
    assert!(st.success(), "git commit failed");
}

#[test]
fn writes_one_commit_per_group_with_forced_identity() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let sid = SessionId::from_raw("sess-abc");
    branch::create_dk_branch(repo, "main", sid.as_str()).unwrap();

    let paths = Paths::new(repo);

    // Group g1: modifies src/a.rs
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "pub fn a() {}\n").unwrap();
    GroupSpec {
        id: "g1".into(),
        symbols: vec![SymbolRef {
            qualified_name: "a".into(),
            file_path: PathBuf::from("src/a.rs"),
            kind: "function".into(),
        }],
        agent_prompt: "...".into(),
        status: GroupStatus::Done,
    }
    .save(&paths, &sid)
    .unwrap();
    let log_g1 = WriteLog::open(&paths, &sid, "g1").unwrap();
    log_g1
        .append(&WriteRecord {
            symbol: "a".into(),
            file_path: PathBuf::from("src/a.rs"),
            timestamp: "2026-04-24T12:00:00Z".into(),
        })
        .unwrap();

    // Group g2: modifies src/b.rs
    std::fs::write(repo.join("src/b.rs"), "pub fn b() {}\n").unwrap();
    GroupSpec {
        id: "g2".into(),
        symbols: vec![SymbolRef {
            qualified_name: "b".into(),
            file_path: PathBuf::from("src/b.rs"),
            kind: "function".into(),
        }],
        agent_prompt: "...".into(),
        status: GroupStatus::Done,
    }
    .save(&paths, &sid)
    .unwrap();
    let log_g2 = WriteLog::open(&paths, &sid, "g2").unwrap();
    log_g2
        .append(&WriteRecord {
            symbol: "b".into(),
            file_path: PathBuf::from("src/b.rs"),
            timestamp: "2026-04-24T12:00:01Z".into(),
        })
        .unwrap();

    commit_per_group(repo, &paths, &sid, &["g1".into(), "g2".into()]).unwrap();

    let log = Command::new("git")
        .args(["log", "--format=%an <%ae> | %s"])
        .current_dir(repo)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&log.stdout);
    // Most recent first. Collect first and assert the exact count so extra
    // commits (e.g. a regression that double-commits a group) are caught.
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "expected init + g1 + g2; got:\n{text}");
    let (l1, l2, l3) = (lines[0], lines[1], lines[2]);
    assert!(
        l1.contains("Haim Ari <haimari1@gmail.com>") && l1.contains("g2") && l1.contains("symbol writes"),
        "top commit = g2 with message format; got {l1}"
    );
    assert!(
        l2.contains("Haim Ari <haimari1@gmail.com>") && l2.contains("g1") && l2.contains("symbol writes"),
        "second = g1 with message format; got {l2}"
    );
    assert!(l3.contains("init"), "base commit preserved; got {l3}");
}

#[test]
fn skips_empty_groups_gracefully() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let sid = SessionId::from_raw("sess-skip");
    branch::create_dk_branch(repo, "main", sid.as_str()).unwrap();

    let paths = Paths::new(repo);

    // Group g1: empty writes.jsonl (never appended to)
    GroupSpec {
        id: "g1".into(),
        symbols: vec![],
        agent_prompt: "...".into(),
        status: GroupStatus::Pending,
    }
    .save(&paths, &sid)
    .unwrap();

    // Group g2: has one write
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/c.rs"), "pub fn c() {}\n").unwrap();
    GroupSpec {
        id: "g2".into(),
        symbols: vec![SymbolRef {
            qualified_name: "c".into(),
            file_path: PathBuf::from("src/c.rs"),
            kind: "function".into(),
        }],
        agent_prompt: "...".into(),
        status: GroupStatus::Done,
    }
    .save(&paths, &sid)
    .unwrap();
    let log_g2 = WriteLog::open(&paths, &sid, "g2").unwrap();
    log_g2
        .append(&WriteRecord {
            symbol: "c".into(),
            file_path: PathBuf::from("src/c.rs"),
            timestamp: "2026-04-24T12:00:02Z".into(),
        })
        .unwrap();

    // Should not fail even though g1 has no writes.jsonl entries.
    commit_per_group(repo, &paths, &sid, &["g1".into(), "g2".into()]).unwrap();

    let log = Command::new("git")
        .args(["log", "--format=%an <%ae> | %s"])
        .current_dir(repo)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&log.stdout);
    let lines: Vec<&str> = text.lines().collect();
    // Should be 2 commits: init + g2 only (g1 was skipped).
    assert_eq!(lines.len(), 2, "expected init + 1 group commit; got:\n{text}");
    assert!(
        lines[0].contains("Haim Ari <haimari1@gmail.com>") && lines[0].contains("g2"),
        "top commit = g2; got {}",
        lines[0]
    );
    assert!(lines[1].contains("init"), "base commit preserved; got {}", lines[1]);
}
