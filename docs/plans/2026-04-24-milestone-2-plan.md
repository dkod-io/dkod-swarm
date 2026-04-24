# Milestone 2: `dkod-mcp` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `dkod-mcp` — a stdio MCP server (rmcp-based) that exposes the 8 dkod-swarm tools, each a thin wrapper around M1's `dkod-worktree` + `dkod-orchestrator` library functions. Adds the two M2-specific responsibilities not present in M1: a per-file lock guarding `dkod_write_symbol`, and idempotency on `dkod_pr`. Milestone ends with a green `cargo test --workspace` plus an in-process rmcp client↔server smoke test that drives the full plan→commit→PR flow (mocking `gh`).

**Architecture:** One new crate `crates/dkod-mcp` under the existing Cargo workspace. A `stdio` binary target (`dkod-mcp`) and a library surface tested via an in-process rmcp client. Server state is a single `ServerCtx` holding: repo root, `dkod_worktree::Paths`, an `Option<SessionId>` for the active session, and a `HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>` lock table for `dkod_write_symbol`. Recovery on restart scans `.dkod/sessions/` and picks the session whose `Manifest.status == Executing` (design §State). Every tool is a thin async wrapper: it loads/saves manifests via M1 APIs, never re-implements business logic. The one new subprocess integration (`gh`) lives in a small `gh.rs` helper in this crate — not in `dkod-worktree`, because it is only used at PR time.

**Tech Stack:** Rust 2024. New crate deps: `rmcp 1.5` (features `server`, `macros`, `transport-io`), `tokio 1` (features `rt-multi-thread`, `macros`, `sync`, `io-std`, `process`), `schemars 0.8` (for rmcp JSON-schema derivation), `async-trait` (rmcp hint), `chrono 0.4` (ISO-8601 timestamps for `Manifest.created_at` / `WriteRecord.timestamp`). Tests use `anyhow`, `tempfile`, and rmcp's `client` feature for in-process harness.

---

## Engine / SDK API probes (reference)

Two external APIs M2 leans on. Both are version-pinned to what crates.io exposes today. The first PR's first task is a **probe** that writes a tiny example and runs it, the same pattern M1 used for `dk-engine`. Do NOT skip it — the WebFetch summaries for rmcp during planning disagreed on the exact feature flags and macro form.

- **rmcp 1.5.x** — confirmed surface (verified by `examples/probe_rmcp.rs` on 2026-04-24):
  - `rmcp::transport::stdio()` returns `(tokio::io::Stdin, tokio::io::Stdout)` — a tuple that satisfies `IntoTransport<Role, std::io::Error, _>` via the blanket impl on `(R: AsyncRead, W: AsyncWrite)` pairs.
  - `rmcp::ServiceExt<R: ServiceRole>` is a trait — its `serve<T, E, A>(self, transport)` method is blanket-impl'd for every `S: Service<R>`. For a server handler, use `ServiceExt<RoleServer>`; for a client, `ServiceExt<RoleClient>`. `RoleServer` and `RoleClient` are re-exported at the rmcp crate root.
  - `#[tool_router]` (no arguments in the probe) decorates an `impl MyServer` block; the decorated struct must own a `tool_router: ToolRouter<MyServer>` field. `#[tool_handler] impl ServerHandler for MyServer {}` wires the router into the MCP `ServerHandler` trait.
  - `#[tool(description = "…")]` methods take `&self` + `rmcp::handler::server::wrapper::Parameters<T>` where `T: serde::Deserialize + schemars::JsonSchema`, and return `Result<_, rmcp::ErrorData>` (note: `ErrorData`, not `Error` — the probe confirms this).
  - `rmcp` re-exports `schemars` at the crate root, so downstream code uses `rmcp::schemars::JsonSchema` (or adds a direct `schemars` dep, which the workspace does).
  - Client side (needed by the in-process test harness in Task 4): `()` implements `ClientHandler`, so `().into_dyn()` yields a `Box<dyn DynService<RoleClient>>`, and `.serve(client_io).await` drives the handshake. `ServiceExt::into_dyn` (returns `Box<dyn DynService<R>>`) is the documented way to erase the concrete handler type.
  - **Pitfall:** the `tool_router` struct field looks unused to dead-code analysis because the `#[tool_router]` macro consults it from a generated `Self::tool_router()` ctor. Either mark the field `#[allow(dead_code)]` or treat the warning as expected.
  - **Probe-verified assertion technique:** writing `fn _assert_service_ext<S: ServiceExt<RoleServer>>(_: &S) {}` followed by `_assert_service_ext(&server)` compiles iff the blanket impl resolves — cleaner than trying to spell the full `serve` signature, which requires three generic args (`T`, `E`, `A`) that can't be inferred without a real transport argument.
- **`gh` CLI** — used only by `dkod_pr`. Three invocations:
  - `gh pr list --head <branch> --state all --json url --jq '.[0].url // empty'` — idempotency check.
  - `git push --force-with-lease --set-upstream origin <branch>` — push (raw git, not gh).
  - `gh pr create --title <t> --body <b> --head <branch>` — returns PR URL on stdout.
  All three are subprocess calls. `gh` uses the user's existing auth (no token handling here).

---

## Design-doc note

Design `§MCP tool surface` lists `dkod_write_symbol(file, symbol, body)` — no session id. This plan resolves the implicit session binding by making the MCP server process own **one current session at a time**: `dkod_execute_begin` sets it, `dkod_abort`/successful `dkod_pr` clears it, and restart-time recovery scans `.dkod/sessions/` for the one in `Executing` state (matches design §State: "If the orchestrator or Claude Code crashes, `dkod status` re-reads state and resumes"). This keeps the tool signature in design §MCP unchanged. Not a design revision — an implementation clarification that stays inside the existing wording. No `docs/design.md` PR is required before code lands. If during implementation the single-session model forces a real spec change, pause and open a `docs/design.md` PR first per CLAUDE.md.

---

## File Structure

New files only. Nothing under `crates/dkod-worktree/` or `crates/dkod-orchestrator/` is modified (except to export helpers already public).

```text
Cargo.toml                                     # +workspace.dependencies: rmcp, tokio, schemars, async-trait, chrono
crates/
└── dkod-mcp/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                             # re-exports for tests
    │   ├── main.rs                            # binary entry (stdio)
    │   ├── error.rs                           # Error enum
    │   ├── ctx.rs                             # ServerCtx (session state, lock table, repo root)
    │   ├── recovery.rs                        # scan_executing_session
    │   ├── gh.rs                              # pr_exists, push_branch, create_pr (subprocess)
    │   ├── time.rs                            # iso8601_now
    │   ├── tools/
    │   │   ├── mod.rs                         # McpServer struct, #[tool_router]
    │   │   ├── plan.rs                        # dkod_plan
    │   │   ├── execute_begin.rs               # dkod_execute_begin
    │   │   ├── write_symbol.rs                # dkod_write_symbol (per-file lock)
    │   │   ├── execute_complete.rs            # dkod_execute_complete
    │   │   ├── commit.rs                      # dkod_commit
    │   │   ├── pr.rs                          # dkod_pr (idempotent)
    │   │   ├── status.rs                      # dkod_status
    │   │   └── abort.rs                       # dkod_abort
    │   └── schema.rs                          # shared request/response structs (#[derive(JsonSchema)])
    └── tests/
        ├── fixtures/
        │   └── tiny_rust/                     # tiny Rust repo scaffold helper for integration tests
        │       └── src/lib.rs                 # `pub fn a() {} pub fn b() {} pub fn c() { a(); }`
        ├── common/
        │   └── mod.rs                         # init_tempo_repo(), spawn_in_process_server()
        ├── rmcp_probe.rs                      # SDK shape verification (Task 1)
        ├── plan_tool.rs
        ├── execute_begin_abort.rs
        ├── write_symbol_lock.rs               # proves concurrent writes to same file serialise
        ├── execute_complete_status.rs
        ├── commit_tool.rs
        ├── pr_tool.rs                         # gh stubbed via PATH shim
        └── e2e_smoke.rs                       # full plan→commit→pr flow
```

Integration tests (outside `tests/`) use the crate as a library (via `lib.rs`) and drive an in-process rmcp server + client pair. The `main.rs` binary is thin — it just builds a `ServerCtx` from `cwd` and calls `Server.serve(stdio()).await`.

### Code-placement convention (applies to every tool task below)

Every `#[tool]` method lives in **one** `#[tool_router] impl McpServer` block in `crates/dkod-mcp/src/tools/mod.rs`. Submodule files (`plan.rs`, `execute_begin.rs`, `write_symbol.rs`, …) contain **pure-function helpers only** — no `impl McpServer`, no `#[tool]`. Each tool task:

1. Writes the pure-function helper(s) (and their unit tests) into the submodule file.
2. Appends **one** `#[tool]` method to the single `impl` block in `tools/mod.rs` that delegates to the helper + `to_rmcp_error`.

Where later tasks show code with `impl McpServer { #[tool] ... }` inline in a submodule snippet, that code is illustrative of the method body — actually place the method in `tools/mod.rs`. This keeps `#[tool_router]`'s attribute surface single-block and side-steps the per-rmcp-version split-across-files pitfall.

---

## PR Plan

Milestone 2 lands in **8 PRs**. Each PR is a feature branch off `main`, opened fresh, and goes through the full CodeRabbit loop per `CLAUDE.md` (local `/coderabbit:code-review` → fix → re-review → commit/push → wait for PR review → `/coderabbit:autofix` → merge autonomously once clean). Branch names match the PR title prefix.

| PR | Branch | Scope | Tasks |
|----|--------|-------|-------|
| M2-1 | `m2/mcp-scaffold-probe` | `dkod-mcp` crate scaffold, workspace deps, rmcp SDK probe, stdio binary, empty handler | 1–4 |
| M2-2 | `m2/mcp-plan` | `dkod_plan` tool + in-process integration test | 5–7 |
| M2-3 | `m2/mcp-execute-begin-abort` | `dkod_execute_begin` + `dkod_abort` + recovery | 8–11 |
| M2-4 | `m2/mcp-write-symbol` | `dkod_write_symbol` with per-file tokio mutex | 12–14 |
| M2-5 | `m2/mcp-complete-status` | `dkod_execute_complete` + `dkod_status` | 15–17 |
| M2-6 | `m2/mcp-commit` | `dkod_commit` wrapper + git identity check | 18–19 |
| M2-7 | `m2/mcp-pr` | `dkod_pr` (idempotent) + `gh` subprocess helper | 20–23 |
| M2-8 | `m2/mcp-e2e-smoke` | full plan→execute→commit→PR integration test | 24–26 |

Each PR must end with a **green `cargo test --workspace`** before the review gate is entered.

---

## Commit & PR conventions (recap — read `CLAUDE.md` first)

Every commit in this plan MUST:

- Run author + committer through the env-var override:
  ```sh
  GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
  GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
  git commit -m "..."
  ```
  No `Co-Authored-By`. Verify with `git log -1 --format='%an <%ae> | %cn <%ce>'` after each commit. Controller-side verification (see `~/.claude/memory/tools/git-subagent-commits.md`) runs after every subagent commit.
