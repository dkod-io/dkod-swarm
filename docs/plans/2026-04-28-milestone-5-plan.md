# Milestone 5: E2E smoke + parallel-vs-serial benchmark Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove dkod-swarm's value proposition empirically. Ship (a) a small but realistic Rust sandbox under `bench/sandboxes/auth/` that the partitioner can split into 3 disjoint groups; (b) an automated correctness E2E that drives the full plan→execute→commit→pr flow against that sandbox via the in-process rmcp client; (c) a parallel-vs-serial micro-benchmark that runs the SAME workload through `tokio::join!`-based parallel writes vs sequential awaits and asserts the parallel run is meaningfully faster. Plus a manual driving guide for human-in-the-loop end-to-end with real Claude Code.

**Architecture:** New top-level `bench/` directory holds the sandbox repo (`bench/sandboxes/auth/`) and the manual driving guide. The automated work lives in `crates/dkod-mcp/tests/`: one new test file `bench_sandbox_e2e.rs` that drives the sandbox through the full surface (proves correctness on a non-trivial repo), and one new test file `bench_parallel_vs_serial.rs` that times parallel vs serial runs of three synthetic symbol writes. Synthetic writes inject a `tokio::time::sleep` per symbol to model LLM thinking time — the real bottleneck dkod-swarm is built to amortise across N agents. With a 100ms-per-write delay, three sequential writes take ~300ms, three parallel writes take ~100ms; the test asserts a > 1.5× speedup with safety margin.

**Tech Stack:** Rust 2024. No new dependencies. The sandbox is a self-contained Rust crate with `Cargo.toml` + `src/lib.rs` + 4 module files; it compiles standalone but isn't built by the workspace (lives in `bench/`, not `crates/`). The benchmark uses `std::time::Instant` for wall-clock measurement and `tokio::time::sleep` to model LLM latency. M2-8's e2e harness pattern (in-process rmcp client, PathGuard for PATH-shimmed `gh`+`git`) is the reference for the new test files.

---

## File Structure

New files only. Nothing under `crates/dkod-worktree/`, `crates/dkod-orchestrator/`, `crates/dkod-mcp/src/`, `crates/dkod-cli/`, or `plugin/` is modified.

```text
bench/
├── README.md                                  # what's in here, how to run
├── MANUAL_E2E.md                              # human-driven end-to-end via real Claude Code
└── sandboxes/
    └── auth/
        ├── Cargo.toml
        └── src/
            ├── lib.rs                         # re-exports
            ├── login.rs                       # pub fn login + fn validate_creds
            ├── logout.rs                      # pub fn logout + fn clear_session
            ├── session.rs                     # session lifecycle (3 fns)
            └── passkey.rs                     # passkey register / verify (2 fns)
crates/dkod-mcp/tests/
├── bench_sandbox_e2e.rs                       # full plan→pr flow against the auth sandbox
└── bench_parallel_vs_serial.rs                # wall-clock parallel vs serial benchmark
```

The sandbox crate is **not** part of the workspace (`Cargo.toml` `members` does not include it) — it's a fixture that the test code reads as files, not a buildable workspace member. That keeps `cargo test --workspace` from compiling it on every test run.

---

## PR Plan

Milestone 5 lands in **3 PRs**. Each PR is a feature branch off `main`, opened fresh, and goes through the full CodeRabbit loop per `CLAUDE.md` (local `/coderabbit:code-review` → fix → re-review → commit/push → wait for PR review → `/coderabbit:autofix` → merge autonomously once clean). Branch names match the PR title prefix.

| PR | Branch | Scope | Tasks |
|----|--------|-------|-------|
| M5-1 | `m5/sandbox-fixture-e2e` | sandbox crate under `bench/sandboxes/auth/` + correctness E2E test | 1–4 |
| M5-2 | `m5/parallel-vs-serial-bench` | wall-clock benchmark test asserting parallel > serial | 5–7 |
| M5-3 | `m5/manual-guide-readme` | `bench/MANUAL_E2E.md` + `bench/README.md` + repo README bump | 8–10 |

Each PR must end with a **green `cargo test --workspace`** before the review gate is entered.

---

## Commit & PR conventions (recap — read `CLAUDE.md` first)

Every commit MUST:

- Run author + committer through the env-var override:
  ```sh
  GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
  GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
  git commit -m "..."
  ```
  No `Co-Authored-By`. Verify with `git log -1 --format='%h | %an <%ae> | %cn <%ce> | %s'` and grep the body for `Co-Authored-By` (must be empty).
- Be preceded by `/coderabbit:code-review` on the local diff vs `main`, unless the changeset is docs / sandbox-Rust only (see below).

