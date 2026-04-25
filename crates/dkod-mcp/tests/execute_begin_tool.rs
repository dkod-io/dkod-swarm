#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema};
use dkod_mcp::tools::execute_begin::execute_begin;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn execute_begin_creates_branch_and_persists_groups() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![SymbolRefSchema {
                qualified_name: "a".into(),
                file_path: PathBuf::from("src/lib.rs"),
                kind: "function".into(),
            }],
            agent_prompt: "refactor a".into(),
        }],
    };
    let resp = execute_begin(&ctx, req).await.expect("execute_begin");
    assert!(resp.session_id.starts_with("sess-"));
    assert_eq!(resp.dk_branch, format!("dk/{}", resp.session_id));
    assert_eq!(resp.group_ids, vec!["g1".to_string()]);

    // Active session recorded in-memory.
    let active = ctx.active_session.lock().await.clone();
    assert_eq!(active.unwrap().as_str(), resp.session_id);

    // Manifest + group spec on disk.
    let paths = &ctx.paths;
    let manifest_path = paths.manifest(&resp.session_id).unwrap();
    assert!(
        manifest_path.exists(),
        "manifest at {manifest_path:?} missing"
    );
    let spec_path = paths.group_spec(&resp.session_id, "g1").unwrap();
    assert!(spec_path.exists(), "group spec at {spec_path:?} missing");

    // dk-branch checked out.
    let head = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&root)
        .output()
        .unwrap();
    let branch = String::from_utf8(head.stdout).unwrap().trim().to_string();
    assert_eq!(branch, resp.dk_branch);
}

#[tokio::test]
async fn execute_begin_rejects_second_concurrent_session() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![],
            agent_prompt: "x".into(),
        }],
    };
    execute_begin(&ctx, req.clone()).await.unwrap();
    let err = execute_begin(&ctx, req).await.unwrap_err();
    assert!(matches!(err, dkod_mcp::Error::SessionAlreadyActive(_)));
}

#[tokio::test]
async fn execute_begin_rejects_empty_groups() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![],
    };
    let err = execute_begin(&ctx, req).await.unwrap_err();
    assert!(matches!(err, dkod_mcp::Error::InvalidArg(_)));
}

#[tokio::test]
async fn abort_destroys_branch_and_clears_session() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![],
            agent_prompt: "x".into(),
        }],
    };
    let begin = execute_begin(&ctx, req).await.unwrap();

    // Seed a bogus file-lock entry so we can verify abort clears the table.
    let _ = ctx.file_lock(&root.join("marker")).await;

    let abort_resp = dkod_mcp::tools::abort::abort(&ctx).await.expect("abort");
    assert_eq!(abort_resp.session_id, begin.session_id);

    assert!(ctx.active_session.lock().await.is_none());
    assert!(
        ctx.file_locks.lock().await.is_empty(),
        "abort should drop the file-lock table"
    );
    // dk-branch gone.
    let br = std::process::Command::new("git")
        .args(["branch", "--list", &begin.dk_branch])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(String::from_utf8(br.stdout).unwrap().trim().is_empty());

    // Manifest marked Aborted — recovery must not pick it back up.
    use dkod_worktree::{Manifest, SessionId, SessionStatus};
    let sid = SessionId::from_raw(&begin.session_id);
    let m = Manifest::load(&ctx.paths, &sid).expect("manifest still loadable");
    assert_eq!(m.status, SessionStatus::Aborted);
}

#[tokio::test]
async fn abort_without_session_errors() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let err = dkod_mcp::tools::abort::abort(&ctx).await.unwrap_err();
    assert!(matches!(err, dkod_mcp::Error::NoActiveSession));
}