- Be preceded by `/coderabbit:code-review` on the local diff vs `main`, unless the changeset is docs/config-only.

Every PR MUST:

- Title ≤ 70 chars.
- Body = short summary + test-plan checklist.
- Open ONE PR at a time. Do not start the next PR's branch until the current one is merged.
- **Merge autonomously** once CodeRabbit is clean and `cargo test --workspace` is green (per project policy — see `feedback_autonomous_merge.md`).

Poller discipline per `~/.claude/memory/tools/coderabbit.md`: arm the 3-condition poller after every push, `TaskStop` stale pollers immediately on merge/close.

---

# PR M2-1 — Scaffold + rmcp probe

## Task 1: rmcp SDK probe

**Files:**
- Create: `crates/dkod-mcp/Cargo.toml`
- Create: `crates/dkod-mcp/examples/probe_rmcp.rs`
- Modify: `Cargo.toml` (workspace root) — add new workspace deps

- [ ] **Step 1: Extend workspace dependencies.**

Edit `Cargo.toml` (workspace root) — keep existing keys untouched; append to `[workspace.dependencies]`:

```toml
rmcp = { version = "1.5", features = ["server", "client", "macros", "transport-io"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "io-std", "process", "time"] }
schemars = "0.8"
async-trait = "0.1"
chrono = { version = "0.4", default-features = false, features = ["clock", "serde"] }
```

And extend the `members` list:

```toml
members = [
    "crates/dkod-worktree",
    "crates/dkod-orchestrator",
    "crates/dkod-mcp",
]
```

- [ ] **Step 2: Create the crate manifest.**

`crates/dkod-mcp/Cargo.toml`:

```toml
[package]
name = "dkod-mcp"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "Stdio MCP server exposing the 8-tool dkod-swarm surface"

# `[lib]` and `[[bin]]` are intentionally omitted — Cargo autodiscovers
# `src/lib.rs` and `src/main.rs`. Explicit blocks are only needed if a
# later PR wants to customise target names or paths.

[dependencies]
dkod-worktree = { path = "../dkod-worktree" }
dkod-orchestrator = { path = "../dkod-orchestrator" }
rmcp.workspace = true
tokio.workspace = true
schemars.workspace = true
async-trait.workspace = true
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
anyhow.workspace = true
```

- [ ] **Step 3: Write the probe example.**

`crates/dkod-mcp/examples/probe_rmcp.rs` — this is the analogue of M1's `probe_engine_api.rs`. It must compile and run to `stderr "probe ok"` without actually starting the stdio loop (we don't want a hanging example). Use `#[tokio::main]` only to exercise the async path.

```rust
//! Probe that verifies the rmcp 1.5 surface the M2 plan leans on:
//! - `rmcp::transport::stdio()` returns a transport.
//! - A handler struct with `#[tool_router(server_handler)]` exposes tools.
//! - `ServiceExt::serve` is available on the handler.
//!
//! Run with: `cargo run -p dkod-mcp --example probe_rmcp`
//! Expected: exits 0 and prints "probe ok" on stderr.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct PingArgs {
    msg: String,
}

#[derive(Clone, Default)]
struct ProbeServer {
    tool_router: ToolRouter<ProbeServer>,
}

#[tool_router]
impl ProbeServer {
    #[tool(description = "probe tool, returns the input string prefixed with 'pong: '")]
    async fn ping(&self, Parameters(args): Parameters<PingArgs>) -> Result<String, rmcp::ErrorData> {
        Ok(format!("pong: {}", args.msg))
    }
}

#[tool_handler]
impl ServerHandler for ProbeServer {}

fn main() {
    // Verify constructibility without actually starting the stdio loop
    // (that would block forever waiting on stdin).
    let _server = ProbeServer::default();
    // `stdio()` is a compile-time check that the transport symbol exists
    // and returns the expected type; don't serve.
    let _t = stdio;
    // `ServiceExt::serve` exists — reference it without calling.
    let _p: fn(ProbeServer, _) -> _ = <ProbeServer as ServiceExt<_>>::serve;
    eprintln!("probe ok");
}
```

- [ ] **Step 4: Run the probe.**

```sh
cargo run -p dkod-mcp --example probe_rmcp 2>&1 | tail -3
```
Expected stderr: `probe ok`. Exit code: 0.

**If this fails**, update the probe to whatever the real rmcp 1.5 surface is (the macro names / import paths), and **amend every `tools/` file in later PRs to match** before committing. Record the correct surface at the top of this plan file under "Engine / SDK API probes" as we did for `dk-engine` in M1. Do not proceed to Task 2 until the probe passes.

- [ ] **Step 5: Commit.**

```sh
git checkout -b m2/mcp-scaffold-probe
git add Cargo.toml crates/dkod-mcp/Cargo.toml crates/dkod-mcp/examples/probe_rmcp.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod-mcp crate scaffold + rmcp SDK probe"
```

Docs-only? No — this touches Rust source. Run `/coderabbit:code-review` first.

## Task 2: Error + ctx skeletons

**Files:**
- Create: `crates/dkod-mcp/src/lib.rs`
- Create: `crates/dkod-mcp/src/error.rs`
- Create: `crates/dkod-mcp/src/ctx.rs`
- Create: `crates/dkod-mcp/src/time.rs`

- [ ] **Step 1: Write the failing test — `McpError::Io` round-trip.**

`crates/dkod-mcp/tests/error_smoke.rs`:

```rust
use dkod_mcp::error::Error as McpError;

#[test]
fn worktree_error_wraps() {
    let wt = dkod_worktree::Error::Invalid("boom".into());
    let err: McpError = wt.into();
    assert!(matches!(err, McpError::Worktree(_)));
    assert!(err.to_string().contains("boom"));
}
```

- [ ] **Step 2: Run — expected fail.**

`cargo test -p dkod-mcp --test error_smoke` → compile error: crate has no `lib.rs` yet.

- [ ] **Step 3: Create `lib.rs`.**

```rust
pub mod ctx;
pub mod error;
pub mod time;

pub use ctx::ServerCtx;
pub use error::{Error, Result};
```

- [ ] **Step 4: Create `error.rs`.**

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("worktree error: {0}")]
    Worktree(#[from] dkod_worktree::Error),

    #[error("orchestrator error: {0}")]
    Orchestrator(#[from] dkod_orchestrator::Error),

    #[error("no active session — call dkod_execute_begin first")]
    NoActiveSession,

    #[error("session already active: {0}")]
    SessionAlreadyActive(String),

    #[error("group not found in active session: {0}")]
    UnknownGroup(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("gh subprocess failed: {cmd}: {stderr}")]
    Gh { cmd: String, stderr: String },

    #[error("verify_cmd failed (exit {exit}): {tail}")]
    VerifyFailed { exit: i32, tail: String },

    #[error("invalid argument: {0}")]
    InvalidArg(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
```

- [ ] **Step 5: Create `time.rs`.**

```rust
use chrono::{SecondsFormat, Utc};

/// ISO-8601 timestamp in UTC, second precision, `Z` suffix.
/// Used for `Manifest.created_at` and `WriteRecord.timestamp`.
pub fn iso8601_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
```

- [ ] **Step 6: Create `ctx.rs` (skeleton only; filled out in later PRs).**

```rust
use dkod_worktree::{Paths, SessionId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Per-process server context. One `ServerCtx` per `dkod-mcp` process.
///
/// `active_session` holds the id of the in-flight session, if any. It is set
/// by `dkod_execute_begin`, cleared by `dkod_abort` and a successful
/// `dkod_pr`. Fresh processes recover it by scanning `.dkod/sessions/` for a
/// manifest with status `Executing` (see `recovery.rs`).
pub struct ServerCtx {
    pub repo_root: PathBuf,
    pub paths: Paths,
    pub active_session: Mutex<Option<SessionId>>,
    /// Per-file locks guarding `dkod_write_symbol`. Keyed by canonicalized
    /// absolute path. Entries are created on first write to a file and live
    /// until the session ends (we intentionally do not GC mid-session).
    pub file_locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl ServerCtx {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            paths: Paths::new(repo_root),
            active_session: Mutex::new(None),
            file_locks: Mutex::new(HashMap::new()),
        }
    }

    /// Fetch or create the lock for `abs_path`. Returns an `Arc<Mutex<()>>`
    /// that the caller can `.lock().await` independently of the map.
    pub async fn file_lock(&self, abs_path: &Path) -> Arc<Mutex<()>> {
        let mut map = self.file_locks.lock().await;
        map.entry(abs_path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
```

- [ ] **Step 7: Run the test — expected pass.**

`cargo test -p dkod-mcp --test error_smoke` → PASS.

- [ ] **Step 8: Commit.**

```sh
git add crates/dkod-mcp/src/lib.rs crates/dkod-mcp/src/error.rs \
        crates/dkod-mcp/src/ctx.rs crates/dkod-mcp/src/time.rs \
        crates/dkod-mcp/tests/error_smoke.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add error, ctx, time skeletons for dkod-mcp"
```

Run `/coderabbit:code-review` before the commit.

## Task 3: Empty server + stdio binary

**Files:**
- Create: `crates/dkod-mcp/src/tools/mod.rs`
- Create: `crates/dkod-mcp/src/schema.rs`
- Create: `crates/dkod-mcp/src/main.rs`

- [ ] **Step 1: Write the failing test — server compiles + default-constructs.**

`crates/dkod-mcp/tests/server_ctor.rs`:

```rust
use dkod_mcp::{McpServer, ServerCtx};
use std::sync::Arc;

#[test]
fn server_constructs_with_ctx() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Arc::new(ServerCtx::new(tmp.path()));
    let _srv = McpServer::new(ctx);
}
```

- [ ] **Step 2: Run — expected fail.**

`cargo test -p dkod-mcp --test server_ctor` → compile error: `McpServer` missing.

- [ ] **Step 3: Create `schema.rs` (empty for now — request/response structs arrive with each tool).**

```rust
//! Shared request/response types. Filled out per-tool in later tasks.
```

- [ ] **Step 4: Create `tools/mod.rs`.**

```rust
use crate::ServerCtx;
use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    tool_handler, tool_router,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct McpServer {
    pub(crate) ctx: Arc<ServerCtx>,
    tool_router: ToolRouter<McpServer>,
}

impl McpServer {
    pub fn new(ctx: Arc<ServerCtx>) -> Self {
        Self { ctx, tool_router: Self::tool_router() }
    }
}

// Tool methods are added by submodule `impl` blocks; this block is the
// canonical `#[tool_router]` target. Each PR appends #[tool] methods here.
#[tool_router]
impl McpServer {}

#[tool_handler]
impl ServerHandler for McpServer {}
```

- [ ] **Step 5: Export `McpServer` from `lib.rs`.**

Modify `crates/dkod-mcp/src/lib.rs`:

```rust
pub mod ctx;
pub mod error;
pub mod schema;
pub mod time;
pub mod tools;

