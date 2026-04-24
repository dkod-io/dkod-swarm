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
