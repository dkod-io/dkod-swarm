# Milestone 3: `dkod-cli` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `dkod-cli` — the user-facing `dkod` binary that wraps M1 (`dkod-worktree`) + M2 (`dkod-mcp`) into the four CLI surfaces specified by `docs/design.md` §Repo layout: `dkod init`, `dkod status`, `dkod abort`, `dkod --mcp`. The first three are direct calls into M1/M2 library helpers; `--mcp` reuses the existing `McpServer::new(ctx).serve(stdio()).await` flow that already lives in `dkod-mcp`'s binary entry. Milestone ends with a green `cargo test --workspace` plus an integration test that drives the compiled `dkod` binary through `dkod init` + `dkod status` against a tempdir repo.

**Architecture:** New crate `crates/dkod-cli` (lib + bin). The `dkod` binary parses subcommands with `clap` (derive macros) and dispatches:

- `dkod init [--verify-cmd CMD]` → `dkod_worktree::init_repo(cwd, verify_cmd)`
- `dkod status` → constructs `ServerCtx`, runs `recover().await`, calls `dkod_mcp::tools::status::status(&ctx).await`, prints the resulting `StatusResponse` as pretty JSON
- `dkod abort` → `ServerCtx::new + recover`, then `dkod_mcp::tools::abort::abort(&ctx).await`, prints `AbortResponse` as JSON
- `dkod --mcp` → identical to the current `dkod-mcp` binary's `main`: build `Arc<ServerCtx>`, recover, `McpServer::new(ctx).serve(stdio()).await`

The existing `dkod-mcp` binary stays in place for now (M3 scope is to ADD the user-facing `dkod` binary, not deprecate the developer-facing one). Removal/rename is a future-milestone decision.

**Tech Stack:** Rust 2024. New crate deps: `clap 4` (with `derive` feature), plus `dkod-worktree`, `dkod-mcp`, `tokio`, `serde_json`, `anyhow`, and dev-deps `tempfile` + `assert_cmd` (for invoking the compiled `dkod` binary in integration tests). M3-1 only pulls in `dkod-worktree` + `clap` + `tokio` + `anyhow`; the rest land per-PR as they become needed.

---

## File Structure

New files only. Nothing under `crates/dkod-worktree/`, `crates/dkod-orchestrator/`, or `crates/dkod-mcp/` is modified.

```text
Cargo.toml                                 # +workspace.dependencies: clap, assert_cmd
crates/
└── dkod-cli/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                         # re-exports; minimal public surface for tests
    │   ├── main.rs                        # binary entry; clap dispatch
    │   ├── cli.rs                         # `Cli` derive struct + `Command` enum
    │   └── cmd/
    │       ├── mod.rs                     # re-exports per-subcommand modules
    │       ├── init.rs                    # `dkod init`
    │       ├── status.rs                  # `dkod status` (prints JSON)
    │       ├── abort.rs                   # `dkod abort` (prints JSON)
    │       └── mcp.rs                     # `dkod --mcp` (stdio MCP server)
    └── tests/
        ├── common/
        │   └── mod.rs                     # tempdir-repo helper, uses assert_cmd
        ├── init_cmd.rs                    # `dkod init` writes config.toml + sessions/
        ├── status_cmd.rs                  # `dkod status` outputs the all-empty response
        └── e2e_subprocess.rs              # spawns the real `dkod` binary in a subprocess (init + status round-trip)
```

`lib.rs` exists so unit tests in submodules and the integration tests can reach into pure helpers without invoking the binary every time. The binary just calls into the lib.

---

## PR Plan

Milestone 3 lands in **3 PRs**. Each PR is a feature branch off `main`, opened fresh, and goes through the full CodeRabbit loop per `CLAUDE.md` (local `/coderabbit:code-review` → fix → re-review → commit/push → wait for PR review → `/coderabbit:autofix` → merge autonomously once clean). Branch names match the PR title prefix.

