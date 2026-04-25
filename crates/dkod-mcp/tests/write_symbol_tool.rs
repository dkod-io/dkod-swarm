#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::write_symbol::write_symbol;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn write_symbol_replaces_function_body() {
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
    .expect("execute_begin");

    let resp = write_symbol(
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

    assert_eq!(resp.outcome, "parsed_ok");
    assert!(resp.fallback_reason.is_none());
    assert!(resp.bytes_written > 0);

    // Disk now contains the new body.
    let src = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(
        src.contains("/* rewritten */"),
        "rewritten marker missing from {src}"
    );
    // bytes_written matches the on-disk file length.
    let on_disk_len = std::fs::metadata(root.join("src/lib.rs")).unwrap().len() as usize;
    assert_eq!(resp.bytes_written, on_disk_len);

    // writes.jsonl has one record matching the symbol.
    let sid = ctx.active_session.lock().await.clone().expect("active sid");
    let log = dkod_worktree::WriteLog::open(&ctx.paths, &sid, "g1").expect("open log");
    let records = log.read_all().expect("read log");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].symbol, "a");
    assert_eq!(records[0].file_path, PathBuf::from("src/lib.rs"));
    assert!(
        !records[0].timestamp.is_empty(),
        "WriteRecord.timestamp is required"
    );
}

#[tokio::test]
async fn write_symbol_rejects_when_no_active_session() {
    // No execute_begin, so dkod_write_symbol should refuse cleanly rather
    // than touch disk.
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));

    let err = write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() {}".into(),
        },
    )
    .await
    .expect_err("must require an active session");
    assert!(matches!(err, dkod_mcp::Error::NoActiveSession));
}

#[tokio::test]
async fn write_symbol_rejects_absolute_path() {
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

    let err = write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("/etc/passwd"),
            qualified_name: "a".into(),
            new_body: "x".into(),
        },
    )
    .await
    .expect_err("absolute path must be rejected");
    assert!(
        matches!(err, dkod_mcp::Error::InvalidArg(ref m) if m.contains("absolute")),
        "unexpected error: {err:?}"
    );
}
