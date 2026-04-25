use std::path::PathBuf;

fn init_repo(root: &std::path::Path) {
    let s = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(s.success());
    // Seed at least one commit so `git checkout -b dk/...` works.
    std::fs::write(root.join("README.md"), "seed").unwrap();
    let s = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(s.success());
    let s = std::process::Command::new("git")
        .args(["commit", "-q", "-m", "seed"])
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .status()
        .unwrap();
    assert!(s.success());
    dkod_worktree::init_repo(root, None).unwrap();
}

#[tokio::test]
async fn abort_errors_when_no_session() {
    let tmp = tempfile::tempdir().unwrap();
    let root: PathBuf = tmp.path().to_path_buf();
    init_repo(&root);
    let err = dkod_cli::cmd::abort::render(&root).await.unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("no active session"), "unexpected error: {s}");
}

#[tokio::test]
async fn abort_clears_an_active_session() {
    use dkod_mcp::ServerCtx;
    use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput};
    use dkod_mcp::tools::execute_begin::execute_begin;
    use std::sync::Arc;

    let tmp = tempfile::tempdir().unwrap();
    let root: PathBuf = tmp.path().to_path_buf();
    init_repo(&root);

    // Spin up an executing session via the MCP helper, then drop the ctx
    // so the on-disk state is the only thing left for `dkod abort` to
    // recover from.
    {
        let ctx = Arc::new(ServerCtx::new(&root));
        execute_begin(
            &ctx,
            ExecuteBeginRequest {
                task_prompt: "demo".into(),
                groups: vec![GroupInput {
                    id: "g1".into(),
                    symbols: vec![],
                    agent_prompt: "x".into(),
                }],
            },
        )
        .await
        .expect("execute_begin");
    }

    let json = dkod_cli::cmd::abort::render(&root)
        .await
        .expect("abort render");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["session_id"].is_string());

    // dk-branch must be gone.
    let out = std::process::Command::new("git")
        .args(["branch", "--list", "dk/*"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        String::from_utf8(out.stdout).unwrap().trim().is_empty(),
        "dk-branch should have been destroyed"
    );
}
