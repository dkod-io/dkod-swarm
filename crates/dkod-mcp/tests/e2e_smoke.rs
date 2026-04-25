//! End-to-end MCP smoke test — the final M2 deliverable.
//!
//! Drives the full dkod-swarm flow through the in-process rmcp client harness:
//! `dkod_plan` → `dkod_execute_begin` → two **parallel** `dkod_write_symbol`
//! calls (different symbols in the same `src/lib.rs`) → `dkod_execute_complete`
//! per group → `dkod_commit` → `dkod_pr` → `dkod_status`.
//!
//! Strategy:
//! - The server runs in-process via `spawn_in_process_server`. That means a
//!   one-shot `std::env::set_var("PATH", …)` at the top of the test makes the
//!   shimmed `gh` / `git push` visible to the `Command::new("gh")` /
//!   `Command::new("git")` calls inside `dkod_pr` without going through
//!   `pr_with_shim`'s `path_prefix` plumbing (which is reserved for the
//!   `pr_tool.rs` direct-helper unit tests).
//! - `PATH` is restored at the end via a `Drop` guard so a panic in the test
//!   body cannot leak the shim into sibling tests.
//! - The `gh` shim distinguishes `pr list` (silent — no existing PR) from
//!   `pr create` (echo a canned URL). The `git` shim short-circuits `git push`
//!   and forwards everything else to the system `git` (the real binary is
//!   resolved at shim-creation time so the shim never re-enters itself).

#[path = "common/mod.rs"]
mod common;

use common::{init_tempo_repo, spawn_in_process_server};
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Wrapper around `client.call_tool` that grabs the first text content block
/// and parses it as JSON. Mirrors the `plan_tool_e2e.rs` / `write_symbol_e2e.rs`
/// pattern but factored out so this file's seven sequential calls don't
/// repeat the same boilerplate.
async fn call_tool_json(
    client: &rmcp::service::RunningService<
        rmcp::RoleClient,
        Box<dyn rmcp::service::DynService<rmcp::RoleClient>>,
    >,
    name: &'static str,
    args: Value,
) -> Value {
    let obj = args
        .as_object()
        .expect("json! macro produces an object")
        .clone();
    let result = client
        .call_tool(CallToolRequestParams::new(name).with_arguments(obj))
        .await
        .unwrap_or_else(|e| panic!("call_tool({name}) failed: {e:?}"));
    let content = result
        .content
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("call_tool({name}) returned no content"));
    let text = content
        .raw
        .as_text()
        .unwrap_or_else(|| panic!("call_tool({name}) returned non-text content"))
        .text
        .clone();
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("call_tool({name}) returned non-JSON text: {e}; raw={text}"))
}

/// Drop-guard that restores the previous `PATH` value when this test exits.
/// `set_var` is `unsafe` from edition 2024 onward (it mutates global process
/// state); the guard makes the unsafe block tightly scoped to setup/teardown.
///
/// **Concurrency invariant.** `PathGuard` is process-global. The `e2e_smoke`
/// test binary intentionally contains exactly ONE `#[tokio::test]`
/// (`full_plan_to_pr_flow`) — Rust tests in the same binary can run in
/// parallel by default, so installing this guard concurrently from a sibling
/// test would race. If you ever add a second test to this file, either
/// (a) move it to its own `tests/*.rs` file (each test binary has its own
/// process), (b) add the `serial_test` crate and annotate both with
/// `#[serial]`, or (c) document the constraint and run with
/// `RUST_TEST_THREADS=1` in CI. None of those is needed today.
struct PathGuard {
    saved: Option<std::ffi::OsString>,
}

