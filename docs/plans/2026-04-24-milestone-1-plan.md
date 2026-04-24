# Milestone 1: `dkod-worktree` + `dkod-orchestrator` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the two library crates that back the rest of dkod-swarm — `dkod-worktree` (session state + dk-branch lifecycle) and `dkod-orchestrator` (symbol extraction, call-graph partitioning, AST symbol replacement, commit finalization). Unit-tested. No MCP, no CLI, no plugin. Milestone stops at a green `cargo test --workspace`.

**Architecture:** Cargo workspace (`edition = "2024"`, fallback `"2021"` if the toolchain lags). Two library crates: `dkod-worktree` has zero AI deps and knows only git + filesystem; `dkod-orchestrator` pulls in `dk-engine 0.3` + `dk-core 0.3` from crates.io for tree-sitter-backed symbol/call extraction and the AST-merge primitive. Partitioner is a pure function over the call graph (union-find connected components). AST symbol replacement is a pure library function — locking is a later-milestone concern. Commit finalization walks `writes.jsonl` and produces one commit per group on the dk-branch with the user's git identity forced via env vars.

**Tech Stack:** Rust 2024, `dk-engine 0.3`, `dk-core 0.3`, `serde` / `serde_json` / `toml`, `thiserror`, `anyhow` (only for tests), `tempfile` (dev-dep). Git subprocess via `std::process::Command` (no `gix`/`git2` dep needed — M1 only uses plumbing commands). Tree-sitter access is **only** via `dk-engine`'s public API; no direct `tree-sitter-rust` dep.

---

## Engine API (reference)

Confirmed during pre-plan probing of `dkod-io/dkod-engine@0.3.x` on crates.io:

- `dk_engine::parser::LanguageParser` — trait. Methods we need: `extract_symbols(&self, source: &[u8], file_path: &Path) -> Result<Vec<Symbol>>` and `extract_calls(&self, source: &[u8], file_path: &Path) -> Result<Vec<RawCallEdge>>`.
- `dk_engine::parser::langs::rust::RustConfig` — `LanguageConfig` implementor for Rust.
- `dk_engine::parser::engine::QueryDrivenParser` — the driver that takes a `LanguageConfig` and implements `LanguageParser`.
- `dk_engine::parser::ParserRegistry` — registry that dispatches by file extension. Convenience wrapper.
- `dk_core::{Symbol, RawCallEdge, CallKind, Visibility, SymbolKind, Span, SymbolId, FileAnalysis}` — re-exported from `dk_core::types`.
- `Symbol.span: Span` carries byte offsets — the single most important field for the AST-replace primitive.

**The exact constructor signature for `QueryDrivenParser::new` on 0.3.x** is the one API detail the plan verifies with a probe task before leaning on it. Every task that references it is gated on that probe.

---

## File Structure

Workspace root `dkod-swarm/`:

```
Cargo.toml                              # workspace
rust-toolchain.toml                     # pin stable + components
.gitignore                              # + /target, .dkod/
crates/
├── dkod-worktree/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                      # re-exports
│   │   ├── error.rs                    # Error, Result
│   │   ├── paths.rs                    # Paths helper
│   │   ├── config.rs                   # Config (TOML)
│   │   ├── session.rs                  # SessionId, Manifest (JSON)
│   │   ├── group.rs                    # GroupId, GroupSpec, WriteRecord (JSONL)
│   │   ├── branch.rs                   # dk-branch + commit helper
│   │   └── init.rs                     # scaffold .dkod/ on a repo
│   └── tests/
│       ├── config_roundtrip.rs
│       ├── session_roundtrip.rs
│       ├── group_writes_jsonl.rs
│       ├── branch_lifecycle.rs         # integration (tempfile git repo)
│       └── init_scaffold.rs
└── dkod-orchestrator/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                      # re-exports
    │   ├── error.rs
    │   ├── symbols.rs                  # extract_rust_file
    │   ├── callgraph.rs                # build resolved call graph
    │   ├── partition.rs                # partition() + Warning
    │   ├── replace.rs                  # replace_symbol() primitive
    │   └── commit.rs                   # commit_per_group()
    └── tests/
        ├── fixtures/                   # small Rust sample repos
        │   ├── basic/                  # 4 disconnected fns
        │   │   └── src/lib.rs
        │   ├── trait_coupling/         # trait + 2 impls + caller
        │   │   └── src/lib.rs
        │   └── big_struct/             # struct + methods, for replace.rs
        │       └── src/lib.rs
        ├── extract_symbols.rs
        ├── callgraph_build.rs
        ├── partition_golden.rs
        │   (+ tests/fixtures/golden/*.json)
        ├── replace_symbol.rs
        └── commit_per_group.rs
```

Each crate has a single clear responsibility. `dkod-worktree` is the lower layer (no AI, no parsing). `dkod-orchestrator` depends on `dkod-worktree` for state I/O + commit path.

---

## PR Plan

Milestone 1 lands in **8 PRs**. Each PR is a feature branch off `main`, opened fresh, and goes through the full CodeRabbit loop per `CLAUDE.md` (local `/coderabbit:code-review` → fix → re-review → commit/push → wait for PR review → `/coderabbit:autofix` → merge). Branch names match the PR title prefix.

| PR | Branch | Scope | Tasks |
|----|--------|-------|-------|
| M1-1 | `m1/workspace-scaffold` | Cargo workspace, toolchain, gitignore | 1–2 |
| M1-2 | `m1/worktree-config-paths` | `dkod-worktree`: error, paths, config | 3–6 |
| M1-3 | `m1/worktree-session-group` | session manifest + group spec + writes.jsonl | 7–9 |
| M1-4 | `m1/worktree-branch-init` | dk-branch lifecycle + init scaffolding | 10–12 |
| M1-5 | `m1/orch-symbols-fixtures` | `dkod-orchestrator` scaffold, API probe, Rust symbol extract, fixtures | 13–17 |
| M1-6 | `m1/orch-partition` | call graph + partitioner + goldens + warnings | 18–22 |
| M1-7 | `m1/orch-replace` | AST symbol replace primitive + fallback | 23–25 |
| M1-8 | `m1/orch-commit-e2e` | commit-per-group + milestone E2E test | 26–28 |

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
  No `Co-Authored-By`. Verify with `git log -1 --format='%an <%ae> | %cn <%ce>'` after each commit.
- Be preceded by `/coderabbit:code-review` on the local diff vs `main`, unless the changeset is docs/config-only.

Every PR MUST:

- Title ≤ 70 chars.
- Body = short summary + test-plan checklist.
- Open ONE PR at a time. Do not start the next PR's branch until the current one is merged.
- **Stop and ask before merging.**

---

# PR M1-1 — Workspace scaffold

### Task 1: Initialise the Cargo workspace

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`

- [ ] **Step 1: Create the workspace manifest.**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [
    "crates/dkod-worktree",
    "crates/dkod-orchestrator",
]

[workspace.package]
version = "0.0.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/dkod-io/dkod-swarm"
authors = ["Haim Ari <haimari1@gmail.com>"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
thiserror = "2"
anyhow = "1"
tempfile = "3"
dk-core = "0.3"
dk-engine = "0.3"

[profile.dev]
# keep default

[profile.release]
lto = "thin"
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 2: Verify the workspace resolves.**

Run: `cargo metadata --no-deps --format-version=1 >/dev/null`
Expected: exits 0 (the crate members don't exist yet, so *also* expect a "could not find ..." error — in which case defer this verification until after Task 3 makes the first member exist). If metadata errors, skip and continue; otherwise move on.

- [ ] **Step 3: Commit.**

Docs-only-adjacent — this is a `.toml` changeset. Skip CodeRabbit; state that explicitly in the PR body.

```sh
git checkout -b m1/workspace-scaffold
git add Cargo.toml rust-toolchain.toml
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add Cargo workspace + rust-toolchain"
```

### Task 2: Extend `.gitignore`

**Files:**
- Modify: `.gitignore` (create if absent)

- [ ] **Step 1: Create/extend `.gitignore`.**

```
/target
/Cargo.lock
.dkod/
```

Note: `Cargo.lock` is ignored because M1 ships libraries only; no binary crate yet. Revisit when `dkod-cli` lands (M3).

- [ ] **Step 2: Commit and open PR M1-1.**

```sh
git add .gitignore
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Ignore target, Cargo.lock, and .dkod/"
git push -u origin m1/workspace-scaffold
gh pr create --title "M1-1: Cargo workspace scaffold" --body "$(cat <<'EOF'
## Summary
- Workspace manifest + pinned toolchain + .gitignore.
- Config-only change — not reviewed by CodeRabbit (markdown/config aren't meaningfully reviewed).

## Test plan
- [ ] `cargo metadata` runs clean after the first crate exists (verified in PR M1-2).
EOF
)"
```

- [ ] **Step 3: STOP — ask before merging.**

---

# PR M1-2 — `dkod-worktree`: error, paths, config

### Task 3: `dkod-worktree` crate skeleton + error type

**Files:**
- Create: `crates/dkod-worktree/Cargo.toml`
- Create: `crates/dkod-worktree/src/lib.rs`
- Create: `crates/dkod-worktree/src/error.rs`

- [ ] **Step 1: Cargo manifest.**

`crates/dkod-worktree/Cargo.toml`:
```toml
[package]
name = "dkod-worktree"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "Session state and dk-branch lifecycle for dkod-swarm"