pub use ctx::ServerCtx;
pub use error::{Error, Result};
pub use tools::McpServer;
```

- [ ] **Step 6: Create the binary entry.**

`crates/dkod-mcp/src/main.rs`:

```rust
//! `dkod-mcp` binary — stdio MCP server.
//!
//! Runs in the current working directory; `ServerCtx::new` rebuilds `Paths`
//! under `<cwd>/.dkod`. The hosting Claude Code plugin is responsible for
//! invoking this binary from the repo root.

use dkod_mcp::{McpServer, ServerCtx};
use rmcp::{ServiceExt, transport::stdio};
use std::sync::Arc;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;
    let ctx = Arc::new(ServerCtx::new(&repo_root));
    let service = McpServer::new(ctx).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 7: Add `anyhow` to `[dependencies]` (not dev-dep) for `main.rs`.**

Edit `crates/dkod-mcp/Cargo.toml` — move `anyhow` from `[dev-dependencies]` up to `[dependencies]`, leaving the workspace pin untouched:

```toml
[dependencies]
# ... existing ...
anyhow.workspace = true
```

- [ ] **Step 8: Run the test — expected pass.**

```sh
cargo test -p dkod-mcp --test server_ctor
cargo build -p dkod-mcp --bin dkod-mcp
```
Both succeed.

- [ ] **Step 9: Commit.**

```sh
git add crates/dkod-mcp/src/tools/mod.rs crates/dkod-mcp/src/schema.rs \
        crates/dkod-mcp/src/main.rs crates/dkod-mcp/src/lib.rs \
        crates/dkod-mcp/Cargo.toml crates/dkod-mcp/tests/server_ctor.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add empty McpServer + stdio binary entry"
```

Run `/coderabbit:code-review` first.

## Task 4: In-process client harness (fixtures)

This test harness is reused by every later PR's integration test. Build it once, well.

**Files:**
- Create: `crates/dkod-mcp/tests/common/mod.rs`
- Create: `crates/dkod-mcp/tests/fixtures/tiny_rust/src/lib.rs`

- [ ] **Step 1: Write `tests/fixtures/tiny_rust/src/lib.rs`.**

```rust
pub fn a() {}
pub fn b() {}
pub fn c() { a(); b(); }
pub fn d() {}
```

- [ ] **Step 2: Write `tests/common/mod.rs`.**

```rust
#![allow(dead_code)] // harness; each test file uses a subset

use dkod_mcp::{McpServer, ServerCtx};
use rmcp::{RoleClient, ServiceExt, service::RunningService};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// Initialise a fresh bare git repo in a tempdir, seed one commit on `main`
/// with `tests/fixtures/tiny_rust/src/lib.rs`, and return the repo path.
pub fn init_tempo_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();

    run(&root, &["git", "init", "-q", "-b", "main"]);
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tiny_rust/src/lib.rs");
    std::fs::copy(&fixture, src.join("lib.rs")).expect("copy fixture");
    // Minimal Cargo.toml so the fixture compiles if a caller chooses to build it.
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    run_with_identity(&root, &["git", "add", "-A"]);
    run_with_identity(&root, &["git", "commit", "-q", "-m", "seed"]);

    // init .dkod/
    dkod_worktree::init_repo(&root, None).expect("init_repo");
    (tmp, root)
}

/// Spawn an in-process rmcp server bound to `repo_root` and return a client
/// connected to it via an in-memory duplex transport.
pub async fn spawn_in_process_server(
    repo_root: &Path,
) -> RunningService<RoleClient, rmcp::model::InitializeRequestParam> {
    // rmcp supports arbitrary `AsyncRead + AsyncWrite` transports; a pair of
    // `tokio::io::duplex` streams gives us an in-process client/server link
    // without touching real stdio. The exact wiring is confirmed in the
    // probe in Task 1 — if the rmcp API differs, adapt here and in every
    // later integration test that calls this fn.
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let ctx = Arc::new(ServerCtx::new(repo_root));
    let server = McpServer::new(ctx);
    let _running_server = tokio::spawn(async move {
        let svc = server.serve(server_io).await.expect("server serve");
        svc.waiting().await.ok();
    });
    ().into_dyn()
        .serve(client_io)
        .await
        .expect("client handshake")
}

fn run(dir: &Path, args: &[&str]) {
    let status = Command::new(args[0]).args(&args[1..]).current_dir(dir).status().unwrap();
    assert!(status.success(), "command failed: {args:?}");
}

fn run_with_identity(dir: &Path, args: &[&str]) {
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Haim Ari")
        .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
        .env("GIT_COMMITTER_NAME", "Haim Ari")
        .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
        .status()
        .unwrap();
    assert!(status.success(), "command failed: {args:?}");
}
```

> **Note on the rmcp client handshake:** the `().into_dyn().serve(client_io)` form above is the expected shape from rmcp 1.5 docs, but the probe in Task 1 is authoritative. If the probe shows a different client entry point, update this harness **before** running any integration test. This is the single point of truth for every later task that wants a client.

- [ ] **Step 3: Quick compile check.**

```sh
cargo check --tests -p dkod-mcp
```
Expected: compiles (no test runs because `common/mod.rs` is a shared module, not a `#[test]`).

- [ ] **Step 4: Commit.**

```sh
git add crates/dkod-mcp/tests/common/mod.rs \
        crates/dkod-mcp/tests/fixtures/tiny_rust/src/lib.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add integration test harness: tiny_rust fixture + in-process client"
```

Run `/coderabbit:code-review` first.

## PR M2-1 wrap-up

- [ ] `cargo test --workspace` → green (existing 48 tests + new `error_smoke` + `server_ctor`).
- [ ] Push, open PR `M2-1: dkod-mcp scaffold + rmcp probe`. Body: summary + test plan checklist.
- [ ] Arm the CodeRabbit poller. Iterate `/coderabbit:autofix` until clean.
- [ ] Merge autonomously. `gh pr merge <n> --merge --delete-branch`. Sync `main`.

---

# PR M2-2 — `dkod_plan`

## Task 5: Plan request/response schema

**Files:**
- Modify: `crates/dkod-mcp/src/schema.rs`

- [ ] **Step 1: Write the failing test — serde round-trip + JsonSchema derivation compiles.**

`crates/dkod-mcp/tests/plan_schema.rs`:

```rust
use dkod_mcp::schema::{PlanRequest, PlanResponse};

#[test]
fn plan_request_round_trips() {
    let req = PlanRequest {
        task_prompt: "refactor auth".into(),
        in_scope: vec!["crate::auth::login".into(), "crate::auth::logout".into()],
        files: vec!["src/auth.rs".into()],
        target_groups: 2,
    };
    let j = serde_json::to_string(&req).unwrap();
    let back: PlanRequest = serde_json::from_str(&j).unwrap();
    assert_eq!(back.target_groups, 2);
    assert_eq!(back.in_scope.len(), 2);
}

#[test]
fn plan_response_is_serializable() {
    let resp = PlanResponse {
        session_preview_id: None,
        groups: vec![],
        warnings: vec![],
        unresolved_edges: 0,
    };
    let _ = serde_json::to_string(&resp).unwrap();
}
```

- [ ] **Step 2: Run — expected fail.**

`cargo test -p dkod-mcp --test plan_schema` → compile error.

- [ ] **Step 3: Define the schema.**

Replace `crates/dkod-mcp/src/schema.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanRequest {
    /// The user's natural-language task description. Stored on the session
    /// manifest when execute_begin fires; not used for partitioning.
    pub task_prompt: String,
    /// Qualified symbol names the caller wants to partition (typically the
    /// output of Claude's scoping pass). Names that do not resolve in the
    /// call graph surface as `ScopeSymbolUnknown` warnings.
    pub in_scope: Vec<String>,
    /// Rust source files to read for symbol/call extraction, relative to
    /// the repo root.
    pub files: Vec<PathBuf>,
    /// Desired number of groups. Mismatches produce warnings; the partition
    /// is never artificially inflated or deflated.
    pub target_groups: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanGroup {
    pub id: String,
    pub symbols: Vec<PlanSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanSymbol {
    pub qualified_name: String,
    pub file_path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanResponse {
    /// Reserved for a future "dry-run" flow where `dkod_plan` pre-allocates
    /// a session id. Always `None` in v0 — `dkod_execute_begin` mints the id.
    pub session_preview_id: Option<String>,
    pub groups: Vec<PlanGroup>,
    pub warnings: Vec<String>,
    /// Number of call edges whose caller or callee could not be resolved to
    /// a known symbol. Purely informational (normal for edges to external
    /// dependencies).
    pub unresolved_edges: usize,
}
```

- [ ] **Step 4: Run the test — expected pass.**

`cargo test -p dkod-mcp --test plan_schema` → PASS.

- [ ] **Step 5: Commit.**

```sh
git checkout -b m2/mcp-plan
git add crates/dkod-mcp/src/schema.rs crates/dkod-mcp/tests/plan_schema.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add plan request/response schema"
```

## Task 6: `dkod_plan` tool implementation

**Files:**
- Create: `crates/dkod-mcp/src/tools/plan.rs`
- Modify: `crates/dkod-mcp/src/tools/mod.rs` — `pub mod plan;` at the top

- [ ] **Step 1: Write the failing unit test — pure function `build_plan`.**

`crates/dkod-mcp/tests/plan_tool_unit.rs`:

```rust
mod common { include!("common/mod.rs"); }
use common::init_tempo_repo;
use dkod_mcp::schema::PlanRequest;
use dkod_mcp::tools::plan::build_plan;
use dkod_mcp::ServerCtx;
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
    let g_coupled = resp.groups.iter().find(|g| g.symbols.len() == 3).unwrap();
    let names: Vec<_> = g_coupled.symbols.iter().map(|s| s.qualified_name.as_str()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    assert!(names.contains(&"c"));
}
```

- [ ] **Step 2: Run — expected fail.**

`cargo test -p dkod-mcp --test plan_tool_unit` → compile error.

- [ ] **Step 3: Implement `plan.rs`.**

```rust
use crate::schema::{PlanGroup, PlanRequest, PlanResponse, PlanSymbol};
use crate::tools::McpServer;
use crate::{Error, Result, ServerCtx};
use dkod_orchestrator::{
    callgraph::CallGraph, partition::partition, symbols::extract_rust_file,
};
use rmcp::{handler::server::wrapper::Parameters, tool};

/// Pure helper used by both the MCP wrapper and unit tests.
pub fn build_plan(ctx: &ServerCtx, req: PlanRequest) -> Result<PlanResponse> {
    if req.target_groups == 0 {
        return Err(Error::InvalidArg("target_groups must be >= 1".into()));
    }
    let mut all_symbols = Vec::new();
    let mut all_edges = Vec::new();
    for rel in &req.files {
        let abs = ctx.repo_root.join(rel);
        let bytes = std::fs::read(&abs).map_err(Error::Io)?;
        let (syms, edges) = extract_rust_file(&bytes, &abs)?;
        all_symbols.extend(syms);
        all_edges.extend(edges);
    }
    let graph = CallGraph::build(&all_symbols, &all_edges);
    let part = partition(&req.in_scope, &graph, req.target_groups)?;

    let groups = part.groups
        .into_iter()
        .map(|g| PlanGroup {
            id: g.id,
            symbols: g.symbols.into_iter().map(|s| PlanSymbol {
                qualified_name: s.qualified_name,
                file_path: s.file_path,
                kind: s.kind,
            }).collect(),
        })
        .collect();
    let warnings = part.warnings.into_iter().map(|w| format!("{w:?}")).collect();

    Ok(PlanResponse {
        session_preview_id: None,
        groups,
        warnings,
        unresolved_edges: graph.unresolved_count(),
    })
}

/// Map `dkod_mcp::Error` to `rmcp::ErrorData` preserving the message.
pub(crate) fn to_rmcp_error(e: Error) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(e.to_string(), None)
}
```