impl PathGuard {
    fn install(prefix: &Path) -> Self {
        let saved = std::env::var_os("PATH");
        let saved_str = saved.as_deref().and_then(|p| p.to_str()).unwrap_or("");
        let new_path = format!("{}:{}", prefix.display(), saved_str);
        // SAFETY: tests in this file run in their own crate-local test binary,
        // and this is the only test in the file. The Drop impl restores the
        // saved value on every exit path (including panic).
        unsafe { std::env::set_var("PATH", new_path) };
        Self { saved }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        // SAFETY: see `install`.
        match self.saved.take() {
            Some(prev) => unsafe { std::env::set_var("PATH", prev) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }
}

/// Drop a `gh` shim under `<root>/.bin/` whose `pr list` returns empty (no
/// existing PR) and whose `pr create` echoes `url`. Mirrors the simpler
/// shim shape from the plan's Task 25 sketch.
fn make_gh_shim(root: &Path, url: &str) -> PathBuf {
    let bin_dir = root.join(".bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("gh");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
    exit 0
elif [ "$1" = "pr" ] && [ "$2" = "create" ]; then
    echo "{url}"
    exit 0
fi
echo "shim: unhandled gh args: $*" >&2
exit 99
"#
    );
    std::fs::write(&shim, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&shim).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&shim, perm).unwrap();
    }
    bin_dir
}

/// Drop a `git` shim alongside the `gh` shim under the same `bin_dir`. The
/// `git push` subcommand is short-circuited (the in-memory test repo has no
/// `origin` remote); every other `git` invocation forwards to the real binary
/// via the absolute path resolved at shim-creation time, so the shim cannot
/// recurse into itself even though `bin_dir` is at the front of `PATH`.
fn install_git_shim(bin_dir: &Path) {
    let real_git = which_git();
    let shim = bin_dir.join("git");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = "push" ]; then
    exit 0
fi
exec {real} "$@"
"#,
        real = shell_quote(&real_git),
    );
    std::fs::write(&shim, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&shim).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&shim, perm).unwrap();
    }
}