[dependencies]
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
anyhow.workspace = true
```

- [ ] **Step 2: Error type.**

`src/error.rs`:
```rust
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },

    #[error("toml decode error in {path}: {source}")]
    TomlDecode { path: PathBuf, #[source] source: toml::de::Error },

    #[error("toml encode error: {0}")]
    TomlEncode(#[from] toml::ser::Error),

    #[error("json error in {path}: {source}")]
    Json { path: PathBuf, #[source] source: serde_json::Error },

    #[error("git command failed: {cmd}: {stderr}")]
    Git { cmd: String, stderr: String },

    #[error("invalid state: {0}")]
    Invalid(String),

    #[error("not initialised: .dkod/ missing at {0}")]
    NotInitialised(PathBuf),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
```

- [ ] **Step 3: `lib.rs` re-exports (stubs for now).**

`src/lib.rs`:
```rust
pub mod error;
pub use error::{Error, Result};
```

- [ ] **Step 4: Build.**

Run: `cargo build -p dkod-worktree`
Expected: succeeds.

- [ ] **Step 5: Commit.**

(Code changeset — run `/coderabbit:code-review` first; fix any findings; re-run until clean.)

```sh
git checkout -b m1/worktree-config-paths origin/main
git add crates/dkod-worktree/
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Scaffold dkod-worktree crate with error type"
```

### Task 4: `Paths` helper — TDD

**Files:**
- Create: `crates/dkod-worktree/src/paths.rs`
- Modify: `crates/dkod-worktree/src/lib.rs`
- Create: `crates/dkod-worktree/tests/paths.rs`

- [ ] **Step 1: Write the failing test.**

`crates/dkod-worktree/tests/paths.rs`:
```rust
use dkod_worktree::Paths;
use std::path::PathBuf;

#[test]
fn paths_resolve_under_dkod_dir() {
    let repo = PathBuf::from("/tmp/fake-repo");
    let p = Paths::new(&repo);
    assert_eq!(p.root(), repo.join(".dkod"));
    assert_eq!(p.config(), repo.join(".dkod/config.toml"));
    assert_eq!(p.sessions_dir(), repo.join(".dkod/sessions"));
    assert_eq!(p.session("abc"), repo.join(".dkod/sessions/abc"));
    assert_eq!(p.manifest("abc"), repo.join(".dkod/sessions/abc/manifest.json"));
    assert_eq!(p.groups_dir("abc"), repo.join(".dkod/sessions/abc/groups"));
    assert_eq!(p.group("abc", "g1"), repo.join(".dkod/sessions/abc/groups/g1"));
    assert_eq!(p.group_spec("abc", "g1"), repo.join(".dkod/sessions/abc/groups/g1/spec.json"));
    assert_eq!(p.group_writes("abc", "g1"), repo.join(".dkod/sessions/abc/groups/g1/writes.jsonl"));
    assert_eq!(p.conflicts_dir("abc"), repo.join(".dkod/sessions/abc/conflicts"));
}
```

- [ ] **Step 2: Run the test — expect failure.**

Run: `cargo test -p dkod-worktree --test paths`
Expected: FAIL — `no module named 'paths'`.

- [ ] **Step 3: Implement `Paths`.**

`src/paths.rs`:
```rust
use std::path::{Path, PathBuf};

/// Filesystem layout helper for `.dkod/`.
/// All paths are derived from the repo root passed to `::new`.
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(repo_root: &Path) -> Self {
        Self { root: repo_root.join(".dkod") }
    }

    pub fn root(&self) -> PathBuf { self.root.clone() }
    pub fn config(&self) -> PathBuf { self.root.join("config.toml") }
    pub fn sessions_dir(&self) -> PathBuf { self.root.join("sessions") }
    pub fn session(&self, sid: &str) -> PathBuf { self.sessions_dir().join(sid) }
    pub fn manifest(&self, sid: &str) -> PathBuf { self.session(sid).join("manifest.json") }
    pub fn groups_dir(&self, sid: &str) -> PathBuf { self.session(sid).join("groups") }
    pub fn group(&self, sid: &str, gid: &str) -> PathBuf { self.groups_dir(sid).join(gid) }
    pub fn group_spec(&self, sid: &str, gid: &str) -> PathBuf { self.group(sid, gid).join("spec.json") }
    pub fn group_writes(&self, sid: &str, gid: &str) -> PathBuf { self.group(sid, gid).join("writes.jsonl") }
    pub fn conflicts_dir(&self, sid: &str) -> PathBuf { self.session(sid).join("conflicts") }
}
```

Re-export in `src/lib.rs`:
```rust
pub mod error;
pub mod paths;
pub use error::{Error, Result};
pub use paths::Paths;
```

- [ ] **Step 4: Run tests — expect pass.**

Run: `cargo test -p dkod-worktree --test paths`
Expected: 1 passed.

- [ ] **Step 5: Commit.**

(`/coderabbit:code-review` first; fix; re-review; then commit.)

```sh
git add crates/dkod-worktree/src/paths.rs crates/dkod-worktree/src/lib.rs crates/dkod-worktree/tests/paths.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add Paths helper for .dkod layout"
```

### Task 5: `Config` — TDD TOML roundtrip

**Files:**
- Create: `crates/dkod-worktree/src/config.rs`
- Modify: `crates/dkod-worktree/src/lib.rs`
- Create: `crates/dkod-worktree/tests/config_roundtrip.rs`

- [ ] **Step 1: Write the failing test.**

`crates/dkod-worktree/tests/config_roundtrip.rs`:
```rust
use dkod_worktree::{Config, Paths};
use tempfile::TempDir;

#[test]
fn config_roundtrips_through_disk() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    std::fs::create_dir_all(paths.root()).unwrap();

    let cfg = Config {
        main_branch: "main".into(),
        verify_cmd: Some("cargo check && cargo test --workspace".into()),
    };
    cfg.save(&paths.config()).unwrap();

    let loaded = Config::load(&paths.config()).unwrap();
    assert_eq!(loaded.main_branch, "main");
    assert_eq!(loaded.verify_cmd.as_deref(), Some("cargo check && cargo test --workspace"));
}

#[test]
fn config_defaults_when_verify_absent() {
    let tmp = TempDir::new().unwrap();
    let cfg_path = tmp.path().join("config.toml");
    std::fs::write(&cfg_path, "main_branch = \"trunk\"\n").unwrap();

    let loaded = Config::load(&cfg_path).unwrap();
    assert_eq!(loaded.main_branch, "trunk");
    assert!(loaded.verify_cmd.is_none());
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-worktree --test config_roundtrip`
Expected: FAIL — `no module named 'config'`.

- [ ] **Step 3: Implement `Config`.**

`src/config.rs`:
```rust
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The upstream branch to base dk-branches on.
    pub main_branch: String,
    /// Optional shell command to run once before PR creation (M3+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_cmd: Option<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
        toml::from_str(&text)
            .map_err(|e| Error::TomlDecode { path: path.to_path_buf(), source: e })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        std::fs::write(path, text)
            .map_err(|e| Error::Io { path: path.to_path_buf(), source: e })
    }
}
```

Re-export:
```rust
// src/lib.rs
pub mod config;
pub use config::Config;
```

- [ ] **Step 4: Run tests — expect pass.**

Run: `cargo test -p dkod-worktree`
Expected: 3 passed (the two config tests + paths test).

- [ ] **Step 5: Commit.**

(`/coderabbit:code-review` → fix → re-review → commit.)

```sh
git add crates/dkod-worktree/src/config.rs crates/dkod-worktree/src/lib.rs crates/dkod-worktree/tests/config_roundtrip.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add Config with TOML load/save"
```

### Task 6: Open PR M1-2

- [ ] **Step 1: Push + open PR.**

```sh
git push -u origin m1/worktree-config-paths
gh pr create --title "M1-2: dkod-worktree error + paths + config" --body "$(cat <<'EOF'
## Summary
- dkod-worktree crate scaffold with `Error`/`Result`, `Paths` helper, and `Config` (TOML).
- TDD throughout — `paths`, `config_roundtrip` tests cover layout resolution and TOML roundtripping with defaults.

## Test plan
- [x] `cargo test -p dkod-worktree` green
- [ ] CodeRabbit PR-side review clean (via `/coderabbit:autofix`)
EOF
)"
```

- [ ] **Step 2: Wait for CodeRabbit PR review, run `/coderabbit:autofix`, iterate until clean.**

- [ ] **Step 3: STOP — ask before merging.**

---

# PR M1-3 — `dkod-worktree`: session + group state

### Task 7: `SessionId` + `Manifest` — TDD

**Files:**
- Create: `crates/dkod-worktree/src/session.rs`
- Modify: `crates/dkod-worktree/src/lib.rs`
- Create: `crates/dkod-worktree/tests/session_roundtrip.rs`

- [ ] **Step 1: Write the failing test.**

`crates/dkod-worktree/tests/session_roundtrip.rs`:
```rust
use dkod_worktree::{Manifest, Paths, SessionId, SessionStatus};
use tempfile::TempDir;

#[test]
fn session_id_is_stable_short_string() {
    let a = SessionId::generate();
    let b = SessionId::generate();
    assert_ne!(a.as_str(), b.as_str());
    assert!(a.as_str().len() >= 8, "session id too short");
    assert!(
        a.as_str().chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "session id must be filesystem-safe: got {:?}", a.as_str()
    );
}

#[test]
fn manifest_roundtrips_through_disk() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from("sess-abc");

    let m = Manifest {
        session_id: sid.clone(),
        task_prompt: "refactor auth to passkeys".into(),
        created_at: "2026-04-24T12:00:00Z".into(),
        status: SessionStatus::Planned,
        group_ids: vec!["g1".into(), "g2".into()],
    };
    m.save(&paths).unwrap();

    let loaded = Manifest::load(&paths, &sid).unwrap();
    assert_eq!(loaded.task_prompt, "refactor auth to passkeys");
    assert_eq!(loaded.status, SessionStatus::Planned);
    assert_eq!(loaded.group_ids, vec!["g1", "g2"]);
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-worktree --test session_roundtrip`
Expected: FAIL — unresolved imports.

- [ ] **Step 3: Implement.**

`src/session.rs`:
```rust
use crate::{Error, Paths, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a new session id: `sess-<12 hex chars>`.
    ///
    /// Uses the process clock + a random nibble for uniqueness. No crypto
    /// guarantees; session ids are not secrets.
    pub fn generate() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        // 12 hex chars = 48 bits — collision-safe for single-user sessions.
        let s = format!("sess-{:012x}", nanos & 0xffff_ffff_ffff);
        Self(s)
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Planned,
    Executing,
    Committed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub session_id: SessionId,
    pub task_prompt: String,
    pub created_at: String,          // ISO-8601, opaque to this crate
    pub status: SessionStatus,
    pub group_ids: Vec<String>,
}

impl Manifest {
    pub fn save(&self, paths: &Paths) -> Result<()> {
        let path = paths.manifest(self.session_id.as_str());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| Error::Json { path: path.clone(), source: e })?;
        std::fs::write(&path, json)
            .map_err(|e| Error::Io { path, source: e })
    }

    pub fn load(paths: &Paths, sid: &SessionId) -> Result<Self> {
        let path = paths.manifest(sid.as_str());
        let bytes = std::fs::read(&path)
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Json { path, source: e })
    }
}
```

Re-export:
```rust
// src/lib.rs
pub mod session;
pub use session::{Manifest, SessionId, SessionStatus};
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-worktree`
Expected: all tests green.

- [ ] **Step 5: Commit.**

(CodeRabbit pre-commit.)
```sh
git checkout -b m1/worktree-session-group origin/main
git add crates/dkod-worktree/src/session.rs crates/dkod-worktree/src/lib.rs crates/dkod-worktree/tests/session_roundtrip.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add SessionId, Manifest, and session roundtrip"
```

### Task 8: `GroupSpec` + `WriteRecord` append log — TDD

**Files:**
- Create: `crates/dkod-worktree/src/group.rs`
- Modify: `crates/dkod-worktree/src/lib.rs`
- Create: `crates/dkod-worktree/tests/group_writes_jsonl.rs`

- [ ] **Step 1: Write the failing test.**

`crates/dkod-worktree/tests/group_writes_jsonl.rs`:
```rust
use dkod_worktree::{GroupSpec, GroupStatus, Paths, SessionId, SymbolRef, WriteLog, WriteRecord};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn group_spec_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from("sess-abc");
    let gid = "g1";

    let spec = GroupSpec {
        id: gid.into(),
        symbols: vec![
            SymbolRef {
                qualified_name: "auth::login".into(),
                file_path: PathBuf::from("src/auth.rs"),
                kind: "function".into(),
            },
        ],
        agent_prompt: "rewrite these as passkeys".into(),
        status: GroupStatus::Pending,
    };
    spec.save(&paths, &sid).unwrap();

    let loaded = GroupSpec::load(&paths, &sid, gid).unwrap();
    assert_eq!(loaded.symbols.len(), 1);
    assert_eq!(loaded.symbols[0].qualified_name, "auth::login");
    assert_eq!(loaded.status, GroupStatus::Pending);
}

