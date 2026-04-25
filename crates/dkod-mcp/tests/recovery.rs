#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput};
use dkod_mcp::tools::execute_begin::execute_begin;
use std::sync::Arc;

#[tokio::test]
async fn fresh_ctx_recovers_executing_session() {
    let (_tmp, root) = init_tempo_repo();
    // Ctx A: begin a session (leaves an Executing manifest on disk).
    let expected_sid = {
        let ctx = Arc::new(ServerCtx::new(&root));
        let resp = execute_begin(
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
        resp.session_id
        // Ctx A dropped here — mimic a process restart.
    };

    // Ctx B: fresh process; recovery populates active_session.
    let ctx = ServerCtx::new(&root);
    ctx.recover().await.expect("recover");
    let active = ctx.active_session.lock().await.clone();
    let sid = active.expect("recovery should have picked up the Executing session");
    assert_eq!(sid.as_str(), expected_sid);
}

#[tokio::test]
async fn fresh_ctx_ignores_aborted_session() {
    let (_tmp, root) = init_tempo_repo();
    // Begin then abort — manifest ends up in state Aborted, not Executing.
    {
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
        dkod_mcp::tools::abort::abort(&ctx).await.unwrap();
    }

    let ctx = ServerCtx::new(&root);
    ctx.recover().await.expect("recover");
    assert!(
        ctx.active_session.lock().await.is_none(),
        "aborted sessions must not be picked up"
    );
}

#[tokio::test]
async fn fresh_ctx_with_no_sessions_is_noop() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = ServerCtx::new(&root);
    ctx.recover().await.expect("recover on empty .dkod is ok");
    assert!(ctx.active_session.lock().await.is_none());
}

#[test]
fn resolve_main_falls_back_to_detect_main_when_config_missing() {
    let (_tmp, root) = common::init_tempo_repo();
    // Delete the config file written by `init_repo`; scan should now read
    // from `branch::detect_main`, which in a fresh repo with HEAD on
    // `main` returns `"main"`.
    std::fs::remove_file(root.join(".dkod/config.toml")).unwrap();
    let ctx = dkod_mcp::ServerCtx::new(&root);
    assert_eq!(ctx.resolve_main().unwrap(), "main");
}
