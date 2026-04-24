#[path = "common/mod.rs"]
mod common;
use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::PlanRequest;
use dkod_mcp::tools::plan::build_plan;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn plan_on_tiny_rust_partitions_disconnected_fns() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = PlanRequest {
        task_prompt: "demo".into(),
        in_scope: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        files: vec![PathBuf::from("src/lib.rs")],
        target_groups: 2,
    };
    let resp = build_plan(&ctx, req).expect("build_plan");
    // c calls a and b — so {a, b, c} coalesces into one group; d is alone.
    assert_eq!(resp.groups.len(), 2);
    let g_coupled = resp
        .groups
        .iter()
        .find(|g| g.symbols.len() == 3)
        .expect("coupled group");
    let names: Vec<_> = g_coupled
        .symbols
        .iter()
        .map(|s| s.qualified_name.as_str())
        .collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    assert!(names.contains(&"c"));
}