#[test]
fn write_log_appends_as_jsonl_and_reads_back_in_order() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::new(tmp.path());
    let sid = SessionId::from("sess-abc");
    let gid = "g1";

    let log = WriteLog::open(&paths, &sid, gid).unwrap();
    log.append(&WriteRecord {
        symbol: "auth::login".into(),
        file_path: PathBuf::from("src/auth.rs"),
        timestamp: "2026-04-24T12:00:00Z".into(),
    }).unwrap();
    log.append(&WriteRecord {
        symbol: "auth::logout".into(),
        file_path: PathBuf::from("src/auth.rs"),
        timestamp: "2026-04-24T12:00:01Z".into(),
    }).unwrap();

    let rows = WriteLog::read_all(&paths, &sid, gid).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].symbol, "auth::login");
    assert_eq!(rows[1].symbol, "auth::logout");
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-worktree --test group_writes_jsonl`
Expected: FAIL — unresolved imports.

- [ ] **Step 3: Implement.**

`src/group.rs`:
```rust
use crate::{Error, Paths, Result, SessionId};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    pub qualified_name: String,
    pub file_path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupStatus {
    Pending,
    InProgress,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSpec {
    pub id: String,
    pub symbols: Vec<SymbolRef>,
    pub agent_prompt: String,
    pub status: GroupStatus,
}

impl GroupSpec {
    pub fn save(&self, paths: &Paths, sid: &SessionId) -> Result<()> {
        let path = paths.group_spec(sid.as_str(), &self.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| Error::Json { path: path.clone(), source: e })?;
        std::fs::write(&path, json)
            .map_err(|e| Error::Io { path, source: e })
    }

    pub fn load(paths: &Paths, sid: &SessionId, gid: &str) -> Result<Self> {
        let path = paths.group_spec(sid.as_str(), gid);
        let bytes = std::fs::read(&path)
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Json { path, source: e })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteRecord {
    pub symbol: String,
    pub file_path: PathBuf,
    pub timestamp: String,
}

/// Append-only JSONL log of agent symbol writes.
pub struct WriteLog {
    path: PathBuf,
}

impl WriteLog {
    pub fn open(paths: &Paths, sid: &SessionId, gid: &str) -> Result<Self> {
        let path = paths.group_writes(sid.as_str(), gid);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        Ok(Self { path })
    }

    pub fn append(&self, rec: &WriteRecord) -> Result<()> {
        let line = serde_json::to_string(rec)
            .map_err(|e| Error::Json { path: self.path.clone(), source: e })?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| Error::Io { path: self.path.clone(), source: e })?;
        writeln!(f, "{line}")
            .map_err(|e| Error::Io { path: self.path.clone(), source: e })
    }

    pub fn read_all(paths: &Paths, sid: &SessionId, gid: &str) -> Result<Vec<WriteRecord>> {
        let path = paths.group_writes(sid.as_str(), gid);
        let f = std::fs::File::open(&path)
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        let mut out = Vec::new();
        for line in BufReader::new(f).lines() {
            let line = line.map_err(|e| Error::Io { path: path.clone(), source: e })?;
            if line.trim().is_empty() { continue; }
            let rec: WriteRecord = serde_json::from_str(&line)
                .map_err(|e| Error::Json { path: path.clone(), source: e })?;
            out.push(rec);
        }
        Ok(out)
    }
}
```

Re-export:
```rust
// src/lib.rs
pub mod group;
pub use group::{GroupSpec, GroupStatus, SymbolRef, WriteLog, WriteRecord};
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-worktree`
Expected: all green.

- [ ] **Step 5: Commit.**

(CodeRabbit pre-commit.)
```sh
git add crates/dkod-worktree/src/group.rs crates/dkod-worktree/src/lib.rs crates/dkod-worktree/tests/group_writes_jsonl.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add GroupSpec and append-only WriteLog"
```

### Task 9: Open PR M1-3

- [ ] **Step 1: Push + open PR.**

```sh
git push -u origin m1/worktree-session-group
gh pr create --title "M1-3: dkod-worktree session + group state" --body "$(cat <<'EOF'
## Summary
- `SessionId`/`Manifest` — JSON-backed session descriptor.
- `GroupSpec` — JSON per group under sessions/<id>/groups/<gid>/spec.json.
- `WriteLog` — append-only JSONL recording agent symbol writes.

## Test plan
- [x] `cargo test -p dkod-worktree` green (4 suites)
- [ ] CodeRabbit PR-side review clean via `/coderabbit:autofix`
EOF
)"
```

- [ ] **Step 2: Wait + `/coderabbit:autofix` loop. STOP — ask before merging.**

---

# PR M1-4 — `dkod-worktree`: dk-branch lifecycle + init

### Task 10: `branch` module — dk-branch lifecycle (TDD, integration)

**Files:**
- Create: `crates/dkod-worktree/src/branch.rs`
- Modify: `crates/dkod-worktree/src/lib.rs`
- Create: `crates/dkod-worktree/tests/branch_lifecycle.rs`

**Notes on API surface:** the `branch` module speaks to git by shelling out. No `gix`/`git2` dep in M1. Every `git` invocation wraps stderr into `Error::Git`.

- [ ] **Step 1: Write the failing test.**

`crates/dkod-worktree/tests/branch_lifecycle.rs`:
```rust
use dkod_worktree::branch;
use std::process::Command;
use tempfile::TempDir;

fn init_repo(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let st = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Haim Ari")
            .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
            .env("GIT_COMMITTER_NAME", "Haim Ari")
            .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    };
    run(&["init", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "hi").unwrap();
    run(&["add", "README.md"]);
    run(&["commit", "-m", "initial"]);
}

#[test]
fn detect_main_returns_main() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    let main = branch::detect_main(tmp.path()).unwrap();
    assert_eq!(main, "main");
}

#[test]
fn create_dkbranch_off_main_then_destroy() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    branch::create_dk_branch(tmp.path(), "main", "sess-abc").unwrap();

    let cur = Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(tmp.path()).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&cur.stdout).trim(), "dk/sess-abc");

    branch::destroy_dk_branch(tmp.path(), "main", "sess-abc").unwrap();

    let cur = Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(tmp.path()).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&cur.stdout).trim(), "main");

    let branches = Command::new("git").args(["branch", "--list", "dk/sess-abc"])
        .current_dir(tmp.path()).output().unwrap();
    assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
}

