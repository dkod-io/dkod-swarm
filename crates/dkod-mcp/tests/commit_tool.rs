#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest};
use dkod_mcp::tools::commit::commit;
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::write_symbol::write_symbol;
use dkod_worktree::{Manifest, SessionStatus};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn commit_writes_one_commit_per_group_with_writes() {
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

    write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* x */ }".into(),
        },
    )
    .await
    .expect("write_symbol");

    let resp = commit(&ctx).await.expect("commit");
    assert_eq!(resp.commits_created, 1);
    assert_eq!(resp.commit_shas.len(), 1);
    assert!(
        resp.dk_branch.starts_with("dk/"),
        "dk_branch should start with dk/, got {}",
        resp.dk_branch
    );

    // Latest commit author/committer is the real Haim Ari identity that
    // `branch::commit_paths` forces via env vars — proves we are not
    // accidentally inheriting the fixture identity.
    let log = std::process::Command::new("git")
        .args(["log", "-1", "--format=%an <%ae> | %cn <%ce>"])
        .current_dir(&root)
        .output()
        .unwrap();
    let out = String::from_utf8(log.stdout).unwrap();
    assert!(
        out.contains("Haim Ari <haimari1@gmail.com>"),
        "identity wrong: {out}"
    );

    // Commit subject matches `commit_per_group`'s `group <gid>: …` format.
    let subj = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&root)
        .output()
        .unwrap();
    let subj = String::from_utf8(subj.stdout).unwrap();
    assert!(subj.contains("group g1"), "subject: {subj}");

    // Manifest transitions to Committed (Task 19).
    let sid = ctx.active_session.lock().await.clone().unwrap();
    let manifest = Manifest::load(&ctx.paths, &sid).unwrap();
    assert!(
        matches!(manifest.status, SessionStatus::Committed),
        "manifest status should be Committed; got {:?}",
        manifest.status
    );
}

#[tokio::test]
async fn commit_marks_manifest_committed() {
    // Standalone repro of the manifest-transition contract (Task 19),
    // independent of the happy-path assertions above. Uses the same
    // begin → write → commit flow so a regression in the transition step
    // is caught even if the SHA / author assertions silently regress.
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

    write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* y */ }".into(),
        },
    )
    .await
    .expect("write_symbol");

    commit(&ctx).await.expect("commit");

    let sid = ctx.active_session.lock().await.clone().unwrap();
    let manifest = Manifest::load(&ctx.paths, &sid).unwrap();
    assert!(matches!(manifest.status, SessionStatus::Committed));
}

#[tokio::test]
async fn commit_without_session_errors() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let err = commit(&ctx).await.unwrap_err();
    assert!(
        matches!(err, dkod_mcp::Error::NoActiveSession),
        "expected NoActiveSession, got {err:?}"
    );
}

#[tokio::test]
async fn commit_with_no_writes_returns_zero() {
    // Session is open but no `dkod_write_symbol` calls happened — every
    // group's `writes.jsonl` is empty, `commit_per_group` skips them all,
    // and HEAD does not move. Expect `commits_created: 0`, an empty SHA
    // list, but the manifest still transitions to Committed (the session
    // is finalized from the orchestrator's point of view).
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));

    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![],
                agent_prompt: "no-op".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");

    let resp = commit(&ctx).await.expect("commit");
    assert_eq!(resp.commits_created, 0);
    assert!(resp.commit_shas.is_empty());
    assert!(resp.dk_branch.starts_with("dk/"));

    let sid = ctx.active_session.lock().await.clone().unwrap();
    let manifest = Manifest::load(&ctx.paths, &sid).unwrap();
    assert!(matches!(manifest.status, SessionStatus::Committed));
}
