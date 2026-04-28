//! End-to-end smoke against the M5 auth sandbox.
//!
//! The auth sandbox under `bench/sandboxes/auth/` is a 4-module crate
//! whose call graph splits into three disjoint groups when scoped to
//! the public symbols. This test proves dkod-swarm can drive the full
//! plan → execute_begin → parallel writes → commit → pr flow against
//! a non-trivial Rust repo, not just the 4-fn tiny_rust fixture.
//!
//! `gh` + `git push` are PATH-shimmed (same pattern as
//! `tests/e2e_smoke.rs`); no GitHub credentials are touched.

#[path = "common/mod.rs"]
mod common;
use common::{init_tempo_repo, spawn_in_process_server};

use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Path of the auth sandbox relative to the workspace root. The test
/// resolves it via `CARGO_MANIFEST_DIR` so it runs the same regardless
/// of caller cwd.
const SANDBOX_REL: &str = "bench/sandboxes/auth";

/// Drop-guard that restores the previous `PATH` value when this test
/// exits. Mirrors the guard in `tests/e2e_smoke.rs`. The
/// `bench_sandbox_e2e` binary intentionally hosts only ONE
/// `#[tokio::test]` so the process-global `PATH` mutation cannot race a
/// sibling test.
struct PathGuard {
    saved: Option<std::ffi::OsString>,
}

impl PathGuard {
    fn install(prefix: &Path) -> Self {
        let saved = std::env::var_os("PATH");
        let saved_str = saved.as_deref().and_then(|p| p.to_str()).unwrap_or("");
        let new_path = format!("{}:{}", prefix.display(), saved_str);
        // SAFETY: `bench_sandbox_e2e` is the only test in this binary;
        // no sibling test races on PATH. Drop restores on every exit.
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

/// Drop a `gh` shim under `<root>/.bin/`. `pr list` returns empty (no
/// existing PR); `pr create` echoes the canned URL.
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

/// Drop a `git` shim alongside the `gh` shim. `git push` is
/// short-circuited (no real remote); everything else forwards to the
/// real binary via the absolute path resolved at shim-creation time so
/// the shim can never recurse into itself.
///
/// Each `git push` call appends a line to `bin_dir/.git_push_calls`
/// so the test can assert post-flow that `dkod_pr` actually invoked
/// `git push` instead of silently short-circuiting earlier. Returns
/// the path of that marker file.
fn install_git_shim(bin_dir: &Path) -> PathBuf {
    let real_git = which_git();
    let shim = bin_dir.join("git");
    let marker = bin_dir.join(".git_push_calls");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = "push" ]; then
    echo "push $*" >> {marker_q}
    exit 0
fi
exec {real} "$@"
"#,
        real = shell_quote(&real_git),
        marker_q = shell_quote(&marker.display().to_string()),
    );
    std::fs::write(&shim, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&shim).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&shim, perm).unwrap();
    }
    marker
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

