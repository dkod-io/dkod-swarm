#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{
    ExecuteBeginRequest, ExecuteCompleteRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest,
};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::execute_complete::execute_complete;
use dkod_mcp::tools::status::status;
use dkod_mcp::tools::write_symbol::write_symbol;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn status_is_empty_when_no_session() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let s = status(&ctx).await.expect("status");
    assert!(s.active_session_id.is_none());
    assert!(s.dk_branch.is_none());
    assert!(s.groups.is_empty());
}

#[tokio::test]
async fn status_reports_active_session_and_pending_group() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let begin = execute_begin(
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

    let s = status(&ctx).await.expect("status");
    assert_eq!(
        s.active_session_id.as_deref(),
        Some(begin.session_id.as_str())
    );
    assert_eq!(s.dk_branch.as_deref(), Some(begin.dk_branch.as_str()));
    assert_eq!(s.groups.len(), 1);
    assert_eq!(s.groups[0].id, "g1");
    assert_eq!(s.groups[0].status, "pending");
    assert_eq!(s.groups[0].writes, 0);
    assert_eq!(s.groups[0].agent_summary.as_deref(), Some("x"));
}

#[tokio::test]
async fn status_counts_writes_after_write_symbol() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![SymbolRefSchema {
                    qualified_name: "a".into(),
                    file_path: PathBuf::from("src/lib.rs"),
                    kind: "function".into(),
                }],
                agent_prompt: "rewrite a".into(),
            }],
        },
    )
    .await
    .unwrap();

    write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* rewritten */ }".into(),
        },
    )
    .await
    .expect("write_symbol");

    let s = status(&ctx).await.expect("status");
    assert_eq!(s.groups.len(), 1);
    assert_eq!(s.groups[0].writes, 1);
    // Status is still pending — write_symbol does not flip it.
    assert_eq!(s.groups[0].status, "pending");
}

#[tokio::test]
async fn status_reports_done_after_execute_complete() {
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
            summary: "wrapped up".into(),
        },
    )
    .await
    .unwrap();

    let s = status(&ctx).await.expect("status");
    assert_eq!(s.groups.len(), 1);
    assert_eq!(s.groups[0].status, "done");
    let summary = s.groups[0]
        .agent_summary
        .as_deref()
        .expect("agent_summary populated");
    assert!(
        summary.contains("wrapped up"),
        "agent_summary should reflect the appended summary; got {summary:?}"
    );
}