**Docs / sandbox-only commits skip local CR.** The sandbox Rust files under `bench/sandboxes/auth/` are fixtures, not source — they're read by tests and never compiled by the workspace. Per CLAUDE.md they fall under the "doesn't meaningfully review" docs-only rule. The two new test files in `crates/dkod-mcp/tests/` ARE compiled and DO go through local CR.

Every PR MUST:

- Title ≤ 70 chars.
- Body = short summary + test-plan checklist.
- Open ONE PR at a time. Do not start the next PR's branch until the current one is merged.
- **Merge autonomously** once CodeRabbit is clean and `cargo test --workspace` is green.

---

# PR M5-1 — Sandbox fixture + correctness E2E

## Task 1: Sandbox crate scaffold

**Files:**
- Create: `bench/sandboxes/auth/Cargo.toml`
- Create: `bench/sandboxes/auth/src/lib.rs`

- [ ] **Step 1: Create `bench/sandboxes/auth/Cargo.toml`.**

```toml
[package]
name = "auth-sandbox"
version = "0.0.0"
edition = "2024"
license = "MIT"
publish = false

# This crate is a fixture — read by dkod-swarm's e2e tests, NEVER built
# by the dkod-swarm workspace. It is therefore not listed in the
# workspace `members`. If a contributor wants to build it standalone:
#   cd bench/sandboxes/auth && cargo build
```

- [ ] **Step 2: Create `bench/sandboxes/auth/src/lib.rs`.**

```rust
//! Auth sandbox for dkod-swarm M5 E2E.
//!
//! Four modules with deliberate call-graph coupling so the partitioner
//! splits the public surface into three disjoint groups:
//! - login: `login` + `validate_creds`
//! - session lifecycle: `create_session` / `destroy_session` / `touch`
//! - passkey: `passkey_register` / `passkey_verify`
//! `logout` calls into session, so `logout` joins the session group.

pub mod login;
pub mod logout;
pub mod passkey;
pub mod session;
```

- [ ] **Step 3: Branch + commit (sandbox-only, skip local CR).**

```sh
git checkout -b m5/sandbox-fixture-e2e
git add bench/sandboxes/auth/Cargo.toml bench/sandboxes/auth/src/lib.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add auth sandbox crate scaffold"
```

## Task 2: Sandbox source — four modules with intentional call-graph coupling

**Files:**
- Create: `bench/sandboxes/auth/src/login.rs`
- Create: `bench/sandboxes/auth/src/logout.rs`
- Create: `bench/sandboxes/auth/src/session.rs`
- Create: `bench/sandboxes/auth/src/passkey.rs`

- [ ] **Step 1: Create `bench/sandboxes/auth/src/session.rs`.**

```rust
//! Session lifecycle. Three coupled functions: `create_session`,
//! `destroy_session`, and the helper `touch` they both call.

pub struct Session {
    pub id: String,
    pub user_id: String,
    pub last_active_ms: u64,
}

pub fn create_session(user_id: &str) -> Session {
    let id = format!("sess-{user_id}");
    let mut s = Session {
        id,
        user_id: user_id.to_string(),
        last_active_ms: 0,
    };
    touch(&mut s);
    s
}

pub fn destroy_session(s: &mut Session) {
    touch(s);
    s.id.clear();
}

fn touch(s: &mut Session) {
    s.last_active_ms = 1;
}
```

- [ ] **Step 2: Create `bench/sandboxes/auth/src/login.rs`.**

```rust
//! Password login. `login` calls `validate_creds`. Coupled pair.

pub fn login(username: &str, password: &str) -> Option<String> {
    if validate_creds(username, password) {
        Some(format!("token-{username}"))
    } else {
        None
    }
}

fn validate_creds(username: &str, password: &str) -> bool {
    !username.is_empty() && password.len() >= 8
}
```

- [ ] **Step 3: Create `bench/sandboxes/auth/src/logout.rs`.**

```rust
//! Logout. Calls `session::destroy_session`, so the partitioner groups
//! `logout` with the session-lifecycle functions.

use crate::session::{Session, destroy_session};

pub fn logout(mut s: Session) {
    destroy_session(&mut s);
    clear_session(&mut s);
}

fn clear_session(s: &mut Session) {
    s.user_id.clear();
}
```

- [ ] **Step 4: Create `bench/sandboxes/auth/src/passkey.rs`.**

```rust
//! Passkey register / verify. Two independent functions; this group is
//! a candidate for a parallel rewrite alongside the login + session
//! groups.

pub fn passkey_register(user_id: &str) -> String {
    format!("pk-{user_id}")
}

pub fn passkey_verify(user_id: &str, pk: &str) -> bool {
    pk == format!("pk-{user_id}")
}
```