/// Copy the auth sandbox into the tempdir AS the working tree, then
/// `git add -A` + amend the seed commit so the dk-branch creation in
/// `dkod_execute_begin` sees the auth sandbox source instead of the
/// tiny_rust fixture seeded by `init_tempo_repo`.
///
/// We amend (rather than commit afresh) so that git history still has
/// exactly one commit on `main`, matching the contract every other
/// in-process test relies on.
fn seed_auth_sandbox(root: &Path) {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src = workspace_root.join(SANDBOX_REL);

    // Copy every file in the sandbox into `root/`, preserving structure.
    fn copy_dir(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                copy_dir(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }
    copy_dir(&src, root);

    let s = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .status()
        .expect("git add");
    assert!(s.success(), "git add -A failed");
    let s = std::process::Command::new("git")
        .args(["commit", "-q", "--amend", "--no-edit", "--allow-empty"])
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .status()
        .expect("git commit --amend");
    assert!(s.success(), "git commit --amend failed");
}

/// Wrapper around `client.call_tool` that grabs the first text content
/// block and parses it as JSON. Mirrors the helper in
/// `tests/e2e_smoke.rs`.
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auth_sandbox_full_plan_to_pr() {
    let (_tmp, root) = init_tempo_repo();
    seed_auth_sandbox(&root);

    let bin_dir = make_gh_shim(&root, "https://github.com/fake/auth-sandbox/pull/1");
    let git_push_marker = install_git_shim(&bin_dir);
    let _path_guard = PathGuard::install(&bin_dir);

    let client = spawn_in_process_server(&root).await;

    // 1. plan — request 3 groups; expect at least 3 to come back. The
    //    parser uses bare function names as `qualified_name` (verified
    //    empirically — see `extract_symbols.rs` test for the dk-engine
    //    parser semantics), so `in_scope` lists bare names, not
    //    `module::name` paths.
    let plan = call_tool_json(
        &client,
        "dkod_plan",
        json!({
            "task_prompt": "Switch from password login to passkeys",
            "in_scope": [
                "login", "validate_creds",
                "logout", "clear_session",
                "create_session", "destroy_session", "touch",
                "passkey_register", "passkey_verify",
            ],
            "files": [
                "src/login.rs",
                "src/logout.rs",
                "src/session.rs",
                "src/passkey.rs",
            ],
            "target_groups": 3,
        }),
    )
    .await;
    let groups = plan["groups"]
        .as_array()
        .expect("plan.groups array")
        .clone();
    assert!(
        groups.len() >= 3,
        "expected >= 3 partition groups, got {}: {plan:?}",
        groups.len()
    );

    // 2. execute_begin — feed the planner's groups back as input. Each
    //    input group needs an `agent_prompt`.
    let begin_groups: Vec<Value> = groups
        .iter()
        .map(|g| {
            json!({
                "id": g["id"],
                "symbols": g["symbols"],
                "agent_prompt": "rewrite in passkey terms",
            })
        })
        .collect();
    let begin = call_tool_json(
        &client,
        "dkod_execute_begin",
        json!({
            "task_prompt": "Switch from password login to passkeys",
            "groups": begin_groups,
        }),
    )
    .await;
    let session_id = begin["session_id"]
        .as_str()
        .expect("session_id string")
        .to_string();
    assert!(
        session_id.starts_with("sess-"),
        "session_id should start with sess-, got {session_id:?}"
    );

    // 3. parallel write_symbol — pick one symbol from each of the first
    //    two groups and rewrite both concurrently. Asserts the per-file
    //    locking behaves on a non-trivial repo.
    // The planner returns absolute file paths (it canonicalises via
    // `resolve_under_repo`), but `dkod_write_symbol` requires repo-
    // relative paths. Convert by stripping the canonical repo root.
    // `init_tempo_repo` returns `root` already canonicalised on Linux,
    // but on macOS `tempfile::tempdir()` lives under `/var/...` and
    // `resolve_under_repo` canonicalises through the `/private/var`
    // alias — match the planner's canonicalisation here too.
    let canon_root = std::fs::canonicalize(&root).expect("canonicalize root");
    let to_rel = |abs: &str| -> String {
        let p = Path::new(abs);
        p.strip_prefix(&canon_root)
            .unwrap_or_else(|_| panic!("{abs} not under {canon_root:?}"))
            .to_string_lossy()
            .into_owned()
    };

    let g0 = &groups[0];
    let g1 = &groups[1];
    let g0_first_sym = g0["symbols"][0]["qualified_name"]
        .as_str()
        .expect("g0 symbol qualified_name");
    let g0_first_file = to_rel(
        g0["symbols"][0]["file_path"]
            .as_str()
            .expect("g0 symbol file_path"),
    );
    let g1_first_sym = g1["symbols"][0]["qualified_name"]
        .as_str()
        .expect("g1 symbol qualified_name");
    let g1_first_file = to_rel(
        g1["symbols"][0]["file_path"]
            .as_str()
            .expect("g1 symbol file_path"),
    );

    // qualified_name from the planner is the bare function name. Use
    // the same name for the rewritten body so `replace_symbol`'s
    // tier-1 (exact qualified_name match) re-parse succeeds.
    let g0_short = g0_first_sym.rsplit("::").next().unwrap();
    let g1_short = g1_first_sym.rsplit("::").next().unwrap();

    let w0 = call_tool_json(
        &client,
        "dkod_write_symbol",
        json!({
            "group_id": g0["id"],
            "file": &g0_first_file,
            "qualified_name": g0_first_sym,
            "new_body": format!(
                "pub fn {g0_short}(_x: &str) -> Option<String> {{ /* MARK_G0 */ Some(\"x\".into()) }}"
            ),
        }),
    );
    let w1 = call_tool_json(
        &client,
        "dkod_write_symbol",
        json!({
            "group_id": g1["id"],
            "file": &g1_first_file,
            "qualified_name": g1_first_sym,
            "new_body": format!(
                "pub fn {g1_short}(_x: &str) -> String {{ /* MARK_G1 */ String::new() }}"
            ),
        }),
    );
    let (wa, wb) = tokio::join!(w0, w1);
    assert_eq!(wa["outcome"], "parsed_ok", "write g0 outcome: {wa}");
    assert_eq!(wb["outcome"], "parsed_ok", "write g1 outcome: {wb}");

    let g0_after = std::fs::read_to_string(root.join(&g0_first_file)).expect("read g0 file");
    let g1_after = std::fs::read_to_string(root.join(&g1_first_file)).expect("read g1 file");
    assert!(g0_after.contains("MARK_G0"), "MARK_G0 missing:\n{g0_after}");
    assert!(g1_after.contains("MARK_G1"), "MARK_G1 missing:\n{g1_after}");

    // 4. complete each group, commit, pr. Groups with no writes are
    //    skipped by `commit_per_group`, so only `commits_created >= 1`
    //    is asserted.
    for g in &groups {
        let resp = call_tool_json(
            &client,
            "dkod_execute_complete",
            json!({"group_id": g["id"], "summary": "done"}),
        )
        .await;
        assert_eq!(resp["new_status"], "done");
    }

    let commit = call_tool_json(&client, "dkod_commit", json!({})).await;
    let commits_created = commit["commits_created"]
        .as_u64()
        .expect("commits_created u64");
    assert!(
        commits_created >= 1,
        "expected >= 1 commit, got {commits_created}: {commit}"
    );

    let pr = call_tool_json(
        &client,
        "dkod_pr",
        json!({"title": "auth sandbox bench", "body": "M5 E2E"}),
    )
    .await;
    let url = pr["url"].as_str().expect("pr.url string");
    assert!(url.contains("/pull/"), "expected /pull/ URL, got {url:?}");

    // Confirm `dkod_pr` actually drove `git push` via the shim — the
    // shim writes a line to `git_push_marker` for each push call.
    // Without this assertion, a bug that silently skipped push (e.g.
    // a regression in `gh::push_branch`) would still let the test
    // pass via the gh shim's pr-create echo.
    let push_log = std::fs::read_to_string(&git_push_marker).unwrap_or_else(|e| {
        panic!(
            "expected git push marker at {}: {e}",
            git_push_marker.display()
        )
    });
    assert!(
        push_log.lines().any(|l| l.starts_with("push ")),
        "git_push_marker has no `push ...` line: {push_log:?}"
    );

    client.cancel().await.ok();
    drop(_path_guard);
}