| PR | Branch | Scope | Tasks |
|----|--------|-------|-------|
| M3-1 | `m3/cli-scaffold-init` | new `dkod-cli` crate, clap parser, `dkod init` | 1–4 |
| M3-2 | `m3/cli-status-abort` | `dkod status` + `dkod abort` (read-only + lifecycle) | 5–8 |
| M3-3 | `m3/cli-mcp-and-e2e` | `dkod --mcp` stdio dispatch + subprocess integration test | 9–12 |

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
  No `Co-Authored-By`. Verify with `git log -1 --format='%h | %an <%ae> | %cn <%ce> | %s'` after each commit and grep the body for `Co-Authored-By` (must be empty). Controller-side verification (see `~/.claude/memory/tools/git-subagent-commits.md`) runs after every subagent commit.
- Be preceded by `/coderabbit:code-review` on the local diff vs `main`, unless the changeset is docs/config-only.

Every PR MUST:

- Title ≤ 70 chars.
- Body = short summary + test-plan checklist.
- Open ONE PR at a time. Do not start the next PR's branch until the current one is merged.
- **Merge autonomously** once CodeRabbit is clean and `cargo test --workspace` is green (per project policy — see `feedback_autonomous_merge.md`).

Poller discipline per `~/.claude/memory/tools/coderabbit.md`: arm the 3-condition poller after every push, `TaskStop` stale pollers immediately on merge/close.

---

# PR M3-1 — Scaffold + `dkod init`

## Task 1: Workspace + crate manifest

**Files:**
- Modify: `Cargo.toml` (workspace root) — add `clap`, `assert_cmd` to `[workspace.dependencies]`; add `crates/dkod-cli` to `members`
- Create: `crates/dkod-cli/Cargo.toml`

- [ ] **Step 1: Extend the workspace dependencies and members.**

Edit the workspace `Cargo.toml`. Append to `[workspace.dependencies]`:

```toml
clap = { version = "4", features = ["derive"] }
assert_cmd = "2"
```

Append `"crates/dkod-cli"` to the `members` list.

- [ ] **Step 2: Create `crates/dkod-cli/Cargo.toml`.**

```toml
[package]
name = "dkod-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "User-facing dkod-swarm CLI: init, status, abort, and stdio MCP launcher"

[lib]
path = "src/lib.rs"

[[bin]]
name = "dkod"
path = "src/main.rs"

[dependencies]
dkod-worktree = { path = "../dkod-worktree" }
dkod-orchestrator = { path = "../dkod-orchestrator" }
dkod-mcp = { path = "../dkod-mcp" }
clap.workspace = true
tokio.workspace = true
rmcp.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
anyhow.workspace = true

[dev-dependencies]
tempfile.workspace = true
assert_cmd.workspace = true
```

`[lib]` + `[[bin]]` are spelled out explicitly here (unlike `dkod-mcp`, which used Cargo autodiscovery) because the binary name `dkod` differs from the crate name `dkod-cli`. Without the explicit `[[bin]]` block, autodiscovery would name the binary `dkod-cli`.

- [ ] **Step 3: Verify the workspace resolves.**

```sh
cargo metadata --no-deps --format-version=1 >/dev/null
```

Expected: exits 0. (`cargo metadata` errors on missing source files until at least one source file exists. If it errors here, defer this verification until after Task 2 lands `lib.rs` and `main.rs`.)

- [ ] **Step 4: Branch + commit.**

```sh
git checkout -b m3/cli-scaffold-init
git add Cargo.toml crates/dkod-cli/Cargo.toml
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod-cli crate skeleton + workspace deps"
```

Verify identity + clean Co-Authored-By per `CLAUDE.md`.

## Task 2: clap `Cli` parser

**Files:**
- Create: `crates/dkod-cli/src/lib.rs`
- Create: `crates/dkod-cli/src/cli.rs`
- Create: `crates/dkod-cli/src/cmd/mod.rs`
- Create: `crates/dkod-cli/src/main.rs`

- [ ] **Step 1: Write the failing test — `Cli::parse` accepts each subcommand.**

`crates/dkod-cli/tests/cli_parse.rs`:

```rust
use dkod_cli::cli::{Cli, Command};
use clap::Parser;

#[test]
fn parses_init() {
    let cli = Cli::parse_from(["dkod", "init"]);
    assert!(matches!(cli.command, Command::Init { verify_cmd: None }));
}

#[test]
fn parses_init_with_verify_cmd() {
    let cli = Cli::parse_from(["dkod", "init", "--verify-cmd", "cargo test"]);
    let Command::Init { verify_cmd } = cli.command else { panic!("expected Init") };
    assert_eq!(verify_cmd.as_deref(), Some("cargo test"));
}

#[test]
fn parses_status() {
    let cli = Cli::parse_from(["dkod", "status"]);
    assert!(matches!(cli.command, Command::Status));
}

#[test]
fn parses_abort() {
    let cli = Cli::parse_from(["dkod", "abort"]);
    assert!(matches!(cli.command, Command::Abort));
}

#[test]
fn parses_mcp_flag() {
    let cli = Cli::parse_from(["dkod", "--mcp"]);
    assert!(matches!(cli.command, Command::Mcp));
}
```

- [ ] **Step 2: Run — expected fail.**

`cargo test -p dkod-cli --test cli_parse` → compile error: crate has no `lib.rs` yet.

- [ ] **Step 3: Create `lib.rs`.**

`crates/dkod-cli/src/lib.rs`:

```rust
pub mod cli;
pub mod cmd;
```

- [ ] **Step 4: Create `cli.rs`.**

```rust
use clap::{Parser, Subcommand};

/// `dkod` — user-facing CLI for dkod-swarm.
///
/// `dkod --mcp` launches the stdio MCP server (Claude Code is the
/// expected caller). The other subcommands are operator-facing wrappers
/// over the same `dkod-mcp` library helpers, so output matches what the
/// MCP tools return.
#[derive(Debug, Parser)]
#[command(name = "dkod", version, about = "dkod-swarm CLI")]
pub struct Cli {
    /// Stdio MCP-server mode. Mutually exclusive with the subcommands.
    /// We expose `--mcp` as a top-level flag (not a subcommand) so the
    /// invocation matches design §Topology: `dkod-cli --mcp`.
    #[arg(long, global = false)]
    pub mcp: bool,

    #[command(subcommand)]
    pub subcommand: Option<RawCommand>,
}

#[derive(Debug, Subcommand)]
pub enum RawCommand {
    /// Initialise `.dkod/` in the current directory.
    Init {
        /// Optional shell command to run before `dkod_pr` opens the PR.
        #[arg(long)]
        verify_cmd: Option<String>,
    },
    /// Print the current session state as JSON.
    Status,
    /// Destroy the active dk-branch and clear session state.
    Abort,
}

/// Resolved command after reconciling the `--mcp` flag with the
/// subcommand: exactly one of the variants below is selected.
#[derive(Debug)]
pub enum Command {
    Init { verify_cmd: Option<String> },
    Status,
    Abort,
    Mcp,
}

impl Cli {
    /// Reconciled view: collapses `--mcp` and the subcommand into a
    /// single enum. Errors if both are set or neither is set.
    pub fn command_resolved(&self) -> Result<Command, &'static str> {
        match (self.mcp, &self.subcommand) {
            (true, Some(_)) => Err("--mcp cannot be combined with a subcommand"),
            (true, None) => Ok(Command::Mcp),
            (false, Some(RawCommand::Init { verify_cmd })) => Ok(Command::Init {
                verify_cmd: verify_cmd.clone(),
            }),
            (false, Some(RawCommand::Status)) => Ok(Command::Status),
            (false, Some(RawCommand::Abort)) => Ok(Command::Abort),
            (false, None) => Err("no subcommand given (try `dkod --help`)"),
        }
    }
}
```

> **Note on the test API:** the integration test in Step 1 references `Cli::command` (a single resolved field) but the struct above exposes `Cli::subcommand` plus a `command_resolved()` method. Update the test in Step 1 to call `cli.command_resolved().unwrap()` and adjust the imports accordingly. **Both styles work — pick one before writing this PR; the rest of the plan assumes the resolved-method form.**

Updated test:
```rust
use dkod_cli::cli::{Cli, Command};
use clap::Parser;

#[test]
fn parses_init() {
    let cli = Cli::parse_from(["dkod", "init"]);
    let cmd = cli.command_resolved().unwrap();
    assert!(matches!(cmd, Command::Init { verify_cmd: None }));
}
// ... and so on for the other tests, using `command_resolved()`.
```

