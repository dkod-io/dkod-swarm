#[path = "common/mod.rs"]
mod common;

use common::{init_tempo_repo, spawn_in_process_server};
use rmcp::model::CallToolRequestParams;
use serde_json::json;

#[tokio::test]
async fn plan_over_mcp_returns_expected_groups() {
    let (_tmp, root) = init_tempo_repo();
    let client = spawn_in_process_server(&root).await;

    let args = json!({
        "task_prompt": "demo",
        "in_scope": ["a", "b", "c", "d"],
        "files": ["src/lib.rs"],
        "target_groups": 2,
    });
    let args_obj = args
        .as_object()
        .expect("json! macro produces an object")
        .clone();

    let result = client
        .call_tool(CallToolRequestParams::new("dkod_plan").with_arguments(args_obj))
        .await
        .expect("call_tool");

    // `Json<PlanResponse>` returns via `CallToolResult::structured`, which
    // populates both `structured_content` (the primary payload) and a text
    // block mirroring the same JSON. We assert via the text block — per the
    // plan template — so the assertion is indifferent to which slot the
    // server filled.
    let content = result.content.into_iter().next().expect("content block");
    let text = content
        .raw
        .as_text()
        .expect("text content block")
        .text
        .clone();
    let resp: serde_json::Value = serde_json::from_str(&text).expect("parse json");
    assert_eq!(
        resp["groups"].as_array().expect("groups array").len(),
        2,
        "tiny_rust partitions into two groups: {{a,b,c}} and {{d}}"
    );

    client.cancel().await.ok();
}