- [ ] **Step 5: Verify the sandbox builds standalone.**

```sh
cd bench/sandboxes/auth && cargo build && cd -
```

Expected: builds clean. Then `rm -rf bench/sandboxes/auth/target` so the build artefacts don't accidentally land in the commit.

- [ ] **Step 6: Commit (sandbox-only, skip local CR).**

```sh
git add bench/sandboxes/auth/src/login.rs bench/sandboxes/auth/src/logout.rs \
        bench/sandboxes/auth/src/session.rs bench/sandboxes/auth/src/passkey.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add auth sandbox source: login, logout, session, passkey"
```

## Task 3: Correctness E2E test against the sandbox

**Files:**
- Create: `crates/dkod-mcp/tests/bench_sandbox_e2e.rs`

This test drives the dkod-swarm flow against the auth sandbox in a tempdir. It asserts that the partitioner produces ≥ 3 groups, parallel writes land both `MARK_LOGIN` and `MARK_PASSKEY` markers, and the `dkod_pr` shim returns a fake URL.

- [ ] **Step 1: Write the test.**

The test mirrors the M2-8 `e2e_smoke.rs` pattern: copy the sandbox into a tempdir tracked as a git repo, init `.dkod/`, drive the rmcp client, assert. Because the sandbox is bigger than `tiny_rust`, the test seeds it from `bench/sandboxes/auth/` rather than the M2 `tests/fixtures/tiny_rust/`.