#[test]
fn commit_on_dk_branch_uses_enforced_identity() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    branch::create_dk_branch(tmp.path(), "main", "sess-abc").unwrap();

    std::fs::write(tmp.path().join("a.txt"), "hello").unwrap();

    branch::commit_paths(
        tmp.path(),
        &[std::path::Path::new("a.txt")],
        "group g1: initial land",
    ).unwrap();

    let out = Command::new("git")
        .args(["log", "-1", "--format=%an <%ae> | %cn <%ce> | %s"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(line, "Haim Ari <haimari1@gmail.com> | Haim Ari <haimari1@gmail.com> | group g1: initial land");
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-worktree --test branch_lifecycle`
Expected: FAIL — module `branch` missing.

- [ ] **Step 3: Implement.**

`src/branch.rs`:
```rust
use crate::{Error, Result};
use std::path::Path;
use std::process::Command;

const AUTHOR_NAME: &str = "Haim Ari";
const AUTHOR_EMAIL: &str = "haimari1@gmail.com";

pub fn dk_branch_name(session_id: &str) -> String {
    format!("dk/{session_id}")
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| Error::Git { cmd: format!("git {}", args.join(" ")), stderr: e.to_string() })?;
    if !out.status.success() {
        return Err(Error::Git {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&out.stderr).into(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_with_identity(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", AUTHOR_NAME)
        .env("GIT_AUTHOR_EMAIL", AUTHOR_EMAIL)
        .env("GIT_COMMITTER_NAME", AUTHOR_NAME)
        .env("GIT_COMMITTER_EMAIL", AUTHOR_EMAIL)
        .output()
        .map_err(|e| Error::Git { cmd: format!("git {}", args.join(" ")), stderr: e.to_string() })?;
    if !out.status.success() {
        return Err(Error::Git {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&out.stderr).into(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Detect the repo's default branch by inspecting HEAD, then falling back to
/// `origin/HEAD`, then to the literal "main".
pub fn detect_main(repo: &Path) -> Result<String> {
    // If HEAD is on a named branch, use it.
    if let Ok(head) = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        if head != "HEAD" { return Ok(head); }
    }
    // Fall back to origin/HEAD.
    if let Ok(sym) = git(repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
        if let Some(stripped) = sym.strip_prefix("origin/") {
            return Ok(stripped.to_string());
        }
    }
    Ok("main".to_string())
}

pub fn create_dk_branch(repo: &Path, main: &str, session_id: &str) -> Result<()> {
    let name = dk_branch_name(session_id);
    git(repo, &["checkout", "-b", &name, main])?;
    Ok(())
}

pub fn destroy_dk_branch(repo: &Path, main: &str, session_id: &str) -> Result<()> {
    let name = dk_branch_name(session_id);
    // Move off the branch before deleting it.
    git(repo, &["checkout", main])?;
    // `-D` is the right tool here: dk-branches are ephemeral and may not be
    // merged. Unmerged is the normal abort case.
    git(repo, &["branch", "-D", &name])?;
    Ok(())
}

/// Stage the given paths and produce a single commit on the current branch
/// using the enforced author/committer identity.
pub fn commit_paths(repo: &Path, paths: &[&Path], msg: &str) -> Result<()> {
    let mut add = vec!["add", "--"];
    let strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    for s in &strs { add.push(s); }
    git_with_identity(repo, &add)?;
    git_with_identity(repo, &["commit", "-m", msg])?;
    Ok(())
}
```

Re-export (only the module; callers import via `dkod_worktree::branch`):
```rust
// src/lib.rs
pub mod branch;
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-worktree --test branch_lifecycle`
Expected: 3 passed.

- [ ] **Step 5: Commit.**

(CodeRabbit pre-commit.)
```sh
git checkout -b m1/worktree-branch-init origin/main
git add crates/dkod-worktree/src/branch.rs crates/dkod-worktree/src/lib.rs crates/dkod-worktree/tests/branch_lifecycle.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dk-branch lifecycle and identity-forcing commit helper"
```

### Task 11: `init` module — scaffold `.dkod/`

**Files:**
- Create: `crates/dkod-worktree/src/init.rs`
- Modify: `crates/dkod-worktree/src/lib.rs`
- Create: `crates/dkod-worktree/tests/init_scaffold.rs`

- [ ] **Step 1: Write the failing test.**

`crates/dkod-worktree/tests/init_scaffold.rs`:
```rust
use dkod_worktree::{init_repo, Config, Paths};
use std::process::Command;
use tempfile::TempDir;

fn init_git(dir: &std::path::Path) {
    Command::new("git").args(["init", "-b", "main"]).current_dir(dir).status().unwrap();
    std::fs::write(dir.join("README.md"), "hi").unwrap();
    Command::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .env("GIT_AUTHOR_NAME", "Haim Ari")
        .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
        .env("GIT_COMMITTER_NAME", "Haim Ari")
        .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
        .current_dir(dir)
        .status()
        .unwrap();
}

#[test]
fn init_scaffolds_dkod_dir_and_detects_main() {
    let tmp = TempDir::new().unwrap();
    init_git(tmp.path());

    init_repo(tmp.path(), None).unwrap();

    let paths = Paths::new(tmp.path());
    assert!(paths.root().is_dir(), ".dkod/ not created");
    assert!(paths.sessions_dir().is_dir(), ".dkod/sessions/ not created");
    assert!(paths.config().is_file(), ".dkod/config.toml not created");

    let cfg = Config::load(&paths.config()).unwrap();
    assert_eq!(cfg.main_branch, "main");
    assert!(cfg.verify_cmd.is_none());
}

#[test]
fn init_is_idempotent_and_preserves_user_verify_cmd() {
    let tmp = TempDir::new().unwrap();
    init_git(tmp.path());

    init_repo(tmp.path(), Some("cargo check".into())).unwrap();
    // Second run must not overwrite existing config.
    init_repo(tmp.path(), Some("different".into())).unwrap();

    let paths = Paths::new(tmp.path());
    let cfg = Config::load(&paths.config()).unwrap();
    assert_eq!(cfg.verify_cmd.as_deref(), Some("cargo check"));
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-worktree --test init_scaffold`
Expected: FAIL — `init_repo` missing.

- [ ] **Step 3: Implement.**

`src/init.rs`:
```rust
use crate::{branch, Config, Error, Paths, Result};
use std::path::Path;

/// Scaffold `.dkod/` for a repo. Idempotent: if `config.toml` already exists,
/// it is left untouched.
pub fn init_repo(repo_root: &Path, verify_cmd: Option<String>) -> Result<()> {
    if !repo_root.exists() {
        return Err(Error::Invalid(format!(
            "repo root does not exist: {}", repo_root.display()
        )));
    }
    let paths = Paths::new(repo_root);
    std::fs::create_dir_all(paths.sessions_dir())
        .map_err(|e| Error::Io { path: paths.sessions_dir(), source: e })?;

    if paths.config().exists() {
        return Ok(());
    }
    let main_branch = branch::detect_main(repo_root)?;
    let cfg = Config { main_branch, verify_cmd };
    cfg.save(&paths.config())?;
    Ok(())
}
```

Re-export:
```rust
// src/lib.rs
pub mod init;
pub use init::init_repo;
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-worktree`
Expected: all suites pass.

- [ ] **Step 5: Commit.**

```sh
git add crates/dkod-worktree/src/init.rs crates/dkod-worktree/src/lib.rs crates/dkod-worktree/tests/init_scaffold.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add init_repo to scaffold .dkod/"
```

### Task 12: Open PR M1-4

- [ ] **Step 1: Push + PR.**

```sh
git push -u origin m1/worktree-branch-init
gh pr create --title "M1-4: dkod-worktree dk-branch lifecycle + init" --body "$(cat <<'EOF'
## Summary
- `branch`: detect main, create/destroy `dk/<session-id>`, identity-forced commit helper.
- `init_repo`: idempotent `.dkod/` scaffolding with detected main branch.

## Test plan
- [x] `cargo test -p dkod-worktree` green (all suites, including tempfile-backed integration tests).
- [ ] CodeRabbit PR-side review clean via `/coderabbit:autofix`.
EOF
)"
```

- [ ] **Step 2: Wait + `/coderabbit:autofix` loop. STOP — ask before merging.**

---

# PR M1-5 — `dkod-orchestrator`: scaffold + symbol extraction

### Task 13: `dkod-orchestrator` crate skeleton + error

**Files:**
- Create: `crates/dkod-orchestrator/Cargo.toml`
- Create: `crates/dkod-orchestrator/src/lib.rs`
- Create: `crates/dkod-orchestrator/src/error.rs`

- [ ] **Step 1: Cargo manifest.**

`crates/dkod-orchestrator/Cargo.toml`:
```toml
[package]
name = "dkod-orchestrator"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "Planner, AST symbol replacement, and commit finalization for dkod-swarm"

[dependencies]
dkod-worktree = { path = "../dkod-worktree" }
dk-core.workspace = true
dk-engine.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
anyhow.workspace = true
```

- [ ] **Step 2: Error type.**

`src/error.rs`:
```rust
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("worktree error: {0}")]
    Worktree(#[from] dkod_worktree::Error),

    #[error("engine parser error: {0}")]
    Engine(String),

    #[error("io at {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },

    #[error("symbol {name} not found in {file}")]
    SymbolNotFound { name: String, file: PathBuf },

    #[error("partition input invalid: {0}")]
    InvalidPartition(String),

    #[error("replace failed: {0}")]
    ReplaceFailed(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
```

`src/lib.rs`:
```rust
pub mod error;
pub use error::{Error, Result};
```

- [ ] **Step 3: Build.**

Run: `cargo build -p dkod-orchestrator`
Expected: succeeds (may take a while — `dk-engine` is heavy).

- [ ] **Step 4: Commit.**

```sh
git checkout -b m1/orch-symbols-fixtures origin/main
git add crates/dkod-orchestrator/
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Scaffold dkod-orchestrator crate with dk-engine dep"
```

### Task 14: **API probe** — verify `dk_engine::parser` constructor shape

Before any test leans on `QueryDrivenParser::new`, confirm the 0.3.x signature empirically. This task writes a throwaway example, runs it, and records the confirmed API shape as a comment in `symbols.rs`.

**Files:**
- Create: `crates/dkod-orchestrator/examples/probe_engine_api.rs`

- [ ] **Step 1: Write the probe.**

`examples/probe_engine_api.rs`:
```rust
//! Probe-only binary. Confirms the public `dk_engine` parser API shape on
//! the version pinned in `Cargo.toml`. Run with:
//!   cargo run --example probe_engine_api -p dkod-orchestrator
//!
//! Not a test — pure smoke. Delete or keep as a diagnostics tool after M1.

use dk_engine::parser::{LanguageParser, langs::rust::RustConfig, engine::QueryDrivenParser};
use std::path::Path;

fn main() {
    let src = b"pub fn hello() -> &'static str { \"hi\" }\n";
    // EXPECTED API on 0.3.x: QueryDrivenParser::new takes a LanguageConfig.
    // If this line fails to compile, update the comment + all callers with the
    // observed signature before proceeding.
    let parser = QueryDrivenParser::new(RustConfig);
    let syms = parser
        .extract_symbols(src, Path::new("probe.rs"))
        .expect("extract_symbols");
    println!("found {} symbols", syms.len());
    for s in &syms {
        println!("  {} ({:?}) span={:?}", s.qualified_name, s.kind, s.span);
    }
}
```

- [ ] **Step 2: Run.**

Run: `cargo run --example probe_engine_api -p dkod-orchestrator 2>&1 | tail -20`
Expected output includes: `found 1 symbols` and a line naming `hello`.

- [ ] **Step 3: Record the confirmed shape.**

If the probe compiles and runs as-is, add a note atop `src/lib.rs`:
```rust
//! `dk-engine 0.3.x` exposes: `QueryDrivenParser::new(LanguageConfig)`,
//! `LanguageParser::{extract_symbols, extract_calls}` returning
//! `Result<Vec<dk_core::{Symbol, RawCallEdge}>>`. Confirmed by
//! `examples/probe_engine_api.rs`.
```

If the probe fails to compile, **STOP**: the plan is drifting from the engine's actual API. Capture the observed signature, then update this task and downstream tasks (15, 18, 23) to match before continuing. Do not hand-wave past this.

- [ ] **Step 4: Commit.**

```sh
git add crates/dkod-orchestrator/examples/probe_engine_api.rs crates/dkod-orchestrator/src/lib.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add engine API probe and record confirmed 0.3.x shape"
```

### Task 15: Rust symbol extraction wrapper — TDD

**Files:**
- Create: `crates/dkod-orchestrator/src/symbols.rs`
- Modify: `crates/dkod-orchestrator/src/lib.rs`
- Create: `crates/dkod-orchestrator/tests/extract_symbols.rs`

- [ ] **Step 1: Write the failing test.**

`tests/extract_symbols.rs`:
```rust
use dkod_orchestrator::symbols::extract_rust_file;

#[test]
fn extracts_function_symbols_from_inline_source() {
    let src = b"pub fn login() {}\npub fn logout() {}\n";
    let (syms, _calls) = extract_rust_file(src, std::path::Path::new("auth.rs")).unwrap();

    let names: Vec<_> = syms.iter().map(|s| s.qualified_name.clone()).collect();
    assert!(names.iter().any(|n| n.contains("login")), "missing login: {names:?}");
    assert!(names.iter().any(|n| n.contains("logout")), "missing logout: {names:?}");
}

#[test]
fn extracts_calls_between_functions() {
    let src = br#"
pub fn login() { validate(); }
pub fn validate() -> bool { true }
"#;
    let (_syms, calls) = extract_rust_file(src, std::path::Path::new("auth.rs")).unwrap();

    let found = calls.iter().any(|c| c.caller_name.contains("login") && c.callee_name.contains("validate"));
    assert!(found, "expected login -> validate edge; got {calls:?}");
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-orchestrator --test extract_symbols`
Expected: FAIL — `symbols` module missing.

- [ ] **Step 3: Implement.**

`src/symbols.rs`:
```rust
use crate::{Error, Result};
use dk_core::{RawCallEdge, Symbol};
use dk_engine::parser::{LanguageParser, engine::QueryDrivenParser, langs::rust::RustConfig};
use std::path::Path;

/// Parse a single Rust file and return its symbols + raw call edges.
/// Stateless — the caller batches across files.
pub fn extract_rust_file(source: &[u8], file_path: &Path) -> Result<(Vec<Symbol>, Vec<RawCallEdge>)> {
    let parser = QueryDrivenParser::new(RustConfig);
    let symbols = parser
        .extract_symbols(source, file_path)
        .map_err(|e| Error::Engine(format!("extract_symbols({}): {e}", file_path.display())))?;
    let calls = parser
        .extract_calls(source, file_path)
        .map_err(|e| Error::Engine(format!("extract_calls({}): {e}", file_path.display())))?;
    Ok((symbols, calls))
}
```

Re-export:
```rust
// src/lib.rs
pub mod symbols;
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-orchestrator`
Expected: both tests pass.

- [ ] **Step 5: Commit.**

(CodeRabbit pre-commit.)
```sh
git add crates/dkod-orchestrator/src/symbols.rs crates/dkod-orchestrator/src/lib.rs crates/dkod-orchestrator/tests/extract_symbols.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add Rust symbol + call-edge extraction via dk-engine"
```

### Task 16: Fixture Rust repos

**Files:**
- Create: `crates/dkod-orchestrator/tests/fixtures/basic/src/lib.rs`
- Create: `crates/dkod-orchestrator/tests/fixtures/trait_coupling/src/lib.rs`
- Create: `crates/dkod-orchestrator/tests/fixtures/big_struct/src/lib.rs`

No `Cargo.toml` for fixtures — they're parsed as source only, never compiled.

- [ ] **Step 1: `basic/src/lib.rs`** — 4 disconnected functions.

```rust
pub fn alpha() -> i32 { 1 }
pub fn beta() -> i32 { 2 }
pub fn gamma() -> i32 { 3 }
pub fn delta() -> i32 { 4 }
```

- [ ] **Step 2: `trait_coupling/src/lib.rs`** — trait + two impls + a caller.

```rust
pub trait Greeter {
    fn greet(&self) -> String;
}

pub struct English;
impl Greeter for English {
    fn greet(&self) -> String { "hello".into() }
}

pub struct French;
impl Greeter for French {
    fn greet(&self) -> String { "bonjour".into() }
}

pub fn say_english() -> String { English.greet() }
pub fn say_french() -> String { French.greet() }
```

- [ ] **Step 3: `big_struct/src/lib.rs`** — struct + several methods, target for the replace primitive.

```rust
pub struct Counter {
    value: i32,
}

impl Counter {
    pub fn new() -> Self { Self { value: 0 } }
    pub fn inc(&mut self) { self.value += 1 }
    pub fn get(&self) -> i32 { self.value }
}
```

- [ ] **Step 4: Commit.**

Config-only changeset; docs-like. Skip `/coderabbit:code-review`, note it in the PR.

```sh
git add crates/dkod-orchestrator/tests/fixtures/
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add test fixtures: basic, trait_coupling, big_struct"
```

### Task 17: Open PR M1-5

- [ ] **Step 1: Push + PR.**

```sh
git push -u origin m1/orch-symbols-fixtures
gh pr create --title "M1-5: dkod-orchestrator scaffold + Rust symbol extraction" --body "$(cat <<'EOF'
## Summary
- Scaffold `dkod-orchestrator` with error type and `dk-engine`/`dk-core` deps.
- Engine API probe (`examples/probe_engine_api.rs`) confirms `QueryDrivenParser::new(RustConfig)` shape on 0.3.x.
- `symbols::extract_rust_file` wraps the engine for both symbols and raw call edges.
- Three fixture source trees (basic, trait_coupling, big_struct) for downstream tasks.

## Test plan
- [x] `cargo test -p dkod-orchestrator` green
- [x] `cargo run --example probe_engine_api -p dkod-orchestrator` smoke-ok
- [ ] CodeRabbit PR-side review clean via `/coderabbit:autofix`
EOF
)"
```

- [ ] **Step 2: Wait + `/coderabbit:autofix`. STOP — ask before merging.**

---

# PR M1-6 — `dkod-orchestrator`: call graph + partitioner

### Task 18: Resolved call-graph builder — TDD

**Files:**
- Create: `crates/dkod-orchestrator/src/callgraph.rs`
- Modify: `crates/dkod-orchestrator/src/lib.rs`
- Create: `crates/dkod-orchestrator/tests/callgraph_build.rs`

**Design:** raw edges carry string `caller_name` / `callee_name`. Resolution = look up both by `qualified_name` (or by `name` fallback) in the symbol table. Unresolved edges are collected as warnings, not errors — real code has external calls.

- [ ] **Step 1: Write the failing test.**

`tests/callgraph_build.rs`:
```rust
use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::symbols::extract_rust_file;

#[test]
fn graph_resolves_intra_file_edges() {
    let src = br#"
pub fn login() { validate(); }
pub fn validate() -> bool { true }
"#;
    let (syms, calls) = extract_rust_file(src, std::path::Path::new("auth.rs")).unwrap();
    let g = CallGraph::build(&syms, &calls);

    let login_id = g.symbol_id_by_name("login").expect("login present");
    let validate_id = g.symbol_id_by_name("validate").expect("validate present");

    let succ = g.successors(login_id);
    assert!(succ.contains(&validate_id), "login should call validate");
}

#[test]
fn unresolved_edges_are_surfaced_not_panicking() {
    let src = b"pub fn boom() { external_thing(); }\n";
    let (syms, calls) = extract_rust_file(src, std::path::Path::new("x.rs")).unwrap();
    let g = CallGraph::build(&syms, &calls);
    // external_thing isn't in syms — it lands in `unresolved`, not a panic
    // and not in successors().
    assert!(g.unresolved_count() >= 1, "expected at least one unresolved edge");
    let id = g.symbol_id_by_name("boom").unwrap();
    assert!(g.successors(id).is_empty());
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-orchestrator --test callgraph_build`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement.**

`src/callgraph.rs`:
```rust
use dk_core::{RawCallEdge, Symbol, SymbolId};
use std::collections::{HashMap, HashSet};

pub struct CallGraph {
    symbol_index: HashMap<String, SymbolId>,   // name → id (last wins on dup)
    by_id: HashMap<SymbolId, Symbol>,
    adj: HashMap<SymbolId, HashSet<SymbolId>>, // directed
    undirected: HashMap<SymbolId, HashSet<SymbolId>>, // used by partitioner
    unresolved: usize,
}

impl CallGraph {
    pub fn build(symbols: &[Symbol], edges: &[RawCallEdge]) -> Self {
        let mut symbol_index = HashMap::new();
        let mut by_id = HashMap::new();
        for s in symbols {
            symbol_index.insert(s.qualified_name.clone(), s.id.clone());
            symbol_index.entry(s.name.clone()).or_insert(s.id.clone());
            by_id.insert(s.id.clone(), s.clone());
        }

        let mut adj: HashMap<SymbolId, HashSet<SymbolId>> = HashMap::new();
        let mut undirected: HashMap<SymbolId, HashSet<SymbolId>> = HashMap::new();
        let mut unresolved = 0usize;

        for e in edges {
            let (Some(caller), Some(callee)) = (
                symbol_index.get(&e.caller_name),
                symbol_index.get(&e.callee_name),
            ) else { unresolved += 1; continue; };

            if caller == callee { continue; }  // self-calls ignored
            adj.entry(caller.clone()).or_default().insert(callee.clone());
            undirected.entry(caller.clone()).or_default().insert(callee.clone());
            undirected.entry(callee.clone()).or_default().insert(caller.clone());
        }

        Self { symbol_index, by_id, adj, undirected, unresolved }
    }

    pub fn symbol_id_by_name(&self, name: &str) -> Option<SymbolId> {
        self.symbol_index.get(name).cloned()
    }

    pub fn successors(&self, id: SymbolId) -> Vec<SymbolId> {
        self.adj.get(&id).map(|s| s.iter().cloned().collect()).unwrap_or_default()
    }

    pub fn undirected_neighbours(&self, id: &SymbolId) -> impl Iterator<Item = &SymbolId> {
        self.undirected.get(id).into_iter().flatten()
    }

    pub fn unresolved_count(&self) -> usize { self.unresolved }

    pub fn symbol(&self, id: &SymbolId) -> Option<&Symbol> {
        self.by_id.get(id)
    }

    pub fn all_symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.by_id.values()
    }
}
```

Re-export:
```rust
// src/lib.rs
pub mod callgraph;
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-orchestrator`
Expected: green.

- [ ] **Step 5: Commit.**

(CodeRabbit pre-commit.)
```sh
git checkout -b m1/orch-partition origin/main
git add crates/dkod-orchestrator/src/callgraph.rs crates/dkod-orchestrator/src/lib.rs crates/dkod-orchestrator/tests/callgraph_build.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add CallGraph with directed + undirected adjacency"
```

### Task 19: Partitioner — core connected-components algorithm (TDD)

**Files:**
- Create: `crates/dkod-orchestrator/src/partition.rs`
- Modify: `crates/dkod-orchestrator/src/lib.rs`
- Create: `crates/dkod-orchestrator/tests/partition.rs`

**Algorithm:** union-find over `in_scope` symbols using the undirected call-graph. Each connected component becomes one group. `target_groups` is an upper-bound hint for v0 — if there are fewer CCs than `target_groups` we emit a `FewerGroupsThanTarget` warning; if more, a `MoreGroupsThanTarget` warning and we do not further subdivide. V0 is correctness-first — balancing is a future optimisation.

- [ ] **Step 1: Write the failing test.**

`tests/partition.rs`:
```rust
use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::partition::{partition, Warning};
use dkod_orchestrator::symbols::extract_rust_file;

fn fixture(path: &str) -> (Vec<dk_core::Symbol>, Vec<dk_core::RawCallEdge>) {
    let p = std::path::Path::new(path);
    let src = std::fs::read(p).unwrap();
    extract_rust_file(&src, p).unwrap()
}

#[test]
fn basic_four_functions_split_into_four_singleton_groups() {
    let (syms, calls) = fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = ["alpha", "beta", "gamma", "delta"].iter().map(|s| s.to_string()).collect();

    let p = partition(&in_scope, &g, 4).unwrap();
    assert_eq!(p.groups.len(), 4, "expected 4 disjoint singleton groups, got {}", p.groups.len());

    let all: Vec<_> = p.groups.iter().flat_map(|g| g.symbols.iter().map(|s| s.qualified_name.clone())).collect();
    for name in &in_scope {
        assert!(all.iter().any(|n| n.contains(name)), "{name} missing from partition");
    }
}

#[test]
fn trait_coupling_puts_coupled_symbols_into_one_group() {
    let (syms, calls) = fixture("tests/fixtures/trait_coupling/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    // say_english -> English::greet, say_french -> French::greet.
    // With undirected coupling, we expect ≤2 groups.
    let in_scope: Vec<String> = g.all_symbols().map(|s| s.qualified_name.clone()).collect();

    let p = partition(&in_scope, &g, 4).unwrap();
    assert!(p.groups.len() <= 2, "trait-coupled symbols should collapse; got {} groups", p.groups.len());
}

#[test]
fn fewer_ccs_than_target_emits_warning() {
    let (syms, calls) = fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = ["alpha", "beta"].iter().map(|s| s.to_string()).collect();

    let p = partition(&in_scope, &g, 4).unwrap();
    assert_eq!(p.groups.len(), 2);
    assert!(p.warnings.iter().any(|w| matches!(w, Warning::FewerGroupsThanTarget { target: 4, got: 2 })));
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-orchestrator --test partition`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement.**

`src/partition.rs`:
```rust
use crate::{Error, Result};
use crate::callgraph::CallGraph;
use dk_core::SymbolId;
use dkod_worktree::SymbolRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Warning {
    /// Partitioner produced fewer groups than the target — usually because
    /// coupling is too dense or the scope is small.
    FewerGroupsThanTarget { target: usize, got: usize },
    /// More connected components than the target. V0 does not subdivide.
    MoreGroupsThanTarget { target: usize, got: usize },
    /// An input qualified name did not resolve to a symbol.
    ScopeSymbolUnknown { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub symbols: Vec<SymbolRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    pub groups: Vec<Group>,
    pub warnings: Vec<Warning>,
}

struct UnionFind {
    parent: HashMap<SymbolId, SymbolId>,
}

impl UnionFind {
    fn new<'a>(ids: impl IntoIterator<Item = &'a SymbolId>) -> Self {
        let parent = ids.into_iter().map(|id| (id.clone(), id.clone())).collect();
        Self { parent }
    }
    fn find(&mut self, x: &SymbolId) -> SymbolId {
        let mut cur = x.clone();
        loop {
            let p = self.parent.get(&cur).cloned().unwrap_or_else(|| cur.clone());
            if p == cur { return cur; }
            cur = p;
        }
    }
    fn union(&mut self, a: &SymbolId, b: &SymbolId) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }
}

pub fn partition(
    in_scope: &[String],
    graph: &CallGraph,
    target_groups: usize,
) -> Result<Partition> {
    if target_groups == 0 {
        return Err(Error::InvalidPartition("target_groups must be >= 1".into()));
    }

    // Resolve names → SymbolIds.
    let mut resolved: Vec<SymbolId> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut in_scope_set: HashSet<SymbolId> = HashSet::new();

    for name in in_scope {
        match graph.symbol_id_by_name(name) {
            Some(id) => {
                if in_scope_set.insert(id.clone()) {
                    resolved.push(id);
                }
            }
            None => warnings.push(Warning::ScopeSymbolUnknown { name: name.clone() }),
        }
    }

    if resolved.is_empty() {
        return Ok(Partition { groups: Vec::new(), warnings });
    }

    // Union coupled symbols (undirected, restricted to in-scope).
    let mut uf = UnionFind::new(resolved.iter());
    for id in &resolved {
        for n in graph.undirected_neighbours(id) {
            if in_scope_set.contains(n) {
                uf.union(id, n);
            }
        }
    }

    // Group by representative.
    let mut buckets: BTreeMap<SymbolId, Vec<SymbolId>> = BTreeMap::new();
    for id in &resolved {
        let root = uf.find(id);
        buckets.entry(root).or_default().push(id.clone());
    }

    let groups: Vec<Group> = buckets
        .into_iter()
        .enumerate()
        .map(|(i, (_root, members))| {
            let gid = format!("g{}", i + 1);
            let mut symbols = Vec::with_capacity(members.len());
            for sid in members {
                if let Some(s) = graph.symbol(&sid) {
                    symbols.push(SymbolRef {
                        qualified_name: s.qualified_name.clone(),
                        file_path: s.file_path.clone(),
                        kind: format!("{:?}", s.kind),
                    });
                }
            }
            // Stable ordering inside a group.
            symbols.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
            Group { id: gid, symbols }
        })
        .collect();

    match groups.len().cmp(&target_groups) {
        std::cmp::Ordering::Less => {
            warnings.push(Warning::FewerGroupsThanTarget { target: target_groups, got: groups.len() });
        }
        std::cmp::Ordering::Greater => {
            warnings.push(Warning::MoreGroupsThanTarget { target: target_groups, got: groups.len() });
        }
        std::cmp::Ordering::Equal => {}
    }

    Ok(Partition { groups, warnings })
}
```

Re-export:
```rust
// src/lib.rs
pub mod partition;
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-orchestrator --test partition`
Expected: green.

- [ ] **Step 5: Commit.**

(CodeRabbit pre-commit.)
```sh
git add crates/dkod-orchestrator/src/partition.rs crates/dkod-orchestrator/src/lib.rs crates/dkod-orchestrator/tests/partition.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add partitioner: union-find over in-scope call graph"
```

### Task 20: Golden-file tests for partition output

**Files:**
- Create: `crates/dkod-orchestrator/tests/fixtures/golden/basic_all_four.json`
- Create: `crates/dkod-orchestrator/tests/fixtures/golden/trait_coupling_full.json`
- Create: `crates/dkod-orchestrator/tests/partition_golden.rs`

**Write the test first**, then run it and capture the failing output; then update the golden files to match the canonicalised output the test would emit. `UPDATE=1 cargo test partition_golden` regenerates.

- [ ] **Step 1: Write the golden-harness test.**

`tests/partition_golden.rs`:
```rust
use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::partition::{partition, Partition};
use dkod_orchestrator::symbols::extract_rust_file;

fn load_fixture(path: &str) -> (Vec<dk_core::Symbol>, Vec<dk_core::RawCallEdge>) {
    let p = std::path::Path::new(path);
    extract_rust_file(&std::fs::read(p).unwrap(), p).unwrap()
}

fn canonical_json(p: &Partition) -> String {
    // Normalise: kinds are Debug-formatted, which is fine. Sort groups by id.
    let mut p = p.clone();
    p.groups.sort_by(|a, b| a.id.cmp(&b.id));
    serde_json::to_string_pretty(&p).unwrap()
}

fn assert_golden(actual: &str, path: &str) {
    let path = std::path::Path::new(path);
    if std::env::var_os("UPDATE").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("golden not found: {}. Run with UPDATE=1 to create.", path.display()));
    assert_eq!(actual.trim(), expected.trim(), "golden mismatch: {}", path.display());
}

#[test]
fn golden_basic_all_four() {
    let (syms, calls) = load_fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = ["alpha", "beta", "gamma", "delta"].iter().map(|s| s.to_string()).collect();
    let p = partition(&in_scope, &g, 4).unwrap();
    assert_golden(&canonical_json(&p), "tests/fixtures/golden/basic_all_four.json");
}

#[test]
fn golden_trait_coupling_full() {
    let (syms, calls) = load_fixture("tests/fixtures/trait_coupling/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = g.all_symbols().map(|s| s.qualified_name.clone()).collect();
    let p = partition(&in_scope, &g, 4).unwrap();
    assert_golden(&canonical_json(&p), "tests/fixtures/golden/trait_coupling_full.json");
}
```

- [ ] **Step 2: Regenerate goldens.**

Run: `UPDATE=1 cargo test -p dkod-orchestrator --test partition_golden`
Expected: writes two files; tests pass.

- [ ] **Step 3: Inspect the generated goldens.**

Check both JSON files by eye — confirm `basic_all_four.json` has 4 groups (`g1`–`g4`) and `trait_coupling_full.json` has 1–2 groups covering every in-scope symbol exactly once. If not, the partitioner or call-edge resolution has a bug; **fix it, don't update the golden to match broken output.**

- [ ] **Step 4: Re-run without UPDATE.**

Run: `cargo test -p dkod-orchestrator --test partition_golden`
Expected: PASS.

- [ ] **Step 5: Commit.**

(Code + small JSON fixtures — run `/coderabbit:code-review`.)
```sh
git add crates/dkod-orchestrator/tests/partition_golden.rs crates/dkod-orchestrator/tests/fixtures/golden/
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add golden-file tests for partition output"
```

### Task 21: Open PR M1-6

- [ ] **Step 1: Push + PR.**

```sh
git push -u origin m1/orch-partition
gh pr create --title "M1-6: dkod-orchestrator call graph + partitioner" --body "$(cat <<'EOF'
## Summary
- `CallGraph::build` — resolves raw edges to SymbolIds, tracks directed + undirected adjacency, surfaces unresolved edges.
- `partition` — union-find over the undirected call graph produces disjoint groups that keep coupled symbols together. Warnings for under/over target and unknown scope symbols.
- Golden-file regression tests for `basic` (4 singletons) and `trait_coupling` (coupled).

## Test plan
- [x] `cargo test -p dkod-orchestrator` green (all suites including goldens).
- [ ] CodeRabbit PR review clean via `/coderabbit:autofix`.
EOF
)"
```

- [ ] **Step 2: Wait + `/coderabbit:autofix`. STOP — ask before merging.**

---

# PR M1-7 — `dkod-orchestrator`: AST symbol replace primitive

### Task 22: `replace_symbol` — happy path TDD

**Files:**
- Create: `crates/dkod-orchestrator/src/replace.rs`
- Modify: `crates/dkod-orchestrator/src/lib.rs`
- Create: `crates/dkod-orchestrator/tests/replace_symbol.rs`

**Design:**
`replace_symbol(current_source: &[u8], qualified_name: &str, new_body_source: &str) -> Result<ReplaceOutcome>`.
- Extract symbols from `current_source`. Find the one whose `qualified_name` matches.
- `Symbol.span` carries byte offsets (from `dk-core`). Splice `new_body_source` between `span.start_byte..span.end_byte`.
- Re-parse the new source; if extraction fails or the replaced symbol is gone, return `ReplaceOutcome::ParsedOk` only if the new source is still syntactically Rust; otherwise `ReplaceOutcome::Fallback` (still returns the new source).
- Symbol not found → `Error::SymbolNotFound`.

**Why `ReplaceOutcome` instead of a bare `Vec<u8>`:** design §edge case #5 requires that we signal when the AST validation didn't verify the replacement. The locked-write path (M2) consumes this signal to record a warning.

- [ ] **Step 1: Write the failing test.**

`tests/replace_symbol.rs`:
```rust
use dkod_orchestrator::replace::{replace_symbol, ReplaceOutcome};

#[test]
fn replaces_existing_function_body_cleanly() {
    let src = b"pub fn hello() -> &'static str { \"hi\" }\n";
    let outcome = replace_symbol(src, "hello", "pub fn hello() -> &'static str { \"HELLO\" }").unwrap();
    match outcome {
        ReplaceOutcome::ParsedOk { new_source } => {
            let s = String::from_utf8(new_source).unwrap();
            assert!(s.contains("HELLO"));
            assert!(!s.contains("\"hi\""));
        }
        ReplaceOutcome::Fallback { .. } => panic!("expected ParsedOk"),
    }
}

#[test]
fn missing_symbol_errors() {
    let src = b"pub fn hello() {}\n";
    let err = replace_symbol(src, "nope", "pub fn nope() {}").unwrap_err();
    assert!(format!("{err}").contains("not found"), "got: {err}");
}

#[test]
fn replaces_one_of_many_preserving_others() {
    let src = br#"pub fn a() -> i32 { 1 }
pub fn b() -> i32 { 2 }
pub fn c() -> i32 { 3 }
"#;
    let outcome = replace_symbol(src, "b", "pub fn b() -> i32 { 20 }").unwrap();
    let s = match outcome {
        ReplaceOutcome::ParsedOk { new_source } => String::from_utf8(new_source).unwrap(),
        ReplaceOutcome::Fallback { .. } => panic!("expected ParsedOk"),
    };
    assert!(s.contains("pub fn a() -> i32 { 1 }"));
    assert!(s.contains("pub fn b() -> i32 { 20 }"));
    assert!(s.contains("pub fn c() -> i32 { 3 }"));
    assert!(!s.contains("pub fn b() -> i32 { 2 }"));
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-orchestrator --test replace_symbol`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement (happy path only; fallback added in the next task).**

`src/replace.rs`:
```rust
use crate::symbols::extract_rust_file;
use crate::{Error, Result};
use std::path::PathBuf;

#[derive(Debug)]
pub enum ReplaceOutcome {
    /// The splice succeeded and a follow-up parse also succeeded.
    ParsedOk { new_source: Vec<u8> },
    /// The splice was applied but the follow-up parse did not verify; caller
    /// should record an `UnsupportedConstruct` warning (design §edge case #5).
    Fallback { new_source: Vec<u8>, reason: String },
}

/// Replace a symbol's source span with `new_body_source`.
///
/// `qualified_name` matches either `Symbol.qualified_name` or `Symbol.name`
/// (the latter so callers can pass short names when the file has no module
/// prefix ambiguity).
pub fn replace_symbol(
    current_source: &[u8],
    qualified_name: &str,
    new_body_source: &str,
) -> Result<ReplaceOutcome> {
    let path = PathBuf::from("<in-memory>");
    let (syms, _calls) = extract_rust_file(current_source, &path)?;
    let target = syms
        .iter()
        .find(|s| s.qualified_name == qualified_name || s.name == qualified_name)
        .ok_or_else(|| Error::SymbolNotFound { name: qualified_name.into(), file: path.clone() })?;

    let start = target.span.start_byte as usize;
    let end = target.span.end_byte as usize;
    if start > end || end > current_source.len() {
        return Err(Error::ReplaceFailed(format!(
            "span out of bounds: start={start} end={end} len={}",
            current_source.len()
        )));
    }

    let mut new_source = Vec::with_capacity(current_source.len() + new_body_source.len());
    new_source.extend_from_slice(&current_source[..start]);
    new_source.extend_from_slice(new_body_source.as_bytes());
    new_source.extend_from_slice(&current_source[end..]);

    // Validate by re-parsing — if the replaced symbol (or at least some
    // symbol) is present, call it ParsedOk.
    let reparse = extract_rust_file(&new_source, &path);
    match reparse {
        Ok((new_syms, _)) if !new_syms.is_empty() => Ok(ReplaceOutcome::ParsedOk { new_source }),
        Ok(_) => Ok(ReplaceOutcome::Fallback {
            new_source,
            reason: "re-parse yielded no symbols".into(),
        }),
        Err(e) => Ok(ReplaceOutcome::Fallback {
            new_source,
            reason: format!("re-parse failed: {e}"),
        }),
    }
}
```

Re-export:
```rust
// src/lib.rs
pub mod replace;
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-orchestrator --test replace_symbol`
Expected: green.

- [ ] **Step 5: Commit.**

```sh
git checkout -b m1/orch-replace origin/main
git add crates/dkod-orchestrator/src/replace.rs crates/dkod-orchestrator/src/lib.rs crates/dkod-orchestrator/tests/replace_symbol.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add replace_symbol primitive with parse-verified happy path"
```

### Task 23: `replace_symbol` — fallback path exercised

**Files:**
- Modify: `crates/dkod-orchestrator/tests/replace_symbol.rs`

- [ ] **Step 1: Add fallback test.**

Append to `tests/replace_symbol.rs`:
```rust
#[test]
fn syntactically_invalid_replacement_yields_fallback() {
    let src = b"pub fn hello() -> i32 { 1 }\n";
    // Intentionally broken — unmatched brace.
    let outcome = replace_symbol(src, "hello", "pub fn hello() -> i32 { ").unwrap();
    match outcome {
        ReplaceOutcome::Fallback { new_source, reason } => {
            let s = String::from_utf8(new_source).unwrap();
            assert!(s.contains("pub fn hello() -> i32 { "));
            assert!(!reason.is_empty());
        }
        ReplaceOutcome::ParsedOk { .. } => panic!("broken replacement must not be reported as ParsedOk"),
    }
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p dkod-orchestrator --test replace_symbol`
Expected: all four tests pass — the fallback branch is exercised.

If it fails (i.e. the re-parse incorrectly returns `Ok` with non-empty symbols for broken input), tighten the heuristic in `replace.rs`: require that the **originally replaced symbol name** is present in the re-parse before declaring `ParsedOk`. Update the Step 3 impl in Task 22 accordingly.

- [ ] **Step 3: Commit.**

```sh
git add crates/dkod-orchestrator/tests/replace_symbol.rs crates/dkod-orchestrator/src/replace.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Exercise replace_symbol fallback path"
```

### Task 24: Open PR M1-7

- [ ] **Step 1: Push + PR.**

```sh
git push -u origin m1/orch-replace
gh pr create --title "M1-7: dkod-orchestrator AST symbol replace primitive" --body "$(cat <<'EOF'
## Summary
- `replace_symbol(src, name, new_body) -> ReplaceOutcome` splices at the AST span from `dk-engine`.
- Re-parse verification → `ParsedOk` | `Fallback` — design §edge case #5.
- No locking here; that is M2's concern in `dkod_write_symbol`.

## Test plan
- [x] Happy-path replacement preserves siblings.
- [x] Missing symbol errors (`SymbolNotFound`).
- [x] Syntactically invalid replacement yields `Fallback` with a reason.
- [ ] CodeRabbit PR-side review clean via `/coderabbit:autofix`.
EOF
)"
```

- [ ] **Step 2: Wait + `/coderabbit:autofix`. STOP — ask before merging.**

---

# PR M1-8 — `dkod-orchestrator`: commit-per-group + milestone E2E

### Task 25: `commit_per_group` — TDD against a tempdir git repo

**Files:**
- Create: `crates/dkod-orchestrator/src/commit.rs`
- Modify: `crates/dkod-orchestrator/src/lib.rs`
- Create: `crates/dkod-orchestrator/tests/commit_per_group.rs`

**Design:**
`commit_per_group(repo_root, paths, session_id, group_ids)` produces one commit per group on the *current* branch (the caller has already `checkout -b dk/<sid>`). Each commit stages only the files recorded in that group's `writes.jsonl`. Authorship is forced per `dkod_worktree::branch::commit_paths`.

- [ ] **Step 1: Write the failing test.**

`tests/commit_per_group.rs`:
```rust
use dkod_orchestrator::commit::commit_per_group;
use dkod_worktree::{branch, GroupSpec, GroupStatus, Paths, SessionId, SymbolRef, WriteLog, WriteRecord};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn init_repo(dir: &Path) {
    Command::new("git").args(["init", "-b", "main"]).current_dir(dir).status().unwrap();
    std::fs::write(dir.join("README.md"), "hi").unwrap();
    let commit_env = [
        ("GIT_AUTHOR_NAME", "Haim Ari"),
        ("GIT_AUTHOR_EMAIL", "haimari1@gmail.com"),
        ("GIT_COMMITTER_NAME", "Haim Ari"),
        ("GIT_COMMITTER_EMAIL", "haimari1@gmail.com"),
    ];
    Command::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    let mut c = Command::new("git");
    c.args(["commit", "-m", "init"]).current_dir(dir);
    for (k, v) in commit_env { c.env(k, v); }
    c.status().unwrap();
}

#[test]
fn writes_one_commit_per_group_with_forced_identity() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let sid = SessionId::from("sess-abc");
    branch::create_dk_branch(repo, "main", sid.as_str()).unwrap();

    let paths = Paths::new(repo);

    // Group g1: modifies src/a.rs
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "pub fn a() {}\n").unwrap();
    GroupSpec {
        id: "g1".into(),
        symbols: vec![SymbolRef {
            qualified_name: "a".into(),
            file_path: PathBuf::from("src/a.rs"),
            kind: "function".into(),
        }],
        agent_prompt: "…".into(),
        status: GroupStatus::Done,
    }.save(&paths, &sid).unwrap();
    let log_g1 = WriteLog::open(&paths, &sid, "g1").unwrap();
    log_g1.append(&WriteRecord {
        symbol: "a".into(),
        file_path: PathBuf::from("src/a.rs"),
        timestamp: "2026-04-24T12:00:00Z".into(),
    }).unwrap();

    // Group g2: modifies src/b.rs
    std::fs::write(repo.join("src/b.rs"), "pub fn b() {}\n").unwrap();
    GroupSpec {
        id: "g2".into(),
        symbols: vec![SymbolRef {
            qualified_name: "b".into(),
            file_path: PathBuf::from("src/b.rs"),
            kind: "function".into(),
        }],
        agent_prompt: "…".into(),
        status: GroupStatus::Done,
    }.save(&paths, &sid).unwrap();
    let log_g2 = WriteLog::open(&paths, &sid, "g2").unwrap();
    log_g2.append(&WriteRecord {
        symbol: "b".into(),
        file_path: PathBuf::from("src/b.rs"),
        timestamp: "2026-04-24T12:00:01Z".into(),
    }).unwrap();

    commit_per_group(repo, &paths, &sid, &["g1".into(), "g2".into()]).unwrap();

    let log = Command::new("git")
        .args(["log", "--format=%an <%ae> | %s"])
        .current_dir(repo)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&log.stdout);
    // Most recent first.
    let mut lines = text.lines();
    let l1 = lines.next().unwrap();
    let l2 = lines.next().unwrap();
    let l3 = lines.next().unwrap();
    assert!(l1.contains("Haim Ari <haimari1@gmail.com>") && l1.contains("g2"), "top commit = g2; got {l1}");
    assert!(l2.contains("Haim Ari <haimari1@gmail.com>") && l2.contains("g1"), "second = g1; got {l2}");
    assert!(l3.contains("init"), "base commit preserved");
}
```

- [ ] **Step 2: Run — expect failure.**

Run: `cargo test -p dkod-orchestrator --test commit_per_group`
Expected: FAIL — `commit_per_group` missing.

- [ ] **Step 3: Implement.**

`src/commit.rs`:
```rust
use crate::{Error, Result};
use dkod_worktree::{branch, Paths, SessionId, WriteLog};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Write one commit per group id on the current branch. Caller is responsible
/// for having checked out the dk-branch.
///
/// `group_ids` is processed in the order given; the resulting `git log` has
/// group `g1` older than `g2` older than `g3`, etc.
pub fn commit_per_group(
    repo_root: &Path,
    paths: &Paths,
    session_id: &SessionId,
    group_ids: &[String],
) -> Result<()> {
    for gid in group_ids {
        let records = WriteLog::read_all(paths, session_id, gid)?;
        if records.is_empty() { continue; }

        // Stable, deduplicated file set.
        let files: BTreeSet<PathBuf> = records.iter().map(|r| r.file_path.clone()).collect();
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        let msg = format!("group {gid}: {} symbol writes", records.len());
        branch::commit_paths(repo_root, &file_refs, &msg)
            .map_err(Error::from)?;
    }
    Ok(())
}
```

Re-export:
```rust
// src/lib.rs
pub mod commit;
```

- [ ] **Step 4: Run — expect pass.**

Run: `cargo test -p dkod-orchestrator --test commit_per_group`
Expected: green.

- [ ] **Step 5: Commit.**

```sh
git checkout -b m1/orch-commit-e2e origin/main
git add crates/dkod-orchestrator/src/commit.rs crates/dkod-orchestrator/src/lib.rs crates/dkod-orchestrator/tests/commit_per_group.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add commit_per_group driven by writes.jsonl"
```

### Task 26: Milestone E2E integration test

**Files:**
- Create: `crates/dkod-orchestrator/tests/milestone1_e2e.rs`

- [ ] **Step 1: Write the test — this is the M1 acceptance test.**

`tests/milestone1_e2e.rs`:
```rust
//! End-to-end happy path for milestone 1:
//! init → plan → simulate two agents landing symbol writes → commit_per_group →
//! assert git log has the expected shape.

use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::commit::commit_per_group;
use dkod_orchestrator::partition::partition;
use dkod_orchestrator::replace::{replace_symbol, ReplaceOutcome};
use dkod_orchestrator::symbols::extract_rust_file;
use dkod_worktree::{branch, init_repo, GroupSpec, GroupStatus, Paths, SessionId, WriteLog, WriteRecord};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn init_git(dir: &Path) {
    Command::new("git").args(["init", "-b", "main"]).current_dir(dir).status().unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn alpha() -> i32 { 1 }\npub fn beta() -> i32 { 2 }\n").unwrap();
    Command::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .env("GIT_AUTHOR_NAME", "Haim Ari")
        .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
        .env("GIT_COMMITTER_NAME", "Haim Ari")
        .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
        .current_dir(dir).status().unwrap();
}

#[test]
fn m1_happy_path_produces_expected_git_log() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git(repo);

    // Phase 0: init
    init_repo(repo, None).unwrap();
    let paths = Paths::new(repo);
    let sid = SessionId::generate();

    // Phase 2: plan
    let src_path = repo.join("src/lib.rs");
    let src = std::fs::read(&src_path).unwrap();
    let (syms, calls) = extract_rust_file(&src, &src_path).unwrap();
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = syms.iter().map(|s| s.qualified_name.clone()).collect();
    let plan = partition(&in_scope, &g, 2).unwrap();
    assert_eq!(plan.groups.len(), 2, "expected 2 singleton groups; got {}", plan.groups.len());

    // Phase 3: execute_begin
    branch::create_dk_branch(repo, "main", sid.as_str()).unwrap();

    // Simulate two agents, one per group.
    let mut group_ids = Vec::new();
    for (i, group) in plan.groups.iter().enumerate() {
        GroupSpec {
            id: group.id.clone(),
            symbols: group.symbols.clone(),
            agent_prompt: format!("bump values in group {}", group.id),
            status: GroupStatus::Done,
        }.save(&paths, &sid).unwrap();

        let log = WriteLog::open(&paths, &sid, &group.id).unwrap();
        for sym in &group.symbols {
            // Touch the actual source via replace_symbol.
            let current = std::fs::read(repo.join(&sym.file_path)).unwrap();
            let short = sym.qualified_name.rsplit("::").next().unwrap();
            let new_body = format!("pub fn {short}() -> i32 {{ {}0 }}", i + 1);
            let outcome = replace_symbol(&current, &sym.qualified_name, &new_body).unwrap();
            let new_src = match outcome {
                ReplaceOutcome::ParsedOk { new_source } => new_source,
                ReplaceOutcome::Fallback { new_source, .. } => new_source,
            };
            std::fs::write(repo.join(&sym.file_path), &new_src).unwrap();

            log.append(&WriteRecord {
                symbol: sym.qualified_name.clone(),
                file_path: sym.file_path.clone(),
                timestamp: "2026-04-24T12:00:00Z".into(),
            }).unwrap();
        }
        group_ids.push(group.id.clone());
    }

    // Phase 4: commit
    commit_per_group(repo, &paths, &sid, &group_ids).unwrap();

    // Assertions: one commit per group on dk-branch, each with enforced identity.
    let log = Command::new("git")
        .args(["log", "--format=%H %an <%ae> | %s"])
        .current_dir(repo)
        .output().unwrap();
    let text = String::from_utf8_lossy(&log.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1 + group_ids.len(), "expected init + {} group commits; got {}\n{text}", group_ids.len(), lines.len());
    for line in lines.iter().take(group_ids.len()) {
        assert!(line.contains("Haim Ari <haimari1@gmail.com>"), "identity not enforced: {line}");
    }

    // Worktree actually reflects the replacements.
    let final_src = String::from_utf8(std::fs::read(&src_path).unwrap()).unwrap();
    assert!(final_src.contains("10") || final_src.contains("20"), "replacements not applied: {final_src}");
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p dkod-orchestrator --test milestone1_e2e -- --nocapture`
Expected: test passes; printed git log shows init + 2 group commits.

- [ ] **Step 3: Commit.**

```sh
git add crates/dkod-orchestrator/tests/milestone1_e2e.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add milestone 1 end-to-end integration test"
```

### Task 27: Green workspace + open PR M1-8

- [ ] **Step 1: Full-workspace run.**

Run: `cargo test --workspace`
Expected: every suite green across both crates.

If anything fails, fix before the PR.

- [ ] **Step 2: Push + PR.**

```sh
git push -u origin m1/orch-commit-e2e
gh pr create --title "M1-8: commit-per-group + milestone 1 E2E test" --body "$(cat <<'EOF'
## Summary
- `commit::commit_per_group` writes one commit per group from `writes.jsonl`, using the identity-forced helper from `dkod-worktree::branch`.
- `milestone1_e2e.rs` exercises the full M1 library surface: init → partition → replace → write-log → commit, asserting the resulting git log shape.
- **Milestone 1 closes here.** No MCP, no CLI, no plugin.

## Test plan
- [x] `cargo test --workspace` green on macOS/Linux.
- [x] E2E test produces 1 init commit + N group commits with enforced identity.
- [ ] CodeRabbit PR-side review clean via `/coderabbit:autofix`.
EOF
)"
```

- [ ] **Step 3: Wait + `/coderabbit:autofix`. STOP — ask before merging.**

### Task 28: Milestone 1 close-out

After PR M1-8 merges:

- [ ] **Step 1: Verify `main` is green.**

```sh
git checkout main && git pull --ff-only
cargo test --workspace
```
Expected: everything passes on `main`.

- [ ] **Step 2: Tag the milestone.**

```sh
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git tag -a v0.1.0-m1 -m "Milestone 1: dkod-worktree + dkod-orchestrator"
git push origin v0.1.0-m1
```

- [ ] **Step 3: Report to the user. Milestone 1 done; next step is milestone 2 (`dkod-mcp`), which requires its own plan.**

---

## Self-review

**Spec coverage** — every M1 scope bullet from `docs/design.md` §Ship order #1 is covered:
- `.dkod/` layout: PR M1-2 (`Paths`, `Config`), PR M1-3 (`Manifest`, `GroupSpec`, `WriteLog`). ✓
- dk-branch lifecycle: PR M1-4 (`branch::create_dk_branch`/`destroy_dk_branch`). ✓
- Symbol-graph planner: PR M1-5 (extract) + M1-6 (call graph + partitioner). ✓
- Commit-per-group: PR M1-8 (`commit_per_group`). ✓
- AST symbol replacement primitive: PR M1-7 (`replace_symbol`). Needed by M2 but implemented in M1 because the core logic lives in `dkod-orchestrator`. ✓
- Fixture-based unit tests: `crates/dkod-orchestrator/tests/fixtures/` + goldens in M1-6. ✓
- Commit authorship: enforced via `GIT_AUTHOR_*` / `GIT_COMMITTER_*` env in `branch::commit_paths`. ✓
- Claude Code plugin, CLI, MCP: **deliberately absent** — those are M2–M4. ✓

**Placeholder scan** — no "TBD", no "implement later", no "similar to Task N"; code blocks present wherever an engineer would need to type something. ✓

**Type consistency** — `SymbolRef`, `GroupSpec`, `Paths`, `SessionId`, `WriteRecord` names match across tasks; `Partition.groups: Vec<Group>`, `Group.symbols: Vec<SymbolRef>` consistent from Task 19 through Task 26. `ReplaceOutcome::{ParsedOk, Fallback}` referenced the same way in Tasks 22, 23, 26. ✓

**Open risks (known, acceptable for M1):**

1. Tree-sitter span fields on `dk_core::Span` — Task 22 assumes `start_byte` / `end_byte`. If the 0.3.x type uses different names (e.g. `byte_range: (u32, u32)`), adjust `replace.rs` at that point. The API probe (Task 14) catches it before it's baked in.
2. `QueryDrivenParser::new` signature — ditto, covered by Task 14's probe.
3. `Symbol.id: SymbolId` equality semantics — assumed to be stable within a single parse. If it isn't (i.e. re-parsing yields new ids for the same source), the partitioner's union-find over `SymbolId` still works because we only use the graph from a single parse. Downstream code that re-parses (replace + commit) uses `qualified_name` / `file_path`, not ids.

These are explicitly flagged so the executor doesn't silently paper over them.

---

## Execution handoff

Plan complete and saved to `docs/plans/2026-04-24-milestone-1-plan.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

Which approach?
