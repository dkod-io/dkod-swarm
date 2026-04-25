//! The lock test that justifies `ServerCtx::file_lock`'s existence.
//!
//! Two concurrent `dkod_write_symbol` calls target the same file but
//! different symbols. Without per-file serialisation, one task's read
//! would see the basis source while the other was midway through its
//! own read-modify-write — the race would land only one rewrite,
//! silently dropping the other.
//!
//! With the lock in place both rewrites must appear in the final file.
//! Run this test 5x in a loop locally (see PR M2-4 instructions) to
//! exercise scheduler variance — flakiness here means the lock is wrong,
//! not that the test needs a sleep.

#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::write_symbol::write_symbol;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writes_to_same_file_serialise() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![
                    SymbolRefSchema {
                        qualified_name: "a".into(),
                        file_path: PathBuf::from("src/lib.rs"),
                        kind: "function".into(),
                    },
                    SymbolRefSchema {
                        qualified_name: "b".into(),
                        file_path: PathBuf::from("src/lib.rs"),
                        kind: "function".into(),
                    },
                ],
                agent_prompt: "rewrite a and b".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");

    let mut handles = Vec::new();
    for (name, marker) in [("a", "MARK_A"), ("b", "MARK_B")] {
        let ctx = Arc::clone(&ctx);
        let name = name.to_string();
        let marker = marker.to_string();
        handles.push(tokio::spawn(async move {
            write_symbol(
                &ctx,
                WriteSymbolRequest {
                    group_id: "g1".into(),
                    file: PathBuf::from("src/lib.rs"),
                    qualified_name: name.clone(),
                    new_body: format!("pub fn {name}() {{ /* {marker} */ }}"),
                },
            )
            .await
            .unwrap_or_else(|e| panic!("write_symbol({name}) failed: {e:?}"))
        }));
    }
    for h in handles {
        h.await.expect("join task");
    }

    let src = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    // Both markers present → both writes landed on the final file.
    assert!(src.contains("MARK_A"), "a's rewrite lost, file:\n{src}");
    assert!(src.contains("MARK_B"), "b's rewrite lost, file:\n{src}");

    // Both records logged exactly once each.
    let sid = ctx.active_session.lock().await.clone().expect("active sid");
    let records = dkod_worktree::WriteLog::open(&ctx.paths, &sid, "g1")
        .expect("open log")
        .read_all()
        .expect("read log");
    assert_eq!(
        records.len(),
        2,
        "expected one record per write, got {records:?}"
    );
    let mut symbols: Vec<_> = records.iter().map(|r| r.symbol.as_str()).collect();
    symbols.sort();
    assert_eq!(symbols, vec!["a", "b"]);
}