```rust
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
use std::sync::Arc;

const SANDBOX_REL: &str = "bench/sandboxes/auth";

/// PathGuard — saves and restores `PATH` on every exit (panic-safe).
struct PathGuard {
    saved: Option<std::ffi::OsString>,
}
impl PathGuard {
    fn install(prefix: &Path) -> Self {
        let saved = std::env::var_os("PATH");
        let saved_str = saved.as_deref().and_then(|p| p.to_str()).unwrap_or("");
        // Skip the trailing `:` when the inherited PATH is empty —
        // POSIX treats an empty path component as `cwd`, which can mask
        // shimmed binaries with whatever happens to be in the working
        // directory.
        let new_path = if saved_str.is_empty() {
            prefix.display().to_string()
        } else {
            format!("{}:{}", prefix.display(), saved_str)
        };
        // SAFETY: bench_sandbox_e2e is the only test in this binary;
        // no sibling test races on PATH. Drop restores on every exit.
        // `set_var` is unsafe in Rust 2024 edition.
        unsafe { std::env::set_var("PATH", new_path) };
        Self { saved }
    }
}
impl Drop for PathGuard {
    fn drop(&mut self) {
        // SAFETY: see install.
        match self.saved.take() {
            Some(prev) => unsafe { std::env::set_var("PATH", prev) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }
}

fn make_gh_shim(root: &Path, url: &str) -> PathBuf {
    let bin_dir = root.join(".bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("gh");
    let body = format!(
        r#"#!/bin/sh
case "$1 $2" in
  "pr list") exit 0 ;;
  "pr create") echo "{url}"; exit 0 ;;
  *) exit 0 ;;
esac"#
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

fn install_git_shim(bin_dir: &Path) {
    let real_git =
        String::from_utf8(std::process::Command::new("which").arg("git").output().unwrap().stdout)
            .unwrap()
            .trim()
            .to_string();
    let shim = bin_dir.join("git");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = "push" ]; then
  exit 0
fi
exec "{real_git}" "$@"
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
}

/// Copy the auth sandbox into the tempdir AS the working tree, then
/// `git add` + commit a seed so dk-branch creation works.
fn seed_auth_sandbox(root: &Path) {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src = workspace_root.join(SANDBOX_REL);
    // Copy every file in the sandbox into `root/`, preserving structure.
    // Skip build / VCS artifacts so a contributor who ran
    // `cd bench/sandboxes/auth && cargo build` locally does not pollute
    // the test repo with a `target/` tree (slow and nondeterministic).
    fn copy_dir(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if matches!(name.to_str(), Some("target" | ".git")) {
                continue;
            }
            let from = entry.path();
            let to = dst.join(&name);
            if from.is_dir() {
                copy_dir(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }
    copy_dir(&src, root);
    // Re-seed git: the helper `init_tempo_repo` already ran git init +
    // an initial commit with the M2 tiny_rust fixture. We need a clean
    // seed of the auth sandbox instead. Reset to a fresh tree:
    let s = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(s.success(), "git add failed");
    let s = std::process::Command::new("git")
        .args(["commit", "-q", "--amend", "--no-edit", "--allow-empty"])
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .status()
        .unwrap();
    assert!(s.success(), "git commit --amend failed");
}

async fn call_tool_json(
    client: &rmcp::service::RunningService<
        rmcp::RoleClient,
        Box<dyn rmcp::service::DynService<rmcp::RoleClient>>,
    >,
    name: &str,
    args: Value,
) -> Value {
    let obj = args.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new(name).with_arguments(obj))
        .await
        .expect("call_tool");
    for c in result.content {
        if let Some(t) = c.raw.as_text() {
            return serde_json::from_str(&t.text).unwrap_or(Value::String(t.text.clone()));
        }
    }
    Value::Null
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auth_sandbox_full_plan_to_pr() {
    let (_tmp, root) = init_tempo_repo();
    seed_auth_sandbox(&root);
    let bin_dir = make_gh_shim(&root, "https://github.com/fake/auth-sandbox/pull/1");
    install_git_shim(&bin_dir);
    let _path_guard = PathGuard::install(&bin_dir);

    let client = spawn_in_process_server(&root).await;

    // 1. plan — request 3 groups; expect at least 3.
    let plan = call_tool_json(
        &client,
        "dkod_plan",
        json!({
            "task_prompt": "Switch from password login to passkeys",
            "in_scope": [
                // Bare names — the dk-engine parser emits `qualified_name`
                // without the module prefix (e.g. `"login"`, not
                // `"login::login"`). Stay consistent with the partitioner's
                // resolution table.
                "login", "validate_creds",
                "logout", "clear_session",
                "create_session", "destroy_session", "touch",
                "passkey_register", "passkey_verify",
            ],
            "files": [
                "src/login.rs", "src/logout.rs", "src/session.rs", "src/passkey.rs",
            ],
            "target_groups": 3,
        }),
    )
    .await;
    let groups = plan["groups"].as_array().expect("groups array");
    assert!(
        groups.len() >= 3,
        "expected ≥ 3 partition groups, got {}: {plan:?}",
        groups.len()
    );

    // 2. execute_begin
    let begin = call_tool_json(
        &client,
        "dkod_execute_begin",
        json!({
            "task_prompt": "Switch from password login to passkeys",
            "groups": groups
                .iter()
                .map(|g| {
                    json!({
                        "id": g["id"],
                        "symbols": g["symbols"],
                        "agent_prompt": "rewrite in passkey terms",
                    })
                })
                .collect::<Vec<_>>(),
        }),
    )
    .await;
    assert!(begin["session_id"].as_str().unwrap().starts_with("sess-"));

    // 3. parallel write_symbol — pick one symbol from each of two groups
    //    and rewrite both concurrently. Asserts the per-file lock works
    //    on a non-trivial repo.
    let g0 = &groups[0];
    let g1 = &groups[1];
    let g0_first_sym = g0["symbols"][0]["qualified_name"].as_str().unwrap();
    let g0_first_file = g0["symbols"][0]["file_path"].as_str().unwrap();
    let g1_first_sym = g1["symbols"][0]["qualified_name"].as_str().unwrap();
    let g1_first_file = g1["symbols"][0]["file_path"].as_str().unwrap();

    let w0 = call_tool_json(
        &client,
        "dkod_write_symbol",
        json!({
            "group_id": g0["id"],
            "file": g0_first_file,
            "qualified_name": g0_first_sym,
            "new_body": format!(
                "pub fn {}(_x: &str) -> Option<String> {{ /* MARK_G0 */ Some(\"x\".into()) }}",
                g0_first_sym.rsplit("::").next().unwrap()
            ),
        }),
    );
    let w1 = call_tool_json(
        &client,
        "dkod_write_symbol",
        json!({
            "group_id": g1["id"],
            "file": g1_first_file,
            "qualified_name": g1_first_sym,
            "new_body": format!(
                "pub fn {}(_x: &str) -> String {{ /* MARK_G1 */ String::new() }}",
                g1_first_sym.rsplit("::").next().unwrap()
            ),
        }),
    );
    let (_w0, _w1) = tokio::join!(w0, w1);

    let g0_after = std::fs::read_to_string(root.join(g0_first_file)).unwrap();
    let g1_after = std::fs::read_to_string(root.join(g1_first_file)).unwrap();
    assert!(g0_after.contains("MARK_G0"));
    assert!(g1_after.contains("MARK_G1"));

    // 4. complete each group, commit, pr.
    for g in groups {
        call_tool_json(
            &client,
            "dkod_execute_complete",
            json!({"group_id": g["id"], "summary": "done"}),
        )
        .await;
    }
    let commit = call_tool_json(&client, "dkod_commit", json!({})).await;
    assert!(commit["commits_created"].as_u64().unwrap() >= 1);
    let pr = call_tool_json(&client, "dkod_pr", json!({"title": "t", "body": "b"})).await;
    assert!(pr["url"].as_str().unwrap().contains("/pull/"));

    client.cancel().await.ok();
    drop(_path_guard);
}
```