The `#[tool]` method wrapping `build_plan` lives in `tools/mod.rs` per Step 4.

- [ ] **Step 4: Wire into `tools/mod.rs`.**

**Primary path: every `#[tool]` method lives in the single `#[tool_router] impl McpServer` block in `tools/mod.rs`**; submodules like `plan.rs` expose pure-function helpers only. This is the shape `#[tool_router]` supports most reliably. Update `plan.rs` — drop the `impl McpServer { #[tool] ... }` block and keep only the `build_plan` free function. Then append to `tools/mod.rs`:

```rust
pub mod plan;

#[tool_router]
impl McpServer {
    #[tool(description = "Plan a task: reads files, builds call graph, partitions in-scope symbols into disjoint groups by call coupling.")]
    pub async fn dkod_plan(
        &self,
        Parameters(req): Parameters<crate::schema::PlanRequest>,
    ) -> std::result::Result<crate::schema::PlanResponse, rmcp::ErrorData> {
        plan::build_plan(&self.ctx, req).map_err(plan::to_rmcp_error)
    }
}
```

Remove the empty `#[tool_router] impl McpServer {}` stub added in Task 3 — there is exactly one `#[tool_router]` block, and it grows one method per task. Later PRs append their `#[tool]` methods to this same block; submodule `.rs` files stay pure helpers (no `impl McpServer`).

Also add the needed imports at the top of `tools/mod.rs`:

```rust
use rmcp::{handler::server::wrapper::Parameters, tool};
```

- [ ] **Step 5: Run the test — expected pass.**

`cargo test -p dkod-mcp --test plan_tool_unit` → PASS.

- [ ] **Step 6: Commit.**

```sh
git add crates/dkod-mcp/src/tools/plan.rs crates/dkod-mcp/src/tools/mod.rs \
        crates/dkod-mcp/tests/plan_tool_unit.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod_plan tool"
```

## Task 7: End-to-end `dkod_plan` call via in-process client

**Files:**
- Create: `crates/dkod-mcp/tests/plan_tool_e2e.rs`

- [ ] **Step 1: Write the integration test.**

```rust
mod common { include!("common/mod.rs"); }
use common::{init_tempo_repo, spawn_in_process_server};
use serde_json::json;

#[tokio::test]
async fn plan_over_mcp_returns_expected_groups() {
    let (_tmp, root) = init_tempo_repo();
    let client = spawn_in_process_server(&root).await;
    let result = client
        .call_tool(rmcp::model::CallToolRequestParam {
            name: "dkod_plan".into(),
            arguments: Some(json!({
                "task_prompt": "demo",
                "in_scope": ["a", "b", "c", "d"],
                "files": ["src/lib.rs"],
                "target_groups": 2,
            }).as_object().cloned().unwrap().into_iter().collect()),
        })
        .await
        .expect("call_tool");
    let text = result.content.into_iter().next().and_then(|c| c.raw.as_text().map(|t| t.text.clone())).expect("text content");
    let resp: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(resp["groups"].as_array().unwrap().len(), 2);
    client.cancel().await.ok();
}
```

> **rmcp API caveat:** the exact shape of `CallToolRequestParam` and how tool return values are serialised (as `text` JSON vs a structured content block) depends on rmcp 1.5 — confirmed via the probe in Task 1. If the probe revealed a different shape, update this test accordingly. The semantic assertion — "two groups, one has three symbols" — is stable.

- [ ] **Step 2: Run.**

`cargo test -p dkod-mcp --test plan_tool_e2e` → PASS.

- [ ] **Step 3: Commit.**

```sh
git add crates/dkod-mcp/tests/plan_tool_e2e.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add end-to-end MCP test for dkod_plan"
```

## PR M2-2 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green.
- [ ] PR `M2-2: dkod_plan tool`. Merge autonomously when CR + tests pass.

---

# PR M2-3 — `dkod_execute_begin` + `dkod_abort`

## Task 8: Execute-begin / abort schema

**Files:**
- Modify: `crates/dkod-mcp/src/schema.rs` — add `ExecuteBeginRequest`, `ExecuteBeginResponse`, `GroupInput`, `SymbolRefSchema`, `AbortResponse`.

- [ ] **Step 1: Add schema.**

Append to `schema.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SymbolRefSchema {
    pub qualified_name: String,
    pub file_path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupInput {
    pub id: String,
    pub symbols: Vec<SymbolRefSchema>,
    pub agent_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteBeginRequest {
    pub task_prompt: String,
    pub groups: Vec<GroupInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteBeginResponse {
    pub session_id: String,
    pub dk_branch: String,
    pub group_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbortResponse {
    pub session_id: String,
}
```

## Task 9: `dkod_execute_begin` wrapper

**Files:**
- Create: `crates/dkod-mcp/src/tools/execute_begin.rs`
- Modify: `crates/dkod-mcp/src/tools/mod.rs`

- [ ] **Step 1: Write the failing integration test.**

`crates/dkod-mcp/tests/execute_begin_tool.rs`:

```rust
mod common { include!("common/mod.rs"); }
use common::init_tempo_repo;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::ServerCtx;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn execute_begin_creates_branch_and_persists_groups() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![SymbolRefSchema {
                qualified_name: "a".into(),
                file_path: PathBuf::from("src/lib.rs"),
                kind: "function".into(),
            }],
            agent_prompt: "refactor a".into(),
        }],
    };
    let resp = execute_begin(&ctx, req).await.expect("execute_begin");
    assert!(resp.session_id.starts_with("sess-"));
    assert_eq!(resp.dk_branch, format!("dk/{}", resp.session_id));
    assert_eq!(resp.group_ids, vec!["g1".to_string()]);

    // Active session recorded in-memory.
    let active = ctx.active_session.lock().await.clone();
    assert_eq!(active.unwrap().as_str(), resp.session_id);

    // Manifest + group spec on disk.
    let paths = &ctx.paths;
    let manifest_path = paths.manifest(&resp.session_id).unwrap();
    assert!(manifest_path.exists(), "manifest at {manifest_path:?} missing");
    let spec_path = paths.group_spec(&resp.session_id, "g1").unwrap();
    assert!(spec_path.exists(), "group spec at {spec_path:?} missing");

    // dk-branch checked out.
    let head = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&root).output().unwrap();
    let branch = String::from_utf8(head.stdout).unwrap().trim().to_string();
    assert_eq!(branch, resp.dk_branch);
}

#[tokio::test]
async fn execute_begin_rejects_second_concurrent_session() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![],
            agent_prompt: "x".into(),
        }],
    };
    execute_begin(&ctx, req.clone()).await.unwrap();
    let err = execute_begin(&ctx, req).await.unwrap_err();
    assert!(matches!(err, dkod_mcp::Error::SessionAlreadyActive(_)));
}
```

- [ ] **Step 2: Run — expected fail.**

`cargo test -p dkod-mcp --test execute_begin_tool` → compile error.

- [ ] **Step 3: Implement `execute_begin.rs`.**

```rust
use crate::schema::{ExecuteBeginRequest, ExecuteBeginResponse};
use crate::tools::McpServer;
use crate::tools::plan::to_rmcp_error;
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{
    GroupSpec, GroupStatus, Manifest, SessionId, SessionStatus, SymbolRef, branch,
};
use rmcp::{handler::server::wrapper::Parameters, tool};

pub async fn execute_begin(
    ctx: &ServerCtx,
    req: ExecuteBeginRequest,
) -> Result<ExecuteBeginResponse> {
    if req.groups.is_empty() {
        return Err(Error::InvalidArg("groups must be non-empty".into()));
    }
    let mut active = ctx.active_session.lock().await;
    if let Some(sid) = active.as_ref() {
        return Err(Error::SessionAlreadyActive(sid.to_string()));
    }

    let sid = SessionId::generate();
    let main = branch::detect_main(&ctx.repo_root)?;
    branch::create_dk_branch(&ctx.repo_root, &main, sid.as_str())?;

    let group_ids: Vec<String> = req.groups.iter().map(|g| g.id.clone()).collect();
    let manifest = Manifest {
        session_id: sid.clone(),
        task_prompt: req.task_prompt,
        created_at: crate::time::iso8601_now(),
        status: SessionStatus::Executing,
        group_ids: group_ids.clone(),
    };
    manifest.save(&ctx.paths)?;

    for g in req.groups {
        let spec = GroupSpec {
            id: g.id,
            symbols: g.symbols.into_iter().map(|s| SymbolRef {
                qualified_name: s.qualified_name,
                file_path: s.file_path,
                kind: s.kind,
            }).collect(),
            agent_prompt: g.agent_prompt,
            status: GroupStatus::Pending,
        };
        spec.save(&ctx.paths, &sid)?;
    }

    let resp = ExecuteBeginResponse {
        session_id: sid.to_string(),
        dk_branch: branch::dk_branch_name(sid.as_str()),
        group_ids,
    };
    *active = Some(sid);
    Ok(resp)
}

impl McpServer {
    #[tool(description = "Begin execution: mint session id, create dk/<sid> branch off main, persist manifest + per-group specs.")]
    pub async fn dkod_execute_begin(
        &self,
        Parameters(req): Parameters<ExecuteBeginRequest>,
    ) -> std::result::Result<ExecuteBeginResponse, rmcp::ErrorData> {
        execute_begin(&self.ctx, req).await.map_err(to_rmcp_error)
    }
}
```

- [ ] **Step 4: Register submodule.** Add `pub mod execute_begin;` to `tools/mod.rs`.
- [ ] **Step 5: Run the test — expected pass.** `cargo test -p dkod-mcp --test execute_begin_tool`.
- [ ] **Step 6: Commit.** `git add` files; standard identity-enforced commit.

## Task 10: `dkod_abort` wrapper

**Files:**
- Create: `crates/dkod-mcp/src/tools/abort.rs`
- Modify: `crates/dkod-mcp/src/tools/mod.rs`

- [ ] **Step 1: Write the failing test.**

Append to `execute_begin_tool.rs`:

```rust
#[tokio::test]
async fn abort_destroys_branch_and_clears_session() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let req = ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![],
            agent_prompt: "x".into(),
        }],
    };
    let begin = execute_begin(&ctx, req).await.unwrap();
    let abort_resp = dkod_mcp::tools::abort::abort(&ctx).await.expect("abort");
    assert_eq!(abort_resp.session_id, begin.session_id);

    assert!(ctx.active_session.lock().await.is_none());
    // dk-branch gone.
    let br = std::process::Command::new("git")
        .args(["branch", "--list", &begin.dk_branch])
        .current_dir(&root).output().unwrap();
    assert!(String::from_utf8(br.stdout).unwrap().trim().is_empty());
}

#[tokio::test]
async fn abort_without_session_errors() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let err = dkod_mcp::tools::abort::abort(&ctx).await.unwrap_err();
    assert!(matches!(err, dkod_mcp::Error::NoActiveSession));
}
```

