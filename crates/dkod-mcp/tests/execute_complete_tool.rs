#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, ExecuteCompleteRequest, GroupInput};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::execute_complete::execute_complete;
use dkod_worktree::{GroupSpec, GroupStatus, SessionId};
use std::sync::Arc;

#[tokio::test]
async fn execute_complete_marks_group_done_and_persists_summary() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![],
                agent_prompt: "refactor a".into(),
            }],
        },
    )
    .await
    .unwrap();

    let resp = execute_complete(
        &ctx,
        ExecuteCompleteRequest {
            group_id: "g1".into(),
            summary: "all done".into(),
        },
    )
    .await
    .expect("execute_complete");
    assert_eq!(resp.group_id, "g1");
    assert_eq!(resp.new_status, "done");

    // Reload from disk to confirm the spec was persisted.
    let sid = ctx.active_session.lock().await.clone().unwrap();
    let spec = GroupSpec::load(&ctx.paths, &sid, "g1").expect("group spec reload");
    assert!(matches!(spec.status, GroupStatus::Done));
    // Summary persisted by appending to agent_prompt — see helper module
    // doc-comment for rationale.
    assert!(
        spec.agent_prompt.contains("all done"),
        "agent_prompt should carry the summary; got {:?}",
        spec.agent_prompt
    );
    assert!(
        spec.agent_prompt.starts_with("refactor a"),
        "original agent_prompt prefix must be preserved; got {:?}",
        spec.agent_prompt
    );
}

#[tokio::test]
async fn execute_complete_without_session_errors() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let err = execute_complete(
        &ctx,
        ExecuteCompleteRequest {
            group_id: "g1".into(),
            summary: "noop".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, dkod_mcp::Error::NoActiveSession));
}

#[tokio::test]
async fn execute_complete_unknown_group_errors() {
    let (_tmp, root) = init_tempo_repo();
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
    .unwrap();
    let err = execute_complete(
        &ctx,
        ExecuteCompleteRequest {
            group_id: "ghost".into(),
            summary: "noop".into(),
        },
    )
    .await
    .unwrap_err();
    match err {
        dkod_mcp::Error::UnknownGroup(g) => assert_eq!(g, "ghost"),
        other => panic!("expected UnknownGroup, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_complete_is_idempotent_on_repeat() {
    // A second `execute_complete` for the same group should still succeed
    // (the spec is already Done) and append a second summary suffix. This
    // documents the current behaviour — agents are expected to call once,
    // but the contract should not crash on retry.
    let (_tmp, root) = init_tempo_repo();
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
    .unwrap();
    execute_complete(
        &ctx,
        ExecuteCompleteRequest {
            group_id: "g1".into(),
            summary: "first".into(),
        },
    )
    .await
    .unwrap();
    execute_complete(
        &ctx,
        ExecuteCompleteRequest {
            group_id: "g1".into(),
            summary: "second".into(),
        },
    )
    .await
    .unwrap();

    let sid = SessionId::from_raw(ctx.active_session.lock().await.clone().unwrap().as_str());
    let spec = GroupSpec::load(&ctx.paths, &sid, "g1").unwrap();
    assert!(matches!(spec.status, GroupStatus::Done));
    assert!(spec.agent_prompt.contains("first"));
    assert!(spec.agent_prompt.contains("second"));
}
