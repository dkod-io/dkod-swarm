//! End-to-end exercise of `dkod_write_symbol` over the in-process rmcp
//! transport. The unit tests in `write_symbol_tool.rs` call the helper
//! directly; this file proves the schema (`WriteSymbolRequest` →
//! `WriteSymbolResponse`) survives a real JSON-RPC roundtrip.
//!
//! Mirrors the precedent set by `plan_tool_e2e.rs` and the M2-2 task plan.

#[path = "common/mod.rs"]
mod common;

use common::{init_tempo_repo, spawn_in_process_server};
use rmcp::model::CallToolRequestParams;
use serde_json::json;

#[tokio::test]
async fn write_symbol_over_mcp_rewrites_disk() {
    let (_tmp, root) = init_tempo_repo();
    let client = spawn_in_process_server(&root).await;

    // Begin a session so dkod_write_symbol has somewhere to log to.
    let begin_args = json!({
        "task_prompt": "demo",
        "groups": [{
            "id": "g1",
            "symbols": [{
                "qualified_name": "a",
                "file_path": "src/lib.rs",
                "kind": "function",
            }],
            "agent_prompt": "rewrite a",
        }],
    })
    .as_object()
    .expect("json! macro produces an object")
    .clone();

    client
        .call_tool(CallToolRequestParams::new("dkod_execute_begin").with_arguments(begin_args))
        .await
        .expect("call dkod_execute_begin");

    let write_args = json!({
        "group_id": "g1",
        "file": "src/lib.rs",
        "qualified_name": "a",
        "new_body": "pub fn a() { /* E2E_MARK */ }",
    })
    .as_object()
    .expect("json! macro produces an object")
    .clone();

    let result = client
        .call_tool(CallToolRequestParams::new("dkod_write_symbol").with_arguments(write_args))
        .await
        .expect("call dkod_write_symbol");

    // Same assertion shape as `plan_tool_e2e.rs`: read the text content
    // block, parse JSON, inspect the response — indifferent to whether
    // rmcp routed the payload through `structured_content` or the text mirror.
    let content = result.content.into_iter().next().expect("content block");
    let text = content
        .raw
        .as_text()
        .expect("text content block")
        .text
        .clone();
    let resp: serde_json::Value = serde_json::from_str(&text).expect("parse json");
    assert_eq!(resp["outcome"], "parsed_ok");
    assert!(
        resp["bytes_written"].as_u64().expect("bytes_written") > 0,
        "bytes_written must be positive: {resp}"
    );

    // Disk side-effect: the marker is on the rewritten file.
    let src = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(
        src.contains("E2E_MARK"),
        "E2E marker missing from rewritten file:\n{src}"
    );

    client.cancel().await.ok();
}