- [ ] **Step 2: Run — fail.**
- [ ] **Step 3: Implement `abort.rs`.**

```rust
use crate::schema::AbortResponse;
use crate::tools::McpServer;
use crate::tools::plan::to_rmcp_error;
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{Manifest, SessionStatus, branch};
use rmcp::tool;

pub async fn abort(ctx: &ServerCtx) -> Result<AbortResponse> {
    let mut active = ctx.active_session.lock().await;
    let sid = active.as_ref().ok_or(Error::NoActiveSession)?.clone();

    let main = branch::detect_main(&ctx.repo_root)?;
    branch::destroy_dk_branch(&ctx.repo_root, &main, sid.as_str())?;

    // Mark manifest Aborted so recovery does not pick it up as executing.
    if let Ok(mut m) = Manifest::load(&ctx.paths, &sid) {
        m.status = SessionStatus::Aborted;
        m.save(&ctx.paths)?;
    }

    *active = None;
    // Drop any file locks held for this session.
    ctx.file_locks.lock().await.clear();
    Ok(AbortResponse { session_id: sid.to_string() })
}

impl McpServer {
    #[tool(description = "Abort the active session: destroy dk/<sid>, mark manifest Aborted, clear in-memory state.")]
    pub async fn dkod_abort(&self) -> std::result::Result<AbortResponse, rmcp::ErrorData> {
        abort(&self.ctx).await.map_err(to_rmcp_error)
    }
}
```

- [ ] **Step 4: Register submodule.**
- [ ] **Step 5: Run the test — pass.**
- [ ] **Step 6: Commit.**

## Task 11: Recovery on restart

**Files:**
- Create: `crates/dkod-mcp/src/recovery.rs`
- Modify: `crates/dkod-mcp/src/ctx.rs` (add `ServerCtx::recover`)
- Modify: `crates/dkod-mcp/src/main.rs`
- Modify: `crates/dkod-mcp/src/lib.rs`

- [ ] **Step 1: Write the failing test.**

`crates/dkod-mcp/tests/recovery.rs`:

```rust
mod common { include!("common/mod.rs"); }
use common::init_tempo_repo;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::ServerCtx;
use std::sync::Arc;

#[tokio::test]
async fn fresh_ctx_recovers_executing_session() {
    let (_tmp, root) = init_tempo_repo();
    // Ctx A: begin a session (leaves an Executing manifest on disk).
    {
        let ctx = Arc::new(ServerCtx::new(&root));
        execute_begin(&ctx, ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![],
                agent_prompt: "x".into(),
            }],
        }).await.unwrap();
        // Drop ctx — mimic process restart.
    }
    // Ctx B: fresh process; recovery populates active_session.
    let ctx = ServerCtx::new(&root);
    ctx.recover().await.expect("recover");
    let active = ctx.active_session.lock().await.clone();
    assert!(active.is_some(), "recovery should have picked up the Executing session");
}
```

- [ ] **Step 2: Run — fail.**
- [ ] **Step 3: Implement `recovery.rs`.**

```rust
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{Manifest, Paths, SessionId, SessionStatus};

/// Scan `.dkod/sessions/<id>/manifest.json` and return the id of the first
/// session whose status is `Executing`. `None` if no such session exists.
///
/// If multiple executing sessions are found (should never happen in practice;
/// would only occur from external tampering), returns the first one in
/// directory-scan order — the orchestrator invariant is "at most one active
/// session per repo," and violating it is caller error.
pub fn scan_executing_session(paths: &Paths) -> Result<Option<SessionId>> {
    let dir = paths.sessions_dir();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::Io(e)),
    };
    for entry in rd {
        let entry = entry.map_err(Error::Io)?;
        let ft = entry.file_type().map_err(Error::Io)?;
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let sid = SessionId::from_raw(name);
        match Manifest::load(paths, &sid) {
            Ok(m) if matches!(m.status, SessionStatus::Executing) => {
                return Ok(Some(sid));
            }
            Ok(_) => {}
            Err(_) => {
                // Corrupt or mid-write manifests are skipped; caller handles
                // this out of band if needed.
            }
        }
    }
    Ok(None)
}

impl ServerCtx {
    /// Best-effort recovery: pick up any on-disk Executing session as the
    /// current in-memory session. Called once at startup from `main.rs`.
    pub async fn recover(&self) -> Result<()> {
        if let Some(sid) = scan_executing_session(&self.paths)? {
            let mut active = self.active_session.lock().await;
            if active.is_none() {
                *active = Some(sid);
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Register in `lib.rs`.** Add `pub mod recovery;`.
- [ ] **Step 5: Hook into `main.rs`.**

Edit `crates/dkod-mcp/src/main.rs` — call `ctx.recover().await?` before `serve`:

```rust
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;
    let ctx = Arc::new(ServerCtx::new(&repo_root));
    ctx.recover().await?;
    let service = McpServer::new(ctx).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 6: Run the test — pass.**
- [ ] **Step 7: Commit.**

## PR M2-3 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green.
- [ ] PR `M2-3: dkod_execute_begin + dkod_abort + recovery`. Merge autonomously.

---

# PR M2-4 — `dkod_write_symbol` with per-file lock

## Task 12: Schema + pure wrapper

**Files:**
- Modify: `crates/dkod-mcp/src/schema.rs`
- Create: `crates/dkod-mcp/src/tools/write_symbol.rs`
- Modify: `crates/dkod-mcp/src/tools/mod.rs`

- [ ] **Step 1: Schema additions.**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WriteSymbolRequest {
    pub group_id: String,
    pub file: PathBuf,
    pub qualified_name: String,
    pub new_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WriteSymbolResponse {
    /// "parsed_ok" or "fallback"
    pub outcome: String,
    /// Populated when outcome == "fallback".
    pub fallback_reason: Option<String>,
    pub bytes_written: usize,
}
```

- [ ] **Step 2: Failing integration test — single write.**

`crates/dkod-mcp/tests/write_symbol_tool.rs`:

```rust
mod common { include!("common/mod.rs"); }
use common::init_tempo_repo;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::write_symbol::write_symbol;
use dkod_mcp::ServerCtx;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn write_symbol_replaces_function_body() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(&ctx, ExecuteBeginRequest {
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
    }).await.unwrap();

    let resp = write_symbol(&ctx, WriteSymbolRequest {
        group_id: "g1".into(),
        file: PathBuf::from("src/lib.rs"),
        qualified_name: "a".into(),
        new_body: "pub fn a() { /* rewritten */ }".into(),
    }).await.expect("write");
    assert_eq!(resp.outcome, "parsed_ok");
    assert!(resp.bytes_written > 0);

    // Disk now contains the new body.
    let src = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(src.contains("/* rewritten */"));

    // writes.jsonl has one record.
    let log = dkod_worktree::WriteLog::open(
        &ctx.paths,
        ctx.active_session.lock().await.as_ref().unwrap(),
        "g1",
    ).unwrap();
    let records = log.read_all().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].symbol, "a");
}
```

- [ ] **Step 3: Run — fail.**

- [ ] **Step 4: Implement `write_symbol.rs`.**

```rust
use crate::schema::{WriteSymbolRequest, WriteSymbolResponse};
use crate::tools::McpServer;
use crate::tools::plan::to_rmcp_error;
use crate::{Error, Result, ServerCtx};
use dkod_orchestrator::replace::{ReplaceOutcome, replace_symbol};
use dkod_worktree::{WriteLog, WriteRecord};
use rmcp::{handler::server::wrapper::Parameters, tool};

pub async fn write_symbol(
    ctx: &ServerCtx,
    req: WriteSymbolRequest,
) -> Result<WriteSymbolResponse> {
    let sid = ctx.active_session.lock().await.clone()
        .ok_or(Error::NoActiveSession)?;

    let abs = ctx.repo_root.join(&req.file);
    // Canonicalize so that symlinks or `./` prefixes do not create multiple
    // distinct lock entries for the same file.
    let canonical = std::fs::canonicalize(&abs).unwrap_or(abs.clone());

    let lock = ctx.file_lock(&canonical).await;
    let _guard = lock.lock().await;

    // Read → replace → write. The lock scope covers every step — two
    // concurrent writes to the same file serialise through `_guard`.
    let bytes = std::fs::read(&abs).map_err(Error::Io)?;
    let outcome = replace_symbol(&bytes, &req.qualified_name, &req.new_body)?;
    let (new_source, outcome_label, reason) = match outcome {
        ReplaceOutcome::ParsedOk { new_source } => (new_source, "parsed_ok", None),
        ReplaceOutcome::Fallback { new_source, reason } => {
            (new_source, "fallback", Some(reason))
        }
    };
    std::fs::write(&abs, &new_source).map_err(Error::Io)?;

    // Append to writes.jsonl (WriteLog uses append mode; concurrent appends
    // within the same file are serialised by our file lock above — we do
    // not rely on POSIX append atomicity).
    let log = WriteLog::open(&ctx.paths, &sid, &req.group_id)?;
    log.append(&WriteRecord {
        symbol: req.qualified_name,
        file_path: req.file,
        timestamp: crate::time::iso8601_now(),
    })?;

    Ok(WriteSymbolResponse {
        outcome: outcome_label.into(),
        fallback_reason: reason,
        bytes_written: new_source.len(),
    })
}