- [ ] **Step 5: Create `cmd/mod.rs` (empty for now — fills out per task).**

```rust
//! Per-subcommand dispatch. Each module exposes one `pub async fn run(...)`
//! that takes the parsed argument struct and the working directory.
```

- [ ] **Step 6: Create `main.rs` with a stub dispatch (errors on every subcommand for now).**

```rust
use clap::Parser;
use dkod_cli::cli::{Cli, Command};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("dkod fatal: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cmd = cli.command_resolved().map_err(anyhow::Error::msg)?;
    match cmd {
        Command::Init { verify_cmd: _ } => {
            anyhow::bail!("`dkod init` not yet implemented (Task 3)")
        }
        Command::Status => anyhow::bail!("`dkod status` not yet implemented (PR M3-2)"),
        Command::Abort => anyhow::bail!("`dkod abort` not yet implemented (PR M3-2)"),
        Command::Mcp => anyhow::bail!("`dkod --mcp` not yet implemented (PR M3-3)"),
    }
}
```

- [ ] **Step 7: Run the test — expected pass.**

```sh
cargo test -p dkod-cli --test cli_parse
```

All five tests `ok`.

- [ ] **Step 8: Build the binary.**

```sh
cargo build -p dkod-cli --bin dkod
```

Expected: exits 0. The binary is `target/debug/dkod`.

- [ ] **Step 9: Commit.**

```sh
git add crates/dkod-cli/src/lib.rs crates/dkod-cli/src/cli.rs \
        crates/dkod-cli/src/cmd/mod.rs crates/dkod-cli/src/main.rs \
        crates/dkod-cli/tests/cli_parse.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod CLI parser with stub dispatch"
```

Run `/coderabbit:code-review` first.

## Task 3: `dkod init` implementation

**Files:**
- Create: `crates/dkod-cli/src/cmd/init.rs`
- Modify: `crates/dkod-cli/src/cmd/mod.rs` — add `pub mod init;`
- Modify: `crates/dkod-cli/src/main.rs` — wire init dispatch

- [ ] **Step 1: Write the failing test.**

`crates/dkod-cli/tests/init_cmd.rs`:

```rust
use std::path::PathBuf;

#[test]
fn init_creates_dkod_dir_with_config() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    // Init a git repo so `init_repo`'s `branch::detect_main` succeeds.
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());

    dkod_cli::cmd::init::run(&root, Some("cargo test".into()))
        .expect("init::run");

    let dkod_dir = root.join(".dkod");
    assert!(dkod_dir.is_dir(), ".dkod/ should exist");
    let cfg: PathBuf = dkod_dir.join("config.toml");
    assert!(cfg.is_file(), ".dkod/config.toml should exist");
    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(body.contains("main_branch"));
    assert!(body.contains("verify_cmd"));
    assert!(body.contains("cargo test"));
    assert!(dkod_dir.join("sessions").is_dir());
}

#[test]
fn init_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());

    dkod_cli::cmd::init::run(&root, None).unwrap();
    // First run wrote no verify_cmd — second run with a different value
    // must NOT overwrite it (per `init_repo`'s idempotency contract).
    dkod_cli::cmd::init::run(&root, Some("ignored".into())).unwrap();
    let body = std::fs::read_to_string(root.join(".dkod/config.toml")).unwrap();
    assert!(!body.contains("ignored"), "init must not overwrite an existing config");
}
```

- [ ] **Step 2: Run — expected fail.**

`cargo test -p dkod-cli --test init_cmd` → compile error: `dkod_cli::cmd::init` does not exist.

- [ ] **Step 3: Implement `init.rs`.**

```rust
use std::path::Path;

/// Initialise `.dkod/` under `repo_root`. Idempotent — leaves an existing
/// `config.toml` untouched even if `verify_cmd` differs.
pub fn run(repo_root: &Path, verify_cmd: Option<String>) -> anyhow::Result<()> {
    dkod_worktree::init_repo(repo_root, verify_cmd)
        .map_err(|e| anyhow::anyhow!("dkod_worktree::init_repo failed: {e}"))?;
    println!("Initialised .dkod/ in {}", repo_root.display());
    Ok(())
}
```