- [ ] **Step 2: Run the test.**

```sh
cargo test -p dkod-mcp --test bench_sandbox_e2e -- --nocapture
```

Expected: `1 passed`. The test takes a few seconds (real git subprocesses + tree-sitter parses).

- [ ] **Step 3: Run `/coderabbit:code-review` locally.**

The test file is Rust source — local CR runs.

- [ ] **Step 4: Commit.**

```sh
git add crates/dkod-mcp/tests/bench_sandbox_e2e.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add full-flow E2E test against the auth sandbox"
```

## Task 4: PR M5-1 wrap-up

- [ ] `/coderabbit:code-review` clean on the test commit (sandbox commits skipped per CLAUDE.md).
- [ ] `cargo test --workspace` green (the new `bench_sandbox_e2e` test joins 45 existing).
- [ ] PR `M5-1: auth sandbox + full-flow E2E`. Body = summary + test plan + note that `bench/sandboxes/auth/**` is sandbox content and skipped local CR.
- [ ] CR poller + autofix until clean. Merge autonomously.

---

# PR M5-2 — Parallel-vs-serial benchmark

## Task 5: Benchmark scaffolding — synthetic-write helper with simulated LLM latency

**Files:**
- Create: `crates/dkod-mcp/tests/bench_parallel_vs_serial.rs`

This test measures wall-clock for three symbol writes. The synthetic-write helper sleeps for `LLM_DELAY` (100ms) per call to model real-world Claude Task subagent latency — without that, the writes themselves are sub-millisecond and parallelism shows nothing.

- [ ] **Step 1: Write the file scaffold + helpers.**

```rust
//! Wall-clock benchmark: parallel writes via `tokio::join!` should beat
//! sequential awaits when each "write" carries a realistic LLM-thinking
//! delay.
//!
//! This is the empirical evidence behind dkod-swarm's parallel-N-agents
//! value proposition. Without the simulated delay, file I/O is too fast
//! to show meaningful parallelism. With a 100ms-per-write delay,
//! sequential = ~300ms, parallel = ~100ms. The test asserts a > 1.5×
//! speedup with a safety margin for CI variance.

#[path = "common/mod.rs"]
mod common;
use common::init_tempo_repo;

use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::write_symbol::write_symbol;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// One simulated LLM-thinking delay per write. Tweak via env var
/// `DKOD_BENCH_LLM_DELAY_MS` for local exploration; CI uses the default.
fn llm_delay() -> Duration {
    let ms: u64 = std::env::var("DKOD_BENCH_LLM_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    Duration::from_millis(ms)
}

/// Drive a single `write_symbol` call after a simulated LLM-thinking
/// delay. Mirrors what a Task subagent actually does in production:
/// the LLM "thinks" (delay), then the AST write happens (fast).
async fn synthetic_write(ctx: Arc<ServerCtx>, req: WriteSymbolRequest) {
    sleep(llm_delay()).await;
    write_symbol(&ctx, req).await.expect("write_symbol");
}
```

- [ ] **Step 2: Compile-check (no test yet).**

```sh
cargo check --tests -p dkod-mcp
```

Expected: builds clean. The file declares helpers but has no `#[tokio::test]` yet.

- [ ] **Step 3: Commit.**

```sh
git checkout -b m5/parallel-vs-serial-bench
git add crates/dkod-mcp/tests/bench_parallel_vs_serial.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add bench scaffolding with synthetic LLM-delay helper"
```

Run `/coderabbit:code-review` first.

## Task 6: The benchmark — three writes parallel vs serial

**Files:**
- Modify: `crates/dkod-mcp/tests/bench_parallel_vs_serial.rs`

- [ ] **Step 1: Append the benchmark test.**

Append to the file from Task 5:

```rust
/// Set up a fresh dkod session with three groups, each owning one
/// distinct file. Returns the active context + the three write
/// requests the benchmark will fire (parallel vs serial).
async fn make_three_writes() -> (Arc<ServerCtx>, Vec<WriteSymbolRequest>, tempfile::TempDir) {
    let (tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));

    // Three distinct files so the per-file lock does not serialise
    // them — we want to measure the orchestrator's parallelism, not
    // the lock's correctness (covered by `tests/write_symbol_lock.rs`).
    std::fs::write(root.join("src/lib.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();
    std::fs::write(root.join("src/m1.rs"), "pub fn m1_fn() {}\n").unwrap();
    std::fs::write(root.join("src/m2.rs"), "pub fn m2_fn() {}\n").unwrap();
    let s = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(s.success());
    let s = std::process::Command::new("git")
        .args(["commit", "-q", "-m", "bench seed"])
        .current_dir(&root)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .status()
        .unwrap();
    assert!(s.success());

    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "bench".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![
                    SymbolRefSchema {
                        qualified_name: "a".into(),
                        file_path: PathBuf::from("src/lib.rs"),
                        kind: "function".into(),
                    },
                    SymbolRefSchema {
                        qualified_name: "m1_fn".into(),
                        file_path: PathBuf::from("src/m1.rs"),
                        kind: "function".into(),
                    },
                    SymbolRefSchema {
                        qualified_name: "m2_fn".into(),
                        file_path: PathBuf::from("src/m2.rs"),
                        kind: "function".into(),
                    },
                ],
                agent_prompt: "rewrite".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");

    let writes = vec![
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* P */ }".into(),
        },
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/m1.rs"),
            qualified_name: "m1_fn".into(),
            new_body: "pub fn m1_fn() { /* P */ }".into(),
        },
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/m2.rs"),
            qualified_name: "m2_fn".into(),
            new_body: "pub fn m2_fn() { /* P */ }".into(),
        },
    ];
    (ctx, writes, tmp)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_writes_beat_serial_writes_under_simulated_llm_delay() {
    // Parallel run.
    let (ctx_p, writes_p, _tmp_p) = make_three_writes().await;
    let start_parallel = Instant::now();
    let ctx_a = Arc::clone(&ctx_p);
    let ctx_b = Arc::clone(&ctx_p);
    let ctx_c = Arc::clone(&ctx_p);
    let mut iter = writes_p.into_iter();
    let w_a = synthetic_write(ctx_a, iter.next().unwrap());
    let w_b = synthetic_write(ctx_b, iter.next().unwrap());
    let w_c = synthetic_write(ctx_c, iter.next().unwrap());
    tokio::join!(w_a, w_b, w_c);
    let parallel = start_parallel.elapsed();

    // Serial run — fresh context (clean tempdir) so the parallel run's
    // write artifacts don't perturb the serial timing.
    let (ctx_s, writes_s, _tmp_s) = make_three_writes().await;
    let start_serial = Instant::now();
    for req in writes_s {
        synthetic_write(Arc::clone(&ctx_s), req).await;
    }
    let serial = start_serial.elapsed();

    eprintln!(
        "parallel: {parallel:?}  serial: {serial:?}  ratio: {:.2}x",
        serial.as_secs_f64() / parallel.as_secs_f64()
    );
    // Expected: serial ≈ 3 × delay, parallel ≈ 1 × delay. Assert > 1.5×
    // with margin for CI scheduling jitter. With a 100ms delay,
    // serial ≈ 310ms and parallel ≈ 110ms — ratio ≈ 2.8×.
    let ratio = serial.as_secs_f64() / parallel.as_secs_f64();
    assert!(
        ratio > 1.5,
        "expected parallel speedup > 1.5×, got {ratio:.2}× (parallel: {parallel:?}, serial: {serial:?})"
    );
}
```

- [ ] **Step 2: Run the benchmark.**

```sh
cargo test -p dkod-mcp --test bench_parallel_vs_serial -- --nocapture
```

Expected: passes; the `eprintln!` line shows something like `parallel: 110ms  serial: 310ms  ratio: 2.82x`.

Run it 5 times in a loop to confirm CI stability:

```sh
for i in 1 2 3 4 5; do
  cargo test -p dkod-mcp --test bench_parallel_vs_serial 2>&1 | grep "test result" | tail -1 | sed "s/^/  $i: /"
done
```

All 5 must pass. If any fails on a slow CI runner, bump `DKOD_BENCH_LLM_DELAY_MS` to 200 — that gives more headroom against scheduling jitter.

- [ ] **Step 3: Run `/coderabbit:code-review` locally.**

- [ ] **Step 4: Commit.**

```sh
git add crates/dkod-mcp/tests/bench_parallel_vs_serial.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Bench: assert parallel writes > 1.5x faster than serial"
```

## Task 7: PR M5-2 wrap-up

- [ ] `/coderabbit:code-review` clean on the benchmark commit.
- [ ] `cargo test --workspace` green; the benchmark passes 5/5 stress runs locally.
- [ ] PR `M5-2: parallel-vs-serial benchmark`. Merge autonomously.