impl McpServer {
    #[tool(description = "AST-level symbol replacement: holds a per-file lock, replaces the named symbol with new_body, re-parses, and appends to writes.jsonl.")]
    pub async fn dkod_write_symbol(
        &self,
        Parameters(req): Parameters<WriteSymbolRequest>,
    ) -> std::result::Result<WriteSymbolResponse, rmcp::ErrorData> {
        write_symbol(&self.ctx, req).await.map_err(to_rmcp_error)
    }
}
```

- [ ] **Step 5: Run the test — pass.**
- [ ] **Step 6: Commit.**

## Task 13: Concurrent-write lock test

This is the test that justifies the per-file lock's existence. It proves that two async writes to the same file are **serialised** (second write sees the first write's output, neither loses the other's change).

**Files:**
- Create: `crates/dkod-mcp/tests/write_symbol_lock.rs`

- [ ] **Step 1: Seed a fixture with two target symbols in the same file.**

The `tiny_rust` fixture already has four top-level fns in `src/lib.rs`. We'll rewrite `a` and `b` concurrently.

- [ ] **Step 2: Write the test.**

```rust
mod common { include!("common/mod.rs"); }
use common::init_tempo_repo;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::write_symbol::write_symbol;
use dkod_mcp::ServerCtx;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writes_to_same_file_serialise() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(&ctx, ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![
                SymbolRefSchema { qualified_name: "a".into(), file_path: PathBuf::from("src/lib.rs"), kind: "function".into() },
                SymbolRefSchema { qualified_name: "b".into(), file_path: PathBuf::from("src/lib.rs"), kind: "function".into() },
            ],
            agent_prompt: "rewrite a and b".into(),
        }],
    }).await.unwrap();

    let mut handles = Vec::new();
    for (name, marker) in [("a", "MARK_A"), ("b", "MARK_B")] {
        let ctx = Arc::clone(&ctx);
        let name = name.to_string();
        let marker = marker.to_string();
        handles.push(tokio::spawn(async move {
            write_symbol(&ctx, WriteSymbolRequest {
                group_id: "g1".into(),
                file: PathBuf::from("src/lib.rs"),
                qualified_name: name.clone(),
                new_body: format!("pub fn {name}() {{ /* {marker} */ }}"),
            }).await.unwrap()
        }));
    }
    for h in handles { h.await.unwrap(); }

    let src = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    // Both markers present → both writes landed on the final file.
    assert!(src.contains("MARK_A"), "a's rewrite lost, file:\n{src}");
    assert!(src.contains("MARK_B"), "b's rewrite lost, file:\n{src}");

    let log = dkod_worktree::WriteLog::open(
        &ctx.paths,
        ctx.active_session.lock().await.as_ref().unwrap(),
        "g1",
    ).unwrap();
    assert_eq!(log.read_all().unwrap().len(), 2);
}
```

- [ ] **Step 3: Run — expected pass.** If either marker is missing, the lock is wrong. Do not proceed until both are present across multiple runs.

Run it 5 times to exercise scheduler variance:
```sh
for i in 1 2 3 4 5; do cargo test -p dkod-mcp --test write_symbol_lock -- --nocapture; done
```
All 5 runs pass.

- [ ] **Step 4: Commit.**

## Task 14: Raw end-to-end write via MCP client

**Files:**
- Append to `crates/dkod-mcp/tests/write_symbol_tool.rs`

- [ ] **Step 1: Add a `spawn_in_process_server`-driven test** that calls `dkod_execute_begin` then `dkod_write_symbol` via `client.call_tool`. Mirrors the pattern in Task 7.

- [ ] **Step 2: Run — pass.**
- [ ] **Step 3: Commit.**

## PR M2-4 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green.
- [ ] PR `M2-4: dkod_write_symbol + per-file lock`. Merge autonomously.

---

# PR M2-5 — `dkod_execute_complete` + `dkod_status`

## Task 15: Schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteCompleteRequest {
    pub group_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecuteCompleteResponse {
    pub group_id: String,
    pub new_status: String, // "done" | "failed"
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StatusResponse {
    pub active_session_id: Option<String>,
    pub dk_branch: Option<String>,
    pub groups: Vec<GroupStatusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupStatusEntry {
    pub id: String,
    pub status: String,
    pub writes: usize,
    pub agent_summary: Option<String>,
}
```

## Task 16: `dkod_execute_complete` wrapper

**Files:**
- Create: `crates/dkod-mcp/src/tools/execute_complete.rs`
- Modify: `crates/dkod-mcp/src/tools/mod.rs`

- [ ] **Step 1: Failing test — loading the updated spec back shows `Done` + `summary`.**

```rust
mod common { include!("common/mod.rs"); }
use common::init_tempo_repo;
use dkod_mcp::schema::{ExecuteBeginRequest, ExecuteCompleteRequest, GroupInput};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::execute_complete::execute_complete;
use dkod_mcp::ServerCtx;
use dkod_worktree::{GroupSpec, GroupStatus};
use std::sync::Arc;

#[tokio::test]
async fn execute_complete_marks_group_done() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(&ctx, ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput { id: "g1".into(), symbols: vec![], agent_prompt: "x".into() }],
    }).await.unwrap();
    let resp = execute_complete(&ctx, ExecuteCompleteRequest {
        group_id: "g1".into(), summary: "all done".into(),
    }).await.unwrap();
    assert_eq!(resp.new_status, "done");

    let sid = ctx.active_session.lock().await.clone().unwrap();
    let spec = GroupSpec::load(&ctx.paths, &sid, "g1").unwrap();
    assert!(matches!(spec.status, GroupStatus::Done));
    // Summary is persisted by appending a " — <summary>" suffix to agent_prompt,
    // since GroupSpec has no dedicated summary field. See impl below.
    assert!(spec.agent_prompt.contains("all done"));
}
```

- [ ] **Step 2: Run — fail.**
- [ ] **Step 3: Implement.**

```rust
use crate::schema::{ExecuteCompleteRequest, ExecuteCompleteResponse};
use crate::tools::McpServer;
use crate::tools::plan::to_rmcp_error;
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{GroupSpec, GroupStatus};
use rmcp::{handler::server::wrapper::Parameters, tool};

pub async fn execute_complete(
    ctx: &ServerCtx,
    req: ExecuteCompleteRequest,
) -> Result<ExecuteCompleteResponse> {
    let sid = ctx.active_session.lock().await.clone()
        .ok_or(Error::NoActiveSession)?;
    let mut spec = GroupSpec::load(&ctx.paths, &sid, &req.group_id)
        .map_err(|_| Error::UnknownGroup(req.group_id.clone()))?;
    spec.status = GroupStatus::Done;
    // Persist summary. GroupSpec does not yet carry a dedicated summary
    // field; appending to agent_prompt keeps the M2 diff minimal. If a
    // dedicated field is needed later, add it in a `dkod-worktree` PR.
    spec.agent_prompt = format!("{} — summary: {}", spec.agent_prompt.trim_end(), req.summary);
    spec.save(&ctx.paths, &sid)?;
    Ok(ExecuteCompleteResponse { group_id: req.group_id, new_status: "done".into() })
}

impl McpServer {
    #[tool(description = "Mark a group done; records the agent's summary on the group spec.")]
    pub async fn dkod_execute_complete(
        &self,
        Parameters(req): Parameters<ExecuteCompleteRequest>,
    ) -> std::result::Result<ExecuteCompleteResponse, rmcp::ErrorData> {
        execute_complete(&self.ctx, req).await.map_err(to_rmcp_error)
    }
}
```

- [ ] **Step 4: Run — pass. Commit.**

## Task 17: `dkod_status` (read-only)

**Files:**
- Create: `crates/dkod-mcp/src/tools/status.rs`

- [ ] **Step 1: Failing test — status reflects current groups, writes count, and dk-branch.**

```rust
#[tokio::test]
async fn status_reports_active_session_and_groups() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(&ctx, ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput { id: "g1".into(), symbols: vec![], agent_prompt: "x".into() }],
    }).await.unwrap();
    let s = dkod_mcp::tools::status::status(&ctx).await.unwrap();
    assert!(s.active_session_id.is_some());
    assert_eq!(s.groups.len(), 1);
    assert_eq!(s.groups[0].id, "g1");
    assert_eq!(s.groups[0].status, "pending");
    assert_eq!(s.groups[0].writes, 0);
}

#[tokio::test]
async fn status_is_empty_when_no_session() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let s = dkod_mcp::tools::status::status(&ctx).await.unwrap();
    assert!(s.active_session_id.is_none());
    assert!(s.groups.is_empty());
}
```

- [ ] **Step 2: Run — fail.**
- [ ] **Step 3: Implement.**

```rust
use crate::schema::{GroupStatusEntry, StatusResponse};
use crate::tools::McpServer;
use crate::tools::plan::to_rmcp_error;
use crate::{Result, ServerCtx};
use dkod_worktree::{GroupSpec, GroupStatus, Manifest, WriteLog, branch};
use rmcp::tool;

pub async fn status(ctx: &ServerCtx) -> Result<StatusResponse> {
    let active = ctx.active_session.lock().await.clone();
    let Some(sid) = active else {
        return Ok(StatusResponse { active_session_id: None, dk_branch: None, groups: vec![] });
    };
    let manifest = Manifest::load(&ctx.paths, &sid)?;
    let mut groups = Vec::with_capacity(manifest.group_ids.len());
    for gid in &manifest.group_ids {
        let spec = match GroupSpec::load(&ctx.paths, &sid, gid) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let writes = WriteLog::open(&ctx.paths, &sid, gid)
            .and_then(|l| l.read_all())
            .map(|v| v.len())
            .unwrap_or(0);
        let status = match spec.status {
            GroupStatus::Pending => "pending",
            GroupStatus::InProgress => "in_progress",
            GroupStatus::Done => "done",
            GroupStatus::Failed => "failed",
        };
        groups.push(GroupStatusEntry {
            id: gid.clone(),
            status: status.into(),
            writes,
            agent_summary: Some(spec.agent_prompt),
        });
    }
    Ok(StatusResponse {
        active_session_id: Some(sid.to_string()),
        dk_branch: Some(branch::dk_branch_name(sid.as_str())),
        groups,
    })
}

impl McpServer {
    #[tool(description = "Return the active session id, dk-branch, and per-group status + write count.")]
    pub async fn dkod_status(&self) -> std::result::Result<StatusResponse, rmcp::ErrorData> {
        status(&self.ctx).await.map_err(to_rmcp_error)
    }
}
```

- [ ] **Step 4: Run — pass. Commit.**

## PR M2-5 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green.
- [ ] PR `M2-5: dkod_execute_complete + dkod_status`. Merge autonomously.

---

# PR M2-6 — `dkod_commit`

## Task 18: Schema + wrapper

**Files:**
- Modify: `crates/dkod-mcp/src/schema.rs`
- Create: `crates/dkod-mcp/src/tools/commit.rs`

- [ ] **Step 1: Schema.**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommitResponse {
    pub commits_created: usize,
    pub dk_branch: String,
    /// Short hex SHAs of each commit, in order.
    pub commit_shas: Vec<String>,
}
```

- [ ] **Step 2: Failing test.**

```rust
#[tokio::test]
async fn commit_writes_one_commit_per_group_with_writes() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(&ctx, ExecuteBeginRequest {
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
    }).await.unwrap();
    write_symbol(&ctx, WriteSymbolRequest {
        group_id: "g1".into(),
        file: PathBuf::from("src/lib.rs"),
        qualified_name: "a".into(),
        new_body: "pub fn a() { /* x */ }".into(),
    }).await.unwrap();

    let resp = dkod_mcp::tools::commit::commit(&ctx).await.unwrap();
    assert_eq!(resp.commits_created, 1);
    assert_eq!(resp.commit_shas.len(), 1);

    // Latest commit is authored by Haim Ari.
    let log = std::process::Command::new("git")
        .args(["log", "-1", "--format=%an <%ae> | %cn <%ce>"])
        .current_dir(&root).output().unwrap();
    let out = String::from_utf8(log.stdout).unwrap();
    assert!(out.contains("Haim Ari <haimari1@gmail.com>"), "identity wrong: {out}");

    // Commit message matches commit_per_group output.
    let subj = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&root).output().unwrap();
    let subj = String::from_utf8(subj.stdout).unwrap();
    assert!(subj.contains("group g1"), "subject: {subj}");
}
```

- [ ] **Step 3: Run — fail.**
- [ ] **Step 4: Implement.**

```rust
use crate::schema::CommitResponse;
use crate::tools::McpServer;
use crate::tools::plan::to_rmcp_error;
use crate::{Error, Result, ServerCtx};
use dkod_orchestrator::commit::commit_per_group;
use dkod_worktree::{Manifest, branch};
use rmcp::tool;
use std::process::Command;

