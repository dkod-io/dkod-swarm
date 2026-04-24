use dkod_worktree::branch;
use std::process::Command;
use tempfile::TempDir;

fn init_repo(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let st = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Haim Ari")
            .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
            .env("GIT_COMMITTER_NAME", "Haim Ari")
            .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    };
    run(&["init", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "hi").unwrap();
    run(&["add", "README.md"]);
    run(&["commit", "-m", "initial"]);
}

#[test]
fn detect_main_returns_main() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    let main = branch::detect_main(tmp.path()).unwrap();
    assert_eq!(main, "main");
}

#[test]
fn create_dkbranch_off_main_then_destroy() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    branch::create_dk_branch(tmp.path(), "main", "sess-abc").unwrap();

    let cur = Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(tmp.path()).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&cur.stdout).trim(), "dk/sess-abc");

    branch::destroy_dk_branch(tmp.path(), "main", "sess-abc").unwrap();

    let cur = Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(tmp.path()).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&cur.stdout).trim(), "main");

    let branches = Command::new("git").args(["branch", "--list", "dk/sess-abc"])
        .current_dir(tmp.path()).output().unwrap();
    assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
}

#[test]
fn commit_on_dk_branch_uses_enforced_identity() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    branch::create_dk_branch(tmp.path(), "main", "sess-abc").unwrap();

    std::fs::write(tmp.path().join("a.txt"), "hello").unwrap();
    branch::commit_paths(
        tmp.path(),
        &[std::path::Path::new("a.txt")],
        "group g1: initial land",
    ).unwrap();

    let out = Command::new("git")
        .args(["log", "-1", "--format=%an <%ae> | %cn <%ce> | %s"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(line, "Haim Ari <haimari1@gmail.com> | Haim Ari <haimari1@gmail.com> | group g1: initial land");
}
