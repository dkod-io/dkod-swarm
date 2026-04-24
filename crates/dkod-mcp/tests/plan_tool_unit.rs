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

#[test]
fn plan_rejects_absolute_file_path() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = PlanRequest {
        task_prompt: "demo".into(),
        in_scope: vec!["a".into()],
        files: vec![PathBuf::from("/etc/passwd")],
        target_groups: 1,
    };
    let err = build_plan(&ctx, req).expect_err("absolute path must be rejected");
    assert!(
        matches!(err, dkod_mcp::Error::InvalidArg(ref m) if m.contains("absolute")),
        "unexpected error: {err:?}"
    );
}

#[test]
fn plan_rejects_parent_traversal_escaping_repo() {
    // Build a "secret" file outside the repo and try to traverse to it from
    // a `../` request. `canonicalize` + `starts_with` guards must catch it.
    let outer = tempfile::tempdir().expect("outer tempdir");
    std::fs::write(outer.path().join("secret.rs"), "pub fn leaked() {}").unwrap();
    let repo = outer.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();

    // Init a minimal git repo so `ServerCtx::new` + `init_repo` would work;
    // we only need a valid repo_root for `build_plan`'s resolve step.
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());

    let ctx = Arc::new(ServerCtx::new(&repo));
    let req = PlanRequest {
        task_prompt: "demo".into(),
        in_scope: vec!["leaked".into()],
        files: vec![PathBuf::from("../secret.rs")],
        target_groups: 1,
    };
    let err = build_plan(&ctx, req).expect_err("parent traversal must be rejected");
    assert!(
        matches!(err, dkod_mcp::Error::InvalidArg(ref m) if m.contains("escapes repo root")),
        "unexpected error: {err:?}"
    );
}