- [ ] **Step 4: Register module in `cmd/mod.rs`.**

Append:
```rust
pub mod init;
```

- [ ] **Step 5: Wire init dispatch in `main.rs`.**

Replace the `Command::Init { verify_cmd: _ } => …` arm with:
```rust
Command::Init { verify_cmd } => {
    let cwd = std::env::current_dir()?;
    dkod_cli::cmd::init::run(&cwd, verify_cmd)?;
    Ok(())
}
```

- [ ] **Step 6: Run the test — expected pass.**

```sh
cargo test -p dkod-cli --test init_cmd
```

Both tests `ok`.

- [ ] **Step 7: Commit.**

```sh
git add crates/dkod-cli/src/cmd/init.rs crates/dkod-cli/src/cmd/mod.rs \
        crates/dkod-cli/src/main.rs crates/dkod-cli/tests/init_cmd.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod init subcommand wrapping init_repo"
```

## Task 4: PR M3-1 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green (the new `cli_parse` and `init_cmd` test files both pass; the rest of the workspace stays green).
- [ ] Push, open PR `M3-1: dkod-cli scaffold + dkod init`. Body = summary + test-plan checklist.
- [ ] Arm the CodeRabbit poller. Iterate `/coderabbit:autofix` until clean.
- [ ] Merge autonomously.

---

# PR M3-2 — `dkod status` + `dkod abort`

## Task 5: `dkod status` implementation

**Files:**
- Create: `crates/dkod-cli/src/cmd/status.rs`
- Modify: `crates/dkod-cli/src/cmd/mod.rs` — add `pub mod status;`
- Modify: `crates/dkod-cli/src/main.rs` — wire status dispatch

- [ ] **Step 1: Write the failing test — empty session prints the all-None response.**

`crates/dkod-cli/tests/status_cmd.rs`:

```rust
#[tokio::test]
async fn status_prints_empty_response_when_no_session() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    dkod_worktree::init_repo(&root, None).unwrap();

    let out = dkod_cli::cmd::status::render(&root).await.expect("status::render");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert!(parsed["active_session_id"].is_null());
    assert!(parsed["dk_branch"].is_null());
    assert_eq!(parsed["groups"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: Run — expected fail.**

- [ ] **Step 3: Implement `status.rs`.**

```rust
use dkod_mcp::ServerCtx;
use dkod_mcp::tools::status::status;
use std::path::Path;
use std::sync::Arc;

/// Render the current session as pretty JSON. Pure helper — `run` calls
/// this and prints to stdout.
pub async fn render(repo_root: &Path) -> anyhow::Result<String> {
    let ctx = Arc::new(ServerCtx::new(repo_root));
    ctx.recover()
        .await
        .map_err(|e| anyhow::anyhow!("ServerCtx::recover failed: {e}"))?;
    let resp = status(&ctx)
        .await
        .map_err(|e| anyhow::anyhow!("status helper failed: {e}"))?;
    let json = serde_json::to_string_pretty(&resp)
        .map_err(|e| anyhow::anyhow!("serialise status response: {e}"))?;
    Ok(json)
}

/// `dkod status` entry — prints the rendered JSON to stdout.
pub async fn run(repo_root: &Path) -> anyhow::Result<()> {
    let json = render(repo_root).await?;
    println!("{json}");
    Ok(())
}
```

- [ ] **Step 4: Register + wire dispatch.**

`cmd/mod.rs`:
```rust
pub mod status;
```

`main.rs`:
```rust
Command::Status => {
    let cwd = std::env::current_dir()?;
    dkod_cli::cmd::status::run(&cwd).await?;
    Ok(())
}
```

- [ ] **Step 5: Run the test — pass.**

- [ ] **Step 6: Commit.**

```sh
git checkout -b m3/cli-status-abort
git add crates/dkod-cli/src/cmd/status.rs crates/dkod-cli/src/cmd/mod.rs \
        crates/dkod-cli/src/main.rs crates/dkod-cli/tests/status_cmd.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod status subcommand"