pub async fn commit(ctx: &ServerCtx) -> Result<CommitResponse> {
    let sid = ctx.active_session.lock().await.clone().ok_or(Error::NoActiveSession)?;
    let manifest = Manifest::load(&ctx.paths, &sid)?;
    let before = git_head_sha(&ctx.repo_root)?;
    commit_per_group(&ctx.repo_root, &ctx.paths, &sid, &manifest.group_ids)?;
    let after = git_head_sha(&ctx.repo_root)?;

    // Collect every new SHA between `before` (exclusive) and `after` (inclusive).
    let range = if before == after {
        Vec::new()
    } else {
        git_rev_list(&ctx.repo_root, &format!("{before}..{after}"))?
    };
    Ok(CommitResponse {
        commits_created: range.len(),
        dk_branch: branch::dk_branch_name(sid.as_str()),
        commit_shas: range,
    })
}

fn git_head_sha(repo: &std::path::Path) -> Result<String> {
    let out = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(repo).output()?;
    if !out.status.success() {
        return Err(Error::InvalidArg(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_rev_list(repo: &std::path::Path, range: &str) -> Result<Vec<String>> {
    let out = Command::new("git")
        .args(["rev-list", "--reverse", "--abbrev-commit", range])
        .current_dir(repo)
        .output()?;
    if !out.status.success() {
        return Err(Error::InvalidArg(format!(
            "git rev-list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

impl McpServer {
    #[tool(description = "Finalize the active session by writing one commit per group (with writes) on the dk-branch. Identity is forced to Haim Ari <haimari1@gmail.com>.")]
    pub async fn dkod_commit(&self) -> std::result::Result<CommitResponse, rmcp::ErrorData> {
        commit(&self.ctx).await.map_err(to_rmcp_error)
    }
}
```

- [ ] **Step 5: Run — pass. Commit.**

## Task 19: Commit updates manifest to `Committed`

**Files:**
- Modify: `crates/dkod-mcp/src/tools/commit.rs`

- [ ] **Step 1: Add a failing assertion to the existing `commit_writes_one_commit_per_group_with_writes` test** right after `commit()`:

```rust
let sid = ctx.active_session.lock().await.clone().unwrap();
let manifest = dkod_worktree::Manifest::load(&ctx.paths, &sid).unwrap();
assert!(matches!(manifest.status, dkod_worktree::SessionStatus::Committed));
```

- [ ] **Step 2: Run — fail.**
- [ ] **Step 3: After the `commit_per_group` call in `commit.rs`, update the manifest:**

```rust
let mut m = Manifest::load(&ctx.paths, &sid)?;
m.status = dkod_worktree::SessionStatus::Committed;
m.save(&ctx.paths)?;
```

- [ ] **Step 4: Run — pass. Commit.**

## PR M2-6 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green.
- [ ] PR `M2-6: dkod_commit`. Merge autonomously.

---

# PR M2-7 — `dkod_pr`

This PR introduces the `gh` subprocess helper + idempotency + verify-command handling. Tests are the tricky part: we can't call real `gh` from CI. Use a `PATH`-shimmed `gh` script that records its args and emits canned JSON.

## Task 20: `gh` helper module

**Files:**
- Create: `crates/dkod-mcp/src/gh.rs`

- [ ] **Step 1: Failing test — `pr_exists` returns URL when shim emits JSON with one entry.**

`crates/dkod-mcp/tests/gh_shim.rs`:

```rust
use std::path::PathBuf;

fn make_shim(tmp: &std::path::Path, body: &str) -> PathBuf {
    let bin_dir = tmp.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("gh");
    std::fs::write(&shim, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&shim).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&shim, perm).unwrap();
    }
    bin_dir
}

#[test]
fn pr_exists_parses_url() {
    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = make_shim(&tmp.path(), r#"echo "https://github.com/x/y/pull/42""#);
    let repo = tempfile::tempdir().unwrap();
    let url = dkod_mcp::gh::pr_exists(repo.path(), "dk/x", Some(&bin_dir)).unwrap();
    assert_eq!(url.as_deref(), Some("https://github.com/x/y/pull/42"));
}

#[test]
fn pr_exists_returns_none_on_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = make_shim(&tmp.path(), "");
    let repo = tempfile::tempdir().unwrap();
    assert_eq!(dkod_mcp::gh::pr_exists(repo.path(), "dk/x", Some(&bin_dir)).unwrap(), None);
}
```

- [ ] **Step 2: Run — fail.**
- [ ] **Step 3: Implement.**

```rust
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run `gh` in `repo`, optionally prefixing `PATH` with `path_prefix` so tests
/// can shim the binary.
fn gh(repo: &Path, args: &[&str], path_prefix: Option<&Path>) -> Result<String> {
    let mut cmd = Command::new("gh");
    cmd.args(args).current_dir(repo);
    if let Some(p) = path_prefix {
        let cur = std::env::var_os("PATH").unwrap_or_default();
        let mut new_path: PathBuf = p.to_path_buf();
        let tail = std::ffi::OsString::from(format!(":{}", cur.to_string_lossy()));
        let mut combined = new_path.into_os_string();
        combined.push(tail);
        cmd.env("PATH", combined);
    }
    let out = cmd.output().map_err(|e| Error::Gh {
        cmd: format!("gh {}", args.join(" ")),
        stderr: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(Error::Gh {
            cmd: format!("gh {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Return the URL of an existing PR whose head is `branch`, or `None`.
pub fn pr_exists(repo: &Path, branch: &str, path_prefix: Option<&Path>) -> Result<Option<String>> {
    let out = gh(repo, &["pr", "list", "--head", branch, "--state", "all",
        "--json", "url", "--jq", ".[0].url // empty"], path_prefix)?;
    if out.is_empty() { Ok(None) } else { Ok(Some(out)) }
}

/// Push `branch` to `origin` with `--force-with-lease` + `--set-upstream`.
pub fn push_branch(repo: &Path, branch: &str) -> Result<()> {
    let out = Command::new("git")
        .args(["push", "--force-with-lease", "--set-upstream", "origin", branch])
        .current_dir(repo)
        .output()
        .map_err(|e| Error::Gh { cmd: "git push".into(), stderr: e.to_string() })?;
    if !out.status.success() {
        return Err(Error::Gh {
            cmd: "git push".into(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(())
}

/// Create a PR and return its URL.
pub fn create_pr(
    repo: &Path, branch: &str, title: &str, body: &str, path_prefix: Option<&Path>,
) -> Result<String> {
    gh(repo, &["pr", "create",
        "--head", branch,
        "--title", title,
        "--body", body], path_prefix)
}
```

- [ ] **Step 4: Register module.** Add `pub mod gh;` to `lib.rs`.
- [ ] **Step 5: Run the test — pass. Commit.**

## Task 21: `dkod_pr` schema + happy-path wrapper

**Files:**
- Modify: `crates/dkod-mcp/src/schema.rs`
- Create: `crates/dkod-mcp/src/tools/pr.rs`

- [ ] **Step 1: Schema.**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PrRequest {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PrResponse {
    pub url: String,
    /// True if the returned URL references a PR that already existed before this call.
    pub was_existing: bool,
}
```

- [ ] **Step 2: Failing test — idempotent: second call returns same URL without hitting `gh pr create`.**

```rust
#[tokio::test]
async fn pr_is_idempotent_when_already_open() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(&ctx, ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput {
            id: "g1".into(),
            symbols: vec![SymbolRefSchema {
                qualified_name: "a".into(), file_path: PathBuf::from("src/lib.rs"), kind: "function".into(),
            }],
            agent_prompt: "x".into(),
        }],
    }).await.unwrap();
    write_symbol(&ctx, WriteSymbolRequest {
        group_id: "g1".into(),
        file: PathBuf::from("src/lib.rs"),
        qualified_name: "a".into(),
        new_body: "pub fn a() { /* x */ }".into(),
    }).await.unwrap();
    commit(&ctx).await.unwrap();

    // Shim gh to report an existing PR.
    let bin_dir = make_shim_always_existing(&root);
    let resp = pr_with_shim(&ctx, PrRequest {
        title: "t".into(), body: "b".into(),
    }, &bin_dir).await.unwrap();
    assert!(resp.was_existing);
    assert!(resp.url.ends_with("/pull/42"));
}
```

(Helper `make_shim_always_existing` in `common/mod.rs` — a `gh` script that always echoes the fake URL for `pr list`, aborts on `pr create`.)

- [ ] **Step 3: Run — fail.**
- [ ] **Step 4: Implement `pr.rs`.**

```rust
use crate::gh;
use crate::schema::{PrRequest, PrResponse};
use crate::tools::McpServer;
use crate::tools::plan::to_rmcp_error;
use crate::{Error, Result, ServerCtx};
use dkod_worktree::{Config, Manifest, SessionStatus, branch};
use rmcp::{handler::server::wrapper::Parameters, tool};
use std::path::Path;
use std::process::Command;

pub async fn pr(ctx: &ServerCtx, req: PrRequest) -> Result<PrResponse> {
    pr_with_shim(ctx, req, None).await
}

pub async fn pr_with_shim(
    ctx: &ServerCtx, req: PrRequest, path_prefix: Option<&Path>,
) -> Result<PrResponse> {
    let sid = ctx.active_session.lock().await.clone().ok_or(Error::NoActiveSession)?;
    let br = branch::dk_branch_name(sid.as_str());

    // 1. Run verify_cmd if configured.
    let cfg = Config::load(&ctx.paths.config())?;
    if let Some(cmd) = cfg.verify_cmd.as_deref() {
        run_verify(&ctx.repo_root, cmd)?;
    }

    // 2. Idempotency check BEFORE pushing.
    if let Some(url) = gh::pr_exists(&ctx.repo_root, &br, path_prefix)? {
        return Ok(PrResponse { url, was_existing: true });
    }

    // 3. Push.
    gh::push_branch(&ctx.repo_root, &br)?;

    // 4. Re-check after push (another process may have raced).
    if let Some(url) = gh::pr_exists(&ctx.repo_root, &br, path_prefix)? {
        return Ok(PrResponse { url, was_existing: true });
    }

    // 5. Create.
    let url = gh::create_pr(&ctx.repo_root, &br, &req.title, &req.body, path_prefix)?;

    // 6. Mark manifest Committed (already set by dkod_commit) — no transition here.
    //    We intentionally do NOT clear the active session: abort/close is a
    //    separate decision left to the caller.
    let _ = Manifest::load(&ctx.paths, &sid)
        .map(|m| (m.status == SessionStatus::Committed));

    Ok(PrResponse { url, was_existing: false })
}

fn run_verify(repo: &Path, cmd: &str) -> Result<()> {
    let out = Command::new("sh").arg("-c").arg(cmd).current_dir(repo).output()?;
    if !out.status.success() {
        let tail = String::from_utf8_lossy(&out.stderr)
            .lines().rev().take(10)
            .collect::<Vec<_>>().iter().rev().cloned().collect::<Vec<_>>().join("\n");
        return Err(Error::VerifyFailed {
            exit: out.status.code().unwrap_or(-1),
            tail,
        });
    }
    Ok(())
}

impl McpServer {
    #[tool(description = "Run verify_cmd, push dk/<sid> with --force-with-lease, and create a PR via gh (idempotent: returns existing PR url if one is already open).")]
    pub async fn dkod_pr(
        &self,
        Parameters(req): Parameters<PrRequest>,
    ) -> std::result::Result<PrResponse, rmcp::ErrorData> {
        pr(&self.ctx, req).await.map_err(to_rmcp_error)
    }
}
```

- [ ] **Step 5: Run the test — pass. Commit.**

## Task 22: Verify-fail test

- [ ] **Step 1: Write a test that sets `verify_cmd = "false"` in `.dkod/config.toml` and asserts `dkod_pr` errors with `VerifyFailed`.**

```rust
#[tokio::test]
async fn pr_errors_when_verify_cmd_fails() {
    let (_tmp, root) = init_tempo_repo();
    // Override verify_cmd to always-fail.
    let cfg = dkod_worktree::Config { main_branch: "main".into(), verify_cmd: Some("false".into()) };
    cfg.save(&root.join(".dkod/config.toml")).unwrap();

    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(&ctx, ExecuteBeginRequest {
        task_prompt: "demo".into(),
        groups: vec![GroupInput { id: "g1".into(), symbols: vec![], agent_prompt: "x".into() }],
    }).await.unwrap();
    let err = dkod_mcp::tools::pr::pr(&ctx, dkod_mcp::schema::PrRequest {
        title: "t".into(), body: "b".into(),
    }).await.unwrap_err();
    assert!(matches!(err, dkod_mcp::Error::VerifyFailed { .. }));
}
```

- [ ] **Step 2: Run — pass (`verify_cmd` handling is already implemented in Task 21).**
- [ ] **Step 3: Commit.**

## Task 23: Existing-branch-no-remote edge case

- [ ] **Step 1: Write a test that shims `gh pr list` to return empty AND `gh pr create` to return a canned URL. Asserts `was_existing == false` and the URL comes through.**

(Confirms that in the happy-path `create_pr` branch of Task 21 actually runs.)

- [ ] **Step 2: Run — pass. Commit.**

## PR M2-7 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green.
- [ ] PR `M2-7: dkod_pr (idempotent) + gh subprocess helper`. Merge autonomously.

---

# PR M2-8 — End-to-end smoke test

## Task 24: Full flow via in-process client

**Files:**
- Create: `crates/dkod-mcp/tests/e2e_smoke.rs`

The full flow exercised: `dkod_plan` → `dkod_execute_begin` → (parallel) two `dkod_write_symbol` calls → two `dkod_execute_complete` → `dkod_commit` → `dkod_pr`. Last step uses the `gh` shim.

- [ ] **Step 1: Write the test.**

```rust
mod common { include!("common/mod.rs"); }
use common::{init_tempo_repo, spawn_in_process_server, make_gh_shim};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_plan_to_pr_flow() {
    let (_tmp, root) = init_tempo_repo();
    let bin_dir = make_gh_shim(&root, "https://github.com/fake/repo/pull/1");
    let client = spawn_in_process_server(&root).await;

    // 1. plan
    let plan = client.call_tool_json("dkod_plan", json!({
        "task_prompt": "demo", "in_scope": ["a", "b", "c", "d"],
        "files": ["src/lib.rs"], "target_groups": 2,
    })).await;
    let groups = plan["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);

    // 2. execute_begin — forward the plan's groups as the input.
    let begin = client.call_tool_json("dkod_execute_begin", json!({
        "task_prompt": "demo", "groups": groups.iter().map(|g| json!({
            "id": g["id"],
            "symbols": g["symbols"],
            "agent_prompt": "rewrite",
        })).collect::<Vec<_>>(),
    })).await;
    let session_id = begin["session_id"].as_str().unwrap().to_string();

    // 3. two writes in parallel (different symbols in same file).
    let w_a = client.call_tool_json("dkod_write_symbol", json!({
        "group_id": groups[0]["id"], "file": "src/lib.rs",
        "qualified_name": "a", "new_body": "pub fn a() { /* A */ }",
    }));
    let w_b = client.call_tool_json("dkod_write_symbol", json!({
        "group_id": groups[0]["id"], "file": "src/lib.rs",
        "qualified_name": "b", "new_body": "pub fn b() { /* B */ }",
    }));
    let (wa, wb) = tokio::join!(w_a, w_b);
    assert_eq!(wa["outcome"], "parsed_ok");
    assert_eq!(wb["outcome"], "parsed_ok");

    // 4. complete each group.
    for g in groups {
        client.call_tool_json("dkod_execute_complete", json!({
            "group_id": g["id"], "summary": "done",
        })).await;
    }

    // 5. commit
    let commit = client.call_tool_json("dkod_commit", json!({})).await;
    assert!(commit["commits_created"].as_u64().unwrap() >= 1);

    // 6. pr — shim gh by prepending `bin_dir` to PATH.
    let saved_path = std::env::var_os("PATH");
    let new_path = format!("{}:{}", bin_dir.display(),
        saved_path.as_deref().and_then(|p| p.to_str()).unwrap_or(""));
    // SAFETY: this test is `#[tokio::test]` in its own process; no sibling
    // tests in this file touch PATH. Restored below.
    unsafe { std::env::set_var("PATH", &new_path); }
    let pr = call_tool_json(&client, "dkod_pr", json!({"title": "t", "body": "b"})).await;
    if let Some(p) = saved_path { unsafe { std::env::set_var("PATH", p); } }
    assert!(pr["url"].as_str().unwrap().contains("/pull/"));

    // 7. status reflects a committed session.
    let st = client.call_tool_json("dkod_status", json!({})).await;
    assert_eq!(st["active_session_id"], session_id);
    assert!(st["groups"].as_array().unwrap().iter().all(|g| g["status"] == "done"));

    client.cancel().await.ok();
}
```

> **Helper method note:** `call_tool_json`, `call_tool_json_with_env`, and `make_gh_shim` are thin convenience wrappers added to `common/mod.rs` for this PR. `call_tool_json` unwraps the first content block as JSON; `call_tool_json_with_env` does the same but spawns the server with an extended `PATH` so the shimmed `gh` is found. Implement both in Task 25.

- [ ] **Step 2: Run — compile error (helpers missing).**

## Task 25: `common` helpers for tool-by-name JSON calls + env override

**Files:**
- Modify: `crates/dkod-mcp/tests/common/mod.rs`

- [ ] **Step 1: Add `call_tool_json` and `make_gh_shim` as free functions.**

```rust
use serde_json::Value;

pub async fn call_tool_json(
    client: &rmcp::service::RunningService<rmcp::RoleClient, rmcp::model::InitializeRequestParam>,
    name: &str, args: Value,
) -> Value {
    let obj = args.as_object().cloned().unwrap_or_default();
    let result = client.call_tool(rmcp::model::CallToolRequestParam {
        name: name.into(),
        arguments: Some(obj.into_iter().collect()),
    }).await.expect("call_tool");
    // rmcp 1.5 returns `Vec<Content>`; pull the first text block and parse.
    for c in result.content {
        if let Some(t) = c.raw.as_text() {
            return serde_json::from_str(&t.text).unwrap_or(Value::String(t.text.clone()));
        }
    }
    Value::Null
}

pub fn make_gh_shim(root: &std::path::Path, url: &str) -> std::path::PathBuf {
    let bin_dir = root.join(".bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("gh");
    // Behave like: `pr list ...` → empty; `pr create ...` → <url>; other → exit 0
    let body = format!(r#"#!/bin/sh
case "$1 $2" in
  "pr list") exit 0 ;;
  "pr create") echo "{url}"; exit 0 ;;
  *) exit 0 ;;
esac"#);
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
```

> **Primary path:** do NOT add `call_tool_json_with_env`. The rmcp server runs in-process, so `std::env::set_var("PATH", …)` at the top of the e2e test is enough for the shimmed `gh` to be found by the `Command::new("gh")` inside `gh::pr_exists` / `create_pr`. Save and restore `PATH` around the test body to avoid leaking into sibling tests (or use a `Drop` guard struct). The `path_prefix` argument on `gh.rs` helpers (Task 20) stays — it is used by `gh_shim.rs` unit tests that run outside the server context.

- [ ] **Step 2: Run the smoke test.** `cargo test -p dkod-mcp --test e2e_smoke -- --nocapture` → PASS.
- [ ] **Step 3: Commit.**

## Task 26: Spec-mapping table in the README

**Files:**
- Modify: `README.md` (root)

- [ ] **Step 1: Replace the "Status" section** that says "Design phase. No code yet." with:

```markdown
## Status

**v0 in flight — milestones 1 & 2 merged.** `cargo test --workspace` is green across 8 PRs of M1 + 8 PRs of M2.

The full design lives in [`docs/design.md`](docs/design.md). Milestones 3+ (CLI wrapper, plugin manifest + skill, E2E smoke on a real Rust sandbox) are the remaining ship items.
```

- [ ] **Step 2: Commit (docs-only — skip `/coderabbit:code-review` and note so in the PR body).**

```sh
git add README.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Update README: M2 landed, 8 PRs merged"
```

## PR M2-8 wrap-up

- [ ] `/coderabbit:code-review` on the Rust changes (skip for README-only tail commit); `cargo test --workspace` green.
- [ ] PR `M2-8: end-to-end MCP smoke test`. Merge autonomously.
- [ ] Tag `v0.2.0-m2` on `main`:

```sh
git checkout main && git pull
git tag -a v0.2.0-m2 -m "Milestone 2: dkod-mcp with the 8-tool surface"
git push origin v0.2.0-m2
```

---

## Milestone 2 exit criteria

1. `cargo test --workspace` green across all 8 PRs merged to `main`.
2. All 8 MCP tools addressable by name via an in-process rmcp client.
3. `dkod_write_symbol` serialises concurrent writes to the same file (proven by `write_symbol_lock.rs` passing 5/5 runs).
4. `dkod_pr` is idempotent: calling twice in a row returns the same URL, not a new PR (proven by `pr_tool.rs::pr_is_idempotent_when_already_open`).
5. A fresh `dkod-mcp` process recovers an `Executing` session from disk (proven by `recovery.rs::fresh_ctx_recovers_executing_session`).
6. All commits on `main` authored AND committed by `Haim Ari <haimari1@gmail.com>`. No `Co-Authored-By` trailers anywhere in M2 history. Verified with the controller-side check in `~/.claude/memory/tools/git-subagent-commits.md`.

## Out of scope (M3+)

- CLI wrapper (`dkod` binary with `init`/`status`/`abort`/`--mcp` subcommands). M2 ships `dkod-mcp` as a bin already — `dkod-cli` will re-export or embed it.
- Plugin manifest (`plugin/plugin.json`), skill authoring, slash commands. M4 territory.
- E2E smoke on a real Rust sandbox with wall-clock measurement vs serial. M5.
- `dkod-mcp` test harness against the actual Claude Code process (rather than in-process). Nice-to-have; not required for M2 exit.