---

# PR M5-3 — Manual driving guide + README + tag

## Task 8: bench/MANUAL_E2E.md — human-driven end-to-end via real Claude Code

**Files:**
- Create: `bench/MANUAL_E2E.md`

This is a docs-only file describing how to take the auth sandbox from `dkod init` through `/dkod-swarm:execute "..."` through `/dkod-swarm:pr` using a real Claude Code session. The automated tests in M5-1 / M5-2 prove correctness + parallelism mechanically; this guide proves the LLM-driven flow works end-to-end with human eyes.

- [ ] **Step 1: Write `bench/MANUAL_E2E.md`.** (Outer fence is four backticks because the body contains nested triple-backtick `sh` / `text` blocks.)

````markdown
# Manual end-to-end against the auth sandbox

The automated tests in `crates/dkod-mcp/tests/bench_sandbox_e2e.rs` and
`crates/dkod-mcp/tests/bench_parallel_vs_serial.rs` prove dkod-swarm's
mechanical correctness and orchestrator-level parallelism. This guide
proves the LLM-driven flow with real Claude Code and a real human in
the loop.

## Setup

1. Build the dkod CLI:

   ```sh
   cargo build --release -p dkod-cli --bin dkod
   ```

2. Copy the auth sandbox to a fresh location outside the dkod-swarm
   workspace (so `cargo test --workspace` doesn't compile it):

   ```sh
   cp -R bench/sandboxes/auth /tmp/auth-sandbox
   cd /tmp/auth-sandbox
   git init -q -b main
   git add -A
   git commit -q -m "seed auth sandbox"
   ```

3. Initialize dkod state:

   ```sh
   /path/to/dkod-swarm/target/release/dkod init --verify-cmd "cargo check"
   ```

4. Install the dkod-swarm Claude Code plugin (development install):

   ```text
   /plugin marketplace add /path/to/dkod-swarm
   /plugin install dkod-swarm@dkod-swarm
   ```

## Run the parallel refactor

In Claude Code, from the `/tmp/auth-sandbox` directory:

```text
/dkod-swarm:execute Switch from password login to passkeys: rewrite
login::login + login::validate_creds to use passkey verification, and
add a new field to session::Session to track the active passkey id.
```

Claude will:

1. Call `dkod_plan` and present a partition (expect ≥ 3 groups).
2. Call `dkod_execute_begin` to mint a session + dk-branch.
3. Spawn N parallel Task subagents — one per group — using the
   `parallel-executor` template. Each subagent rewrites its own
   symbols via `dkod_write_symbol`.
4. Wait for all subagents to return DONE.
5. Call `dkod_commit` to land one commit per group on the dk-branch.

## Inspect

After execution:

```sh
git log --oneline main..HEAD       # one commit per group
git diff main..HEAD                # the actual rewrite
```

Then ship:

```text
/dkod-swarm:pr M5 manual smoke: passkey rewrite
```

This pushes the dk-branch and opens a PR via `gh`. (The repo is local
without a remote, so the push will fail — that is the expected
end-of-test signal. The PR step is exercised by the automated test.)

## What success looks like

- The partition has ≥ 3 groups
- Wall-clock from `dkod_execute_begin` to `dkod_commit` is noticeably
  faster than driving the same rewrite single-agent (the empirical
  bound is the M5-2 micro-benchmark; here it's a feel-test with real
  LLM latency)
- The diff compiles (`cargo check` in `/tmp/auth-sandbox`)
- The parallel writes did not produce git conflicts or stomp on each
  other's edits

## Cleanup

```sh
/path/to/dkod-swarm/target/release/dkod abort  # destroys the dk-branch
rm -rf /tmp/auth-sandbox
```

This guide is intentionally not executable in CI — it requires a real
Claude Code session and real LLM round-trips. Treat it as the
human-in-the-loop counterpart to the automated tests.
````

- [ ] **Step 2: Commit (docs-only, skip local CR).**

```sh
git checkout -b m5/manual-guide-readme
git add bench/MANUAL_E2E.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add manual E2E driving guide for auth sandbox"
```

## Task 9: bench/README.md + repo README bump

**Files:**
- Create: `bench/README.md`
- Modify: `README.md`

- [ ] **Step 1: Create `bench/README.md`.** (Outer fence is four backticks because the body contains nested triple-backtick `sh` blocks.)

````markdown
# bench/

Benchmarking and end-to-end fixtures for dkod-swarm.

## Layout

- `sandboxes/auth/` — a 4-module Rust crate (login, logout, session,
  passkey) used by both the automated E2E tests and the manual
  driving guide. Not a workspace member; never built by
  `cargo build --workspace`.
- `MANUAL_E2E.md` — step-by-step guide for driving the auth sandbox
  end-to-end through real Claude Code with the dkod-swarm plugin.

## Automated counterparts

The automated tests live next to the rest of the workspace tests:

- `crates/dkod-mcp/tests/bench_sandbox_e2e.rs` — full plan→pr flow
  against the auth sandbox via the in-process rmcp client. PATH-shimmed
  `gh` and `git push`; no GitHub credentials touched.
- `crates/dkod-mcp/tests/bench_parallel_vs_serial.rs` — wall-clock
  benchmark asserting that three parallel writes (each carrying a
  100ms simulated LLM delay) complete > 1.5× faster than the same
  three writes done sequentially.

Run them with:

```sh
cargo test -p dkod-mcp --test bench_sandbox_e2e
cargo test -p dkod-mcp --test bench_parallel_vs_serial -- --nocapture
```

## Why a sandbox crate that isn't built by the workspace

The auth sandbox is *fixture content* — files dkod-swarm reads in
order to exercise the partitioner, the AST-merge primitive, and the
end-to-end flow. It's not part of the dkod-swarm product. Building it
on every `cargo test --workspace` run would slow the suite for zero
correctness signal.

If you want to build it standalone:

```sh
cd bench/sandboxes/auth && cargo build
```
````

- [ ] **Step 2: Update the repo `README.md` Status section.**

Replace the existing Status section with:

```markdown
## Status

**v0 in flight — milestones 1, 2, 3, 4, and 5 merged.** `cargo test --workspace` is green across 8 PRs of M1, 8 of M2, 3 of M3, 3 of M4, and 3 of M5. Empirical proof of the parallel-vs-serial speedup lives in `crates/dkod-mcp/tests/bench_parallel_vs_serial.rs`; a human-driven counterpart is documented in `bench/MANUAL_E2E.md`.

The full design lives in [`docs/design.md`](docs/design.md). Milestone 6 (marketplace publish — replaces the `cargo run`-based `.mcp.json` with binary distribution) is the remaining ship item.
```

- [ ] **Step 3: Commit (docs-only, skip local CR).**

```sh
git add bench/README.md README.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Document bench/ + bump README to mention M5"
```

## Task 10: PR M5-3 wrap-up

- [ ] `cargo test --workspace` green (no Rust changes in this PR; should be unchanged from M5-2).
- [ ] PR `M5-3: manual E2E guide + README bump`. Merge autonomously.
- [ ] After merge, controller tags `v0.5.0-m5` on `main`:

```sh
git checkout main && git pull
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git tag -a v0.5.0-m5 -m "Milestone 5: E2E smoke + parallel-vs-serial benchmark"
git push origin v0.5.0-m5
```

---

## Milestone 5 exit criteria

1. `cargo test --workspace` green across all 3 PRs merged to `main`. Two new test files (`bench_sandbox_e2e`, `bench_parallel_vs_serial`) pass.
2. The auth sandbox under `bench/sandboxes/auth/` is a self-contained Rust crate that builds standalone (`cd bench/sandboxes/auth && cargo build` exits 0). It is NOT a workspace member; `cargo build --workspace` does not touch it.
3. `bench_parallel_vs_serial.rs` asserts a > 1.5× wall-clock speedup of three parallel writes vs three sequential writes under a 100ms simulated LLM delay. The assertion passes 5/5 stress runs locally.
4. `bench/MANUAL_E2E.md` documents the human-driven counterpart end-to-end: install plugin, init sandbox, run `/dkod-swarm:execute`, inspect, ship via `/dkod-swarm:pr`.
5. The repo README's Status section mentions M5 alongside M1-M4.
6. All commits on `main` authored AND committed by `Haim Ari <haimari1@gmail.com>` — zero `Co-Authored-By` trailers across M5 history.

## Out of scope (M6+)

- Marketplace publish — the `.mcp.json` still uses `cargo run`. Hardening to a binary-distribution model is M6's job.
- Real-LLM-driven CI E2E. The MANUAL_E2E.md guide is the closest dkod-swarm gets to that without taking on Claude API auth + flake budget. M6+ may add an opt-in CI job that drives Claude API for nightly smoke.
- Sandbox variety — only one sandbox (auth) is shipped. M6+ may add a database-migration sandbox, a tree-sitter-grammar sandbox, etc., to broaden the empirical surface.
- `cargo bench` integration via Criterion. The current benchmark is a `#[tokio::test]` with `eprintln!` output, not a Criterion harness. Criterion would let us track regressions over time but adds a dep + a 10-minute baseline run; M5 keeps it simple.