```

## Task 6: `dkod abort` implementation

**Files:**
- Create: `crates/dkod-cli/src/cmd/abort.rs`
- Modify: `crates/dkod-cli/src/cmd/mod.rs` — add `pub mod abort;`
- Modify: `crates/dkod-cli/src/main.rs` — wire abort dispatch

- [ ] **Step 1: Write the failing test.**

`crates/dkod-cli/tests/abort_cmd.rs`:

```rust
use std::path::PathBuf;

fn init_repo(root: &std::path::Path) {
    let s = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(s.success());
    // Seed at least one commit so `git checkout -b dk/...` works.
    std::fs::write(root.join("README.md"), "seed").unwrap();
    let s = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(s.success());
    let s = std::process::Command::new("git")
        .args(["commit", "-q", "-m", "seed"])
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .status()
        .unwrap();
    assert!(s.success());
    dkod_worktree::init_repo(root, None).unwrap();
}

#[tokio::test]
async fn abort_errors_when_no_session() {
    let tmp = tempfile::tempdir().unwrap();
    let root: PathBuf = tmp.path().to_path_buf();
    init_repo(&root);
    let err = dkod_cli::cmd::abort::render(&root).await.unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("no active session"), "unexpected error: {s}");
}

#[tokio::test]
async fn abort_clears_an_active_session() {
    use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput};
    use dkod_mcp::tools::execute_begin::execute_begin;
    use dkod_mcp::ServerCtx;
    use std::sync::Arc;

    let tmp = tempfile::tempdir().unwrap();
    let root: PathBuf = tmp.path().to_path_buf();
    init_repo(&root);

    // Spin up an executing session via the MCP helper, then drop the ctx
    // so the on-disk state is the only thing left for `dkod abort` to
    // recover from.
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
        .expect("execute_begin");
    }

    let json = dkod_cli::cmd::abort::render(&root).await.expect("abort render");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["session_id"].is_string());

    // dk-branch must be gone.
    let out = std::process::Command::new("git")
        .args(["branch", "--list", "dk/*"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        String::from_utf8(out.stdout).unwrap().trim().is_empty(),
        "dk-branch should have been destroyed"
    );
}
```

- [ ] **Step 2: Run — expected fail.**

- [ ] **Step 3: Implement `abort.rs`.**

```rust
use dkod_mcp::ServerCtx;
use dkod_mcp::tools::abort::abort;
use std::path::Path;
use std::sync::Arc;

/// Run `dkod_abort` against an on-disk session. Returns the JSON
/// response so tests can introspect it without capturing stdout.
pub async fn render(repo_root: &Path) -> anyhow::Result<String> {
    let ctx = Arc::new(ServerCtx::new(repo_root));
    ctx.recover()
        .await
        .map_err(|e| anyhow::anyhow!("ServerCtx::recover failed: {e}"))?;
    let resp = abort(&ctx)
        .await
        .map_err(|e| anyhow::anyhow!("abort helper failed: {e}"))?;
    serde_json::to_string_pretty(&resp)
        .map_err(|e| anyhow::anyhow!("serialise abort response: {e}").into())
}

pub async fn run(repo_root: &Path) -> anyhow::Result<()> {
    let json = render(repo_root).await?;
    println!("{json}");
    Ok(())
}
```

- [ ] **Step 4: Register + wire dispatch.**

`cmd/mod.rs`:
```rust
pub mod abort;
```

`main.rs`:
```rust
Command::Abort => {
    let cwd = std::env::current_dir()?;
    dkod_cli::cmd::abort::run(&cwd).await?;
    Ok(())
}
```

- [ ] **Step 5: Run the test — pass.**

- [ ] **Step 6: Commit.**

```sh
git add crates/dkod-cli/src/cmd/abort.rs crates/dkod-cli/src/cmd/mod.rs \
        crates/dkod-cli/src/main.rs crates/dkod-cli/tests/abort_cmd.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod abort subcommand"