fn which_git() -> String {
    let out = std::process::Command::new("which")
        .arg("git")
        .output()
        .expect("which git");
    if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        "/usr/bin/git".to_string()
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_plan_to_pr_flow() {
    let (_tmp, root) = init_tempo_repo();

    // 0. Install gh + git shims and prepend the bin dir to PATH for the
    //    duration of this test. The in-process server inherits the same
    //    process env, so its `Command::new("gh")` / `Command::new("git")`
    //    pick up the shims without us threading `path_prefix` through.
    let bin_dir = make_gh_shim(&root, "https://github.com/fake/repo/pull/1");
    install_git_shim(&bin_dir);
    let _path_guard = PathGuard::install(&bin_dir);

    let client = spawn_in_process_server(&root).await;

    // 1. plan — partition the tiny_rust fixture into two groups.
    let plan = call_tool_json(
        &client,
        "dkod_plan",
        json!({
            "task_prompt": "demo",
            "in_scope": ["a", "b", "c", "d"],
            "files": ["src/lib.rs"],
            "target_groups": 2,
        }),
    )
    .await;
    let groups = plan["groups"]
        .as_array()
        .expect("plan.groups array")
        .clone();
    assert_eq!(
        groups.len(),
        2,
        "tiny_rust partitions into two groups: {{a,b,c}} and {{d}}"
    );

    // Find which group owns symbol `a` and which owns `b`. The planner is
    // free to pick its own order, so we don't hardcode group[0] vs group[1].
    let group_for = |sym: &str| -> &Value {
        groups
            .iter()
            .find(|g| {
                g["symbols"]
                    .as_array()
                    .map(|arr| arr.iter().any(|s| s["qualified_name"] == sym))
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("no group contains symbol `{sym}`"))
    };
    let g_a = group_for("a");
    let g_b = group_for("b");
    // tiny_rust groups symbols by call coupling — `a`, `b`, `c` are all coupled
    // through `c` calling `a` and `b`, so they land in the same group.
    assert_eq!(
        g_a["id"], g_b["id"],
        "expected `a` and `b` to share a group (call-coupled via `c`)"
    );
    let group_id_ab = g_a["id"].clone();

    // 2. execute_begin — feed the planner's groups back as input. Each input
    //    group needs an `agent_prompt` (the planner's `PlanGroup` doesn't
    //    carry one).
    let begin_groups: Vec<Value> = groups
        .iter()
        .map(|g| {
            json!({
                "id": g["id"],
                "symbols": g["symbols"],
                "agent_prompt": "rewrite",
            })
        })
        .collect();
    let begin = call_tool_json(
        &client,
        "dkod_execute_begin",
        json!({
            "task_prompt": "demo",
            "groups": begin_groups,
        }),
    )
    .await;
    let session_id = begin["session_id"]
        .as_str()
        .expect("session_id string")
        .to_string();
    assert!(!session_id.is_empty(), "session_id should not be empty");

    // 3. Two writes in parallel — `a` and `b` both live in `src/lib.rs`.
    //    `dkod_write_symbol` holds a per-file lock, so the actual disk
    //    writes serialise; `tokio::join!` proves the *callers* can fire
    //    concurrently and both succeed.
    let w_a = call_tool_json(
        &client,
        "dkod_write_symbol",
        json!({
            "group_id": group_id_ab,
            "file": "src/lib.rs",
            "qualified_name": "a",
            "new_body": "pub fn a() { /* MARK_A */ }",
        }),
    );
    let w_b = call_tool_json(
        &client,
        "dkod_write_symbol",
        json!({
            "group_id": group_id_ab,
            "file": "src/lib.rs",
            "qualified_name": "b",
            "new_body": "pub fn b() { /* MARK_B */ }",
        }),
    );
    let (wa, wb) = tokio::join!(w_a, w_b);
    assert_eq!(wa["outcome"], "parsed_ok", "write `a` outcome: {wa}");
    assert_eq!(wb["outcome"], "parsed_ok", "write `b` outcome: {wb}");

    // Both markers must end up in the final file content. This is the
    // load-bearing assertion that proves the AST-level write composition
    // didn't lose either edit when they raced.
    let src = std::fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    assert!(src.contains("MARK_A"), "MARK_A missing:\n{src}");
    assert!(src.contains("MARK_B"), "MARK_B missing:\n{src}");

    // 4. complete each group — `dkod_execute_complete` flips the group
    //    status from `in_progress` to `done`.
    for g in &groups {
        let resp = call_tool_json(
            &client,
            "dkod_execute_complete",
            json!({
                "group_id": g["id"],
                "summary": "done",
            }),
        )
        .await;
        assert_eq!(resp["new_status"], "done");
    }

    // 5. commit — at least one commit (the group containing `a`+`b` had
    //    writes; the `d`-only group has none so `commit_per_group` skips it).
    let commit = call_tool_json(&client, "dkod_commit", json!({})).await;
    let commits_created = commit["commits_created"]
        .as_u64()
        .expect("commits_created u64");
    assert!(
        commits_created >= 1,
        "expected ≥ 1 commit, got {commits_created}: {commit}"
    );

    // 6. status (post-commit, pre-pr) — every group should be `done` and
    //    the active session should still match the one `execute_begin`
    //    minted. We assert here (not after `dkod_pr`) because a successful
    //    `dkod_pr` clears `active_session` per `pr_with_shim`'s post-success
    //    cleanup, after which `dkod_status` short-circuits to an empty
    //    response (see `tools/status.rs`).
    let st = call_tool_json(&client, "dkod_status", json!({})).await;
    assert_eq!(
        st["active_session_id"].as_str(),
        Some(session_id.as_str()),
        "active_session_id should match the session begin minted: {st}"
    );
    let status_groups = st["groups"].as_array().expect("status.groups array");
    assert_eq!(
        status_groups.len(),
        groups.len(),
        "status.groups count mismatch: {st}"
    );
    assert!(
        status_groups.iter().all(|g| g["status"] == "done"),
        "all groups should be `done`: {st}"
    );

    // 7. pr — the gh shim returns the canned URL; the git shim swallows
    //    `git push`. PATH is set via the guard above so the in-process
    //    `dkod_pr` picks up both shims.
    let pr = call_tool_json(
        &client,
        "dkod_pr",
        json!({
            "title": "demo PR",
            "body": "demo body",
        }),
    )
    .await;
    let url = pr["url"].as_str().expect("pr.url string");
    assert!(url.contains("/pull/"), "expected a /pull/ URL, got {url:?}");

    client.cancel().await.ok();
}