```

## Task 7: PR M3-2 wrap-up

- [ ] `/coderabbit:code-review` clean; `cargo test --workspace` green (the new `status_cmd` and `abort_cmd` test files both pass).
- [ ] PR `M3-2: dkod status + dkod abort`. Merge autonomously.

---

# PR M3-3 — `dkod --mcp` + subprocess E2E

## Task 8: `dkod --mcp` dispatch

**Files:**
- Create: `crates/dkod-cli/src/cmd/mcp.rs`
- Modify: `crates/dkod-cli/src/cmd/mod.rs` — add `pub mod mcp;`
- Modify: `crates/dkod-cli/src/main.rs` — wire `--mcp` dispatch

- [ ] **Step 1: Implement `mcp.rs`.**

This is a near-verbatim copy of `crates/dkod-mcp/src/main.rs`'s `run`, but as a library function.

```rust
use dkod_mcp::{McpServer, ServerCtx};
use rmcp::{ServiceExt, transport::stdio};
use std::path::Path;
use std::sync::Arc;

/// Launch the stdio MCP server in the current working directory. Blocks
/// until Claude Code (or whatever client) closes the stdio pair.
///
/// Mirrors `dkod-mcp`'s standalone binary: `ServerCtx::new` → `recover` →
/// `McpServer::new(ctx).serve(stdio()).await`. The `dkod-mcp` binary
/// stays in place for now (used by tests and existing tooling); this
/// helper is the user-facing entry that Claude Code's plugin will
/// invoke as `dkod --mcp` once the plugin manifest lands in M4.
pub async fn run(repo_root: &Path) -> anyhow::Result<()> {
    let ctx = Arc::new(ServerCtx::new(repo_root));
    ctx.recover()
        .await
        .map_err(|e| anyhow::anyhow!("ServerCtx::recover failed: {e}"))?;
    let service = McpServer::new(ctx)
        .serve(stdio())
        .await
        .map_err(|e| anyhow::anyhow!("MCP serve failed: {e}"))?;
    service
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("MCP wait failed: {e}"))?;
    Ok(())
}
```

- [ ] **Step 2: Register + wire.**

`cmd/mod.rs`:
```rust
pub mod mcp;
```

`main.rs`:
```rust
Command::Mcp => {
    let cwd = std::env::current_dir()?;
    dkod_cli::cmd::mcp::run(&cwd).await
}
```

- [ ] **Step 3: Build the binary.**

```sh
cargo build -p dkod-cli --bin dkod
```

We do NOT spawn the binary in stdio mode from a test (it would block waiting on stdin). The integration coverage for the MCP path comes from `dkod-mcp/tests/e2e_smoke.rs` (M2-8), which already exercises every tool through `McpServer`. `dkod --mcp` is a thin re-host of the same `serve(stdio())` call.

- [ ] **Step 4: Commit.**

```sh
git checkout -b m3/cli-mcp-and-e2e
git add crates/dkod-cli/src/cmd/mcp.rs crates/dkod-cli/src/cmd/mod.rs \
        crates/dkod-cli/src/main.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod --mcp stdio dispatch"
```

## Task 9: Subprocess E2E test

**Files:**
- Create: `crates/dkod-cli/tests/e2e_subprocess.rs`

This test compiles the `dkod` binary and spawns it as a child process, exercising the real argv path. Uses `assert_cmd` (added to dev-deps in Task 1).

- [ ] **Step 1: Write the test.**

```rust
use assert_cmd::Command;
use std::path::PathBuf;

fn init_git_repo(root: &std::path::Path) {
    let s = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(s.success());
}

#[test]
fn dkod_init_then_status_via_subprocess() {
    let tmp = tempfile::tempdir().unwrap();
    let root: PathBuf = tmp.path().to_path_buf();
    init_git_repo(&root);

    // `dkod init`
    Command::cargo_bin("dkod")
        .unwrap()
        .arg("init")
        .arg("--verify-cmd")
        .arg("cargo test")
        .current_dir(&root)
        .assert()
        .success();
    assert!(root.join(".dkod/config.toml").is_file());

    // `dkod status` should print a JSON document with no active session.
    let out = Command::cargo_bin("dkod")
        .unwrap()
        .arg("status")
        .current_dir(&root)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).expect("status JSON");
    assert!(parsed["active_session_id"].is_null());
    assert!(parsed["groups"].as_array().unwrap().is_empty());
}

#[test]
fn dkod_help_lists_subcommands() {
    let out = Command::cargo_bin("dkod")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("init"));
    assert!(s.contains("status"));
    assert!(s.contains("abort"));
    assert!(s.contains("--mcp"));
}

#[test]
fn dkod_rejects_mcp_with_subcommand() {
    Command::cargo_bin("dkod")
        .unwrap()
        .args(["--mcp", "init"])
        .assert()
        .failure();
}
```

- [ ] **Step 2: Run.**

```sh
cargo test -p dkod-cli --test e2e_subprocess
```

All three tests `ok`.

- [ ] **Step 3: Commit.**

```sh
git add crates/dkod-cli/tests/e2e_subprocess.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod CLI subprocess E2E test"
```

## Task 10: README update

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a "Try it" section** that documents `dkod init` / `dkod status` / `dkod abort` / `dkod --mcp` so anyone reading the README can run the CLI without diving into design docs.

Add after the existing "Status" section (the outer fence is four backticks
because the inner shell block uses three):

````markdown
## Try it

```sh
# In any git repo:
cargo run -p dkod-cli --bin dkod -- init
cargo run -p dkod-cli --bin dkod -- status
cargo run -p dkod-cli --bin dkod -- abort   # only if a session is active
cargo run -p dkod-cli --bin dkod -- --mcp   # stdio MCP server (Claude Code expected)
```

`dkod init` writes `.dkod/config.toml`. `dkod status` prints a JSON snapshot of the
current session. `dkod abort` destroys an active dk-branch. `dkod --mcp` is the
stdio entry the Claude Code plugin will use once M4's plugin manifest lands.
````

(Use four backticks around the inner code block since it's nested in a markdown listing.)

Also bump the "Status" section to mention M3:

```markdown
## Status

**v0 in flight — milestones 1, 2, and 3 merged.** `cargo test --workspace` is green across 8 PRs of M1, 8 PRs of M2, and 3 PRs of M3.

The full design lives in [`docs/design.md`](docs/design.md). Milestones 4+ (plugin manifest + skill, real-Rust-sandbox smoke test, marketplace publish) are the remaining ship items.
```

- [ ] **Step 2: Commit (docs-only — skip `/coderabbit:code-review` and note so in the PR body).**

```sh
git add README.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Document dkod CLI usage in README"
```

## Task 11: PR M3-3 wrap-up

- [ ] `cargo test --workspace` green (the new `e2e_subprocess` tests pass; the workspace stays green).
- [ ] PR `M3-3: dkod --mcp + subprocess E2E`. Merge autonomously.
- [ ] Tag `v0.3.0-m3` on `main` (controller's job after merge):

```sh
git checkout main && git pull
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git tag -a v0.3.0-m3 -m "Milestone 3: dkod-cli wrapping M1+M2"
git push origin v0.3.0-m3
```

---

## Milestone 3 exit criteria

1. `cargo test --workspace` green across all 3 PRs merged to `main`.
2. `dkod init`, `dkod status`, `dkod abort`, and `dkod --mcp` all dispatch through `Cli::command_resolved` and return the right value (subprocess test proves init + status; helper-fn tests prove status + abort; e2e_smoke from M2-8 proves the MCP surface).
3. The compiled `dkod` binary works against a fresh tempdir repo end-to-end (init → status round-trip).
4. `--help` lists every subcommand + the `--mcp` flag.
5. `--mcp` rejects being combined with a subcommand.
6. All commits on `main` authored AND committed by `Haim Ari <haimari1@gmail.com>`. No `Co-Authored-By` trailers anywhere in M3 history.

## Out of scope (M4+)

- Plugin manifest (`plugin/plugin.json`), skill authoring, slash commands. M4.
- Removing or renaming the `dkod-mcp` binary. M4 or later.
- Real-Rust-sandbox smoke test measuring wall-clock vs serial. M5.
- `dkod init` accepting a `--cwd <PATH>` flag. M3 uses `current_dir()`; flag-based override is a future ergonomic improvement.
- Coloured / progress-spinner output. JSON-to-stdout is the v0 contract.
