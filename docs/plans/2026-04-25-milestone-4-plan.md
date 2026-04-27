# Milestone 4: Plugin manifest + skill + slash commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Claude Code plugin layer for dkod-swarm — `plugin/.claude-plugin/plugin.json` manifest, `plugin/.mcp.json` MCP-server launcher, the `dkod-swarm` skill that instructs Claude how to drive the 8 MCP tools, three slash commands (`/dkod-swarm:plan`, `/dkod-swarm:execute`, `/dkod-swarm:pr`), and the `parallel-executor` subagent definition. Plus a repo-root `.claude-plugin/marketplace.json` so `/plugin marketplace add dkod-io/dkod-swarm` resolves the plugin's location. Milestone ends with a green `cargo test --workspace` plus structural validation tests that assert every plugin file parses (JSON / YAML frontmatter) and the manifest's name is `dkod-swarm`.

**Architecture:** Pure content milestone — no Rust source touched in `crates/`. The plugin directory mirrors the conventions of the existing `dkod` plugin (`~/.claude/plugins/cache/dkod/dkod/0.2.87/`, read-only inspection only — no source modifications) and the `superpowers` plugin (`~/.claude/plugins/cache/claude-plugins-official/superpowers/`). The marketplace manifest at the repo root points to `plugin/` as the plugin root; the plugin manifest at `plugin/.claude-plugin/plugin.json` is what Claude Code loads. The `.mcp.json` for development launches our compiled `dkod` binary in `--mcp` mode (M3-3); marketplace publish (M6) will harden this to a binary-distribution model.

**Tech Stack:** Markdown + JSON + YAML frontmatter. Validation tests live in a new dev-only test file in `crates/dkod-cli` and use `serde_json` (already a dev/runtime dep) plus a small frontmatter regex parser — no new dependencies. The skill is the substantive deliverable: ~300–500 lines of carefully-worded instruction telling Claude how to orchestrate the 8 MCP tools end-to-end with parallel Task subagents.

---

## File Structure

New files only. Nothing under `crates/` is modified except for one new validation test file in `crates/dkod-cli/tests/`.

```text
.claude-plugin/
└── marketplace.json                       # marketplace manifest pointing at ./plugin
plugin/
├── .claude-plugin/
│   └── plugin.json                        # plugin manifest (name: dkod-swarm)
├── .mcp.json                              # launches `dkod --mcp`
├── README.md                              # plugin-specific README (install + invocation)
├── commands/
│   ├── plan.md                            # /dkod-swarm:plan
│   ├── execute.md                         # /dkod-swarm:execute
│   └── pr.md                              # /dkod-swarm:pr
├── skills/
│   └── dkod-swarm/
│       └── SKILL.md                       # the instruction manual
└── agents/
    └── parallel-executor.md               # parallel-executor subagent
crates/dkod-cli/tests/
└── plugin_layout.rs                       # validation: JSON parses, frontmatter present, plugin name matches
```

---

## PR Plan

Milestone 4 lands in **3 PRs**. Each PR is a feature branch off `main`, opened fresh, and goes through the full CodeRabbit loop per `CLAUDE.md` (local `/coderabbit:code-review` → fix → re-review → commit/push → wait for PR review → `/coderabbit:autofix` → merge autonomously once clean). Branch names match the PR title prefix.

| PR | Branch | Scope | Tasks |
|----|--------|-------|-------|
| M4-1 | `m4/plugin-scaffold` | manifest + .mcp.json + marketplace.json + README + validation test | 1–4 |
| M4-2 | `m4/plugin-skill` | `skills/dkod-swarm/SKILL.md` (the substantive instruction manual) | 5–6 |
| M4-3 | `m4/plugin-commands-agents` | three slash commands + parallel-executor subagent | 7–9 |

**Docs-only commits**: every M4 commit is markdown / JSON only — no Rust source under `crates/`. Per CLAUDE.md, docs-only commits skip the local `/coderabbit:code-review` pre-commit run; the PR-side bot still runs and findings are addressed. The lone exception is M4-1's validation test file (`crates/dkod-cli/tests/plugin_layout.rs`), which IS Rust source and so its commit DOES go through local `/coderabbit:code-review`.

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
- Skip local `/coderabbit:code-review` when the changeset is markdown / JSON only (CLAUDE.md docs-only rule). The Rust test file in M4-1 task 4 IS code and DOES go through the local CR loop.

Every PR MUST:

- Title ≤ 70 chars.
- Body = short summary + test-plan checklist.
- Open ONE PR at a time. Do not start the next PR's branch until the current one is merged.
- **Merge autonomously** once CodeRabbit is clean and `cargo test --workspace` is green.

---

# PR M4-1 — Plugin scaffold

## Task 1: Repo-root marketplace manifest

**Files:**
- Create: `.claude-plugin/marketplace.json`

- [ ] **Step 1: Create the marketplace manifest.**

```json
{
  "name": "dkod-swarm",
  "description": "Local-first parallel AI coding agents — N agents, same files, one PR. AST-level symbol partitioning runs entirely on the user's machine; no platform, no API, no network coordination.",
  "owner": {
    "name": "Haim Ari",
    "email": "haimari1@gmail.com"
  },
  "plugins": [
    {
      "name": "dkod-swarm",
      "description": "Parallel agent execution against a single git worktree using AST-level symbol partitioning. Two agents safely rewrite different functions in the same file; merge happens in-place at write time, not at end-of-flow.",
      "version": "0.4.0",
      "source": "./plugin",
      "author": {
        "name": "Haim Ari",
        "email": "haimari1@gmail.com"
      }
    }
  ]
}
```

The `"source": "./plugin"` field tells Claude Code that the plugin lives in the `plugin/` subdirectory; otherwise the loader would expect the repo root to be the plugin. Version `0.4.0` aligns with the milestone — `v0.4.0-m4` is the release tag.

- [ ] **Step 2: Branch + commit (docs-only, skip local CR).**

```sh
git checkout -b m4/plugin-scaffold
git add .claude-plugin/marketplace.json
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add marketplace manifest pointing at plugin/"
```

## Task 2: Plugin manifest + .mcp.json + README

**Files:**
- Create: `plugin/.claude-plugin/plugin.json`
- Create: `plugin/.mcp.json`
- Create: `plugin/README.md`

- [ ] **Step 1: Create `plugin/.claude-plugin/plugin.json`.**

```json
{
  "name": "dkod-swarm",
  "version": "0.4.0",
  "description": "Local-first parallel AI coding agents. N Task subagents rewrite different functions in the same file simultaneously; AST-level merge happens in-place via the dkod-mcp server. One worktree, one dk-branch, one PR. Zero network coordination.",
  "author": {
    "name": "Haim Ari",
    "email": "haimari1@gmail.com"
  },
  "homepage": "https://github.com/dkod-io/dkod-swarm",
  "repository": "https://github.com/dkod-io/dkod-swarm",
  "license": "MIT",
  "keywords": [
    "parallel",
    "multi-agent",
    "ast",
    "symbol-merge",
    "rust",
    "tree-sitter",
    "mcp",
    "local-first",
    "claude-code"
  ]
}
```

- [ ] **Step 2: Create `plugin/.mcp.json`.**

This file tells Claude Code how to spawn the MCP server. For M4 (pre-marketplace-publish) we use `cargo run` so the user can develop against the source tree without a binary build step:

```json
{
  "mcpServers": {
    "dkod-swarm": {
      "command": "cargo",
      "args": [
        "run",
        "--quiet",
        "--manifest-path",
        "${CLAUDE_PLUGIN_ROOT}/../Cargo.toml",
        "-p",
        "dkod-cli",
        "--bin",
        "dkod",
        "--",
        "--mcp"
      ]
    }
  }
}
```

`${CLAUDE_PLUGIN_ROOT}` is set by Claude Code to the plugin directory (`plugin/`). The workspace `Cargo.toml` lives one level up. M6 (marketplace publish) replaces this with a binary-distribution model.

- [ ] **Step 3: Create `plugin/README.md`.** (Outer fence is four backticks because the body contains a triple-backtick shell block.)

````markdown
# dkod-swarm — Claude Code plugin

The Claude Code-side of [dkod-swarm](https://github.com/dkod-io/dkod-swarm) — a skill, three slash commands, and a parallel-executor subagent that orchestrate the 8-tool MCP server shipped in `crates/dkod-mcp`.

## What it does

When you give Claude a code task that touches multiple symbols (e.g. "refactor auth to use passkeys"), the skill instructs Claude to:

1. Call `dkod_plan` to partition the task by call-graph coupling
2. Call `dkod_execute_begin` to mint a session and a `dk/<sid>` branch
3. Spawn N Task subagents in parallel, each owning one symbol group
4. Each subagent calls `dkod_write_symbol` (AST-aware) for every edit in its partition
5. After all subagents finish, call `dkod_commit` (one git commit per group)
6. Call `dkod_pr` to push and open a GitHub PR

The result: parallel work on the same file with no merge conflicts, because edits compose at the AST level as they happen.

## Install (development)

From a clone of `dkod-io/dkod-swarm`:

```sh
/plugin marketplace add /absolute/path/to/dkod-swarm
/plugin install dkod-swarm@dkod-swarm
```

The `.mcp.json` runs `cargo run -p dkod-cli --bin dkod -- --mcp` against the workspace, so the source tree must be present and `cargo` must be on `PATH`. Marketplace install with a pre-built binary lands in M6.

## Slash commands

- `/dkod-swarm:plan <task>` — invoke `dkod_plan` for review
- `/dkod-swarm:execute` — drive the full plan→execute→commit flow
- `/dkod-swarm:pr <title>` — finalize and open the PR

## Subagent

- `parallel-executor` — orchestrates Task subagents per partition group; surfaces conflict events via `dkod_status`
````

- [ ] **Step 4: Commit (docs-only, skip local CR).**

```sh
git add plugin/.claude-plugin/plugin.json plugin/.mcp.json plugin/README.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add plugin manifest + .mcp.json + README"
```

## Task 3: Plugin layout placeholders

**Files:**
- Create: `plugin/commands/.gitkeep`
- Create: `plugin/skills/dkod-swarm/.gitkeep`
- Create: `plugin/agents/.gitkeep`

- [ ] **Step 1: Create empty placeholder files** so the directory structure is committed even before M4-2 / M4-3 fill it. Some marketplace tooling refuses to load a plugin with missing directories.

```sh
mkdir -p plugin/commands plugin/skills/dkod-swarm plugin/agents
touch plugin/commands/.gitkeep plugin/skills/dkod-swarm/.gitkeep plugin/agents/.gitkeep
```

- [ ] **Step 2: Commit.**

```sh
git add plugin/commands/.gitkeep plugin/skills/dkod-swarm/.gitkeep plugin/agents/.gitkeep
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add plugin/ layout placeholders for skill/commands/agents"
```

## Task 4: Validation test

**Files:**
- Create: `crates/dkod-cli/tests/plugin_layout.rs`

This test asserts the structural invariants of the plugin layout. It is the only Rust source change in M4 — its commit DOES go through `/coderabbit:code-review`.

- [ ] **Step 1: Write the test.**

```rust
//! Structural validation for the dkod-swarm plugin layout.
//!
//! These tests don't exercise the plugin's *behaviour* — that is M5's job
//! (e2e smoke against a real Rust sandbox). What they assert is that the
//! plugin directory under `plugin/` is *shaped correctly*: manifests parse,
//! markdown files have the expected frontmatter delimiter, and the
//! plugin.json's `name` field matches what the marketplace.json advertises.
//!
//! The test only runs from the workspace root; it locates files relative
//! to `CARGO_MANIFEST_DIR`.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `crates/dkod-cli`; workspace root is two up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn marketplace_manifest_is_valid_json_and_names_dkod_swarm() {
    let path = workspace_root().join(".claude-plugin/marketplace.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("read {} failed: {e}", path.display())
    });
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse JSON");
    assert_eq!(parsed["name"], "dkod-swarm", "marketplace name mismatch");
    let plugins = parsed["plugins"].as_array().expect("plugins array");
    assert!(
        plugins.iter().any(|p| p["name"] == "dkod-swarm"),
        "marketplace.plugins must contain a `dkod-swarm` entry"
    );
}

#[test]
fn plugin_manifest_is_valid_json_and_names_dkod_swarm() {
    let path = workspace_root().join("plugin/.claude-plugin/plugin.json");
    let text = std::fs::read_to_string(&path).expect("read plugin.json");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse JSON");
    assert_eq!(parsed["name"], "dkod-swarm");
    assert!(parsed["version"].is_string());
    assert!(parsed["description"].is_string());
}

#[test]
fn mcp_config_is_valid_json_and_declares_dkod_swarm_server() {
    let path = workspace_root().join("plugin/.mcp.json");
    let text = std::fs::read_to_string(&path).expect("read .mcp.json");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse JSON");
    let servers = parsed["mcpServers"].as_object().expect("mcpServers map");
    assert!(
        servers.contains_key("dkod-swarm"),
        "mcpServers must declare a `dkod-swarm` entry"
    );
}
```

- [ ] **Step 2: Run — expected pass.**

```sh
cargo test -p dkod-cli --test plugin_layout
```

All three tests `ok`.

- [ ] **Step 3: Run `/coderabbit:code-review` on the local diff.**

The test file is Rust source so the docs-only exemption does not apply.

- [ ] **Step 4: Commit.**

```sh
git add crates/dkod-cli/tests/plugin_layout.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add structural validation tests for plugin layout"
```

## PR M4-1 wrap-up

- [ ] `/coderabbit:code-review` clean on the validation-test commit (the docs-only commits earlier skip local CR).
- [ ] `cargo test --workspace` green (the new `plugin_layout` test file passes).
- [ ] PR `M4-1: plugin scaffold + manifests + validation`. Body = summary + test-plan checklist + note that `marketplace.json`, `plugin.json`, `.mcp.json`, README, and `.gitkeep` files are docs-only.
- [ ] Arm CodeRabbit poller. Iterate `/coderabbit:autofix` until clean. Merge autonomously.

---

# PR M4-2 — Plugin skill

## Task 5: Replace the skill placeholder with the SKILL.md instruction manual

**Files:**
- Delete: `plugin/skills/dkod-swarm/.gitkeep`
- Create: `plugin/skills/dkod-swarm/SKILL.md`

This is the substantive content of M4 — the document Claude reads to understand how to drive the 8 MCP tools.

- [ ] **Step 1: Write `plugin/skills/dkod-swarm/SKILL.md`.**

```markdown
---
name: dkod-swarm
description: >
  Use this skill when the user gives Claude a code task that touches more than one
  symbol — refactors that span multiple functions, multi-step rewrites, or any
  task where parallelism across symbols would speed up the work. The skill drives
  the dkod-swarm MCP server's 8 tools (`dkod_plan`, `dkod_execute_begin`,
  `dkod_write_symbol`, `dkod_execute_complete`, `dkod_commit`, `dkod_pr`,
  `dkod_status`, `dkod_abort`) to partition the task by call-graph coupling, run
  N Task subagents in parallel against a single shared worktree, and produce one
  commit per group plus one GitHub PR. Trigger when (a) the user asks for a
  refactor that names two or more symbols, (b) the user explicitly asks for
  "parallel" or "multi-agent" work, (c) a task description contains "and"
  joining independently-named pieces of code, or (d) the user invokes one of
  the `/dkod-swarm:` slash commands. Skip when the task is a single-line edit
  or single-symbol rewrite — the partitioning overhead isn't worth it.
compatibility: >
  Requires the dkod-swarm MCP server (shipped via this same plugin). The 8
  tools must all be available; if any is missing, the plugin install was
  incomplete and the user needs to reinstall (`/plugin install dkod-swarm@dkod-swarm`).
---

# dkod-swarm — Parallel agent orchestration over one git worktree

## What this skill does

Most "let agents work in parallel" approaches fail one of two ways:

- **File-level partition** — agent A owns `auth.rs`, agent B owns `db.rs`. Safe but limits parallelism: real refactors touch overlapping files, so 10 candidate agents collapse to 2-3.
- **Free-for-all writes** — agents edit anything; merge at the end via Git's text-level conflict resolver. Conflicts are routine and often produce broken code.

dkod-swarm partitions at the **symbol** level — function, struct, method — not the file level. Two agents rewrite different functions in the same `auth.rs` simultaneously. AST-aware writes through `dkod_write_symbol` compose their edits in-place. The partition comes from the call graph, so coupled symbols stay in the same group — correctness preserved, parallelism unlocked.

**One worktree. N Task subagents. One commit per group on one dk-branch. One PR.**

## When to use this skill

Trigger on:

1. The user asks for a refactor that names ≥ 2 symbols ("rewrite `login` and `logout` to use passkeys").
2. The task description joins independent operations ("add validation to `parse_request` AND fix the off-by-one in `iter_chunks`").
3. The user invokes `/dkod-swarm:plan`, `/dkod-swarm:execute`, or `/dkod-swarm:pr`.
4. The user explicitly says "in parallel" or "use multiple agents".

**Skip** when:

- The task is a single-line edit, single-symbol rewrite, or a question about the codebase. Partitioning overhead isn't worth it.
- The repo has not been initialised with `dkod init` (no `.dkod/config.toml`). Tell the user to run `dkod init` first.

## The flow — six phases

### Phase 0: Verify environment

Before any tool call, check that the dkod-swarm MCP tools are available. Look for `dkod_plan` in the tool listing. If absent, the plugin's MCP server didn't start — tell the user:

> The dkod-swarm MCP server isn't running. Try `/plugin install dkod-swarm@dkod-swarm` to reinstall, or check `~/.claude/logs/` for stderr from the `dkod --mcp` invocation.

### Phase 1: Plan

Call `dkod_plan` with:

- `task_prompt` — the user's task description verbatim
- `in_scope` — qualified names of every symbol you think the task will touch. Use code search and the user's task description to identify these. Err on the side of inclusion; the partitioner will tell you if any name is unknown.
- `files` — Rust source files containing those symbols, relative to the repo root
- `target_groups` — your best guess at the parallelism degree (2–4 is typical; never above 8)

The response is a `PlanResponse { groups: Vec<PlanGroup>, warnings, unresolved_edges }`. Each `PlanGroup` is a connected component of the call graph among the in-scope symbols.

**Review the partition.** If `groups.len()` is 1, the symbols are too coupled to parallelise — fall back to single-agent execution. If `groups.len() >= 2`, proceed. If `warnings` mentions `ScopeSymbolUnknown`, ask the user which symbol they meant.

Show the partition to the user as a brief table (group id, symbol count, sample names). Wait for confirmation before Phase 2 unless the user invoked `/dkod-swarm:execute` (which auto-confirms).

### Phase 2: Execute begin

Call `dkod_execute_begin(task_prompt, groups)`. The response includes `session_id` and `dk_branch` (`dk/<session-id>`).

The MCP server has now:
- Created `dk/<session-id>` off `main`
- Written a manifest under `.dkod/sessions/<sid>/`
- Set `active_session` so subsequent tool calls know which session they're in

### Phase 3: Spawn parallel Task subagents

For each group in the partition, dispatch a Task subagent. Use the `parallel-executor` subagent template (this plugin's `agents/parallel-executor.md`) or the generic Agent tool.

**Each subagent's prompt MUST include:**

1. The full set of symbol qualified names in its group
2. The agent's specific task — derived from the user's overall task by scoping to those symbols
3. Explicit instruction: **"For every file listed in your symbol set, you MUST use `dkod_write_symbol(group_id, file, qualified_name, new_body)` for the edit. DO NOT use raw `Edit` or `Write` on those files. Raw `Edit` / `Write` is only acceptable for files that are NOT in any partition group (e.g., a genuinely new file you're creating)."**
4. The `group_id` from the partition response — the subagent passes it to `dkod_write_symbol` and to the final `dkod_execute_complete` call.
5. Instruction to call `dkod_execute_complete(group_id, summary)` when done, where `summary` is a one-paragraph description of what changed.

**Why this enforcement matters:** raw `Edit` bypasses the per-file lock and the AST-merge primitive. Two subagents using raw `Edit` on the same file will produce a Git conflict at commit time — exactly what dkod-swarm exists to prevent. Always include the enforcement clause.

Wait for every Task subagent to return before Phase 4. The Task tool's natural synchronization handles this — no polling needed.

### Phase 4: Verify subagent reports

For each subagent that returned, confirm:
- Status was DONE (not BLOCKED, NEEDS_CONTEXT, or DONE_WITH_CONCERNS without good cause)
- The subagent called `dkod_execute_complete` for its group

Optionally call `dkod_status` to see per-group write counts and confirm everything is in `done` state.

### Phase 5: Commit

Call `dkod_commit()`. The response is `CommitResponse { commits_created, dk_branch, commit_shas }`. One commit per group with writes; groups that produced no writes are silently skipped.

The dk-branch now has all the work. The session manifest is marked `Committed`.

### Phase 6: Review and PR

Briefly review the diff (`git diff main..<dk_branch>`) — look for obvious bugs, missing tests, or scope drift. If anything is off, dispatch a fix-up subagent or call `dkod_abort` to discard the session.

When satisfied, call `dkod_pr(title, body)`:

- `title` — concise, ≤ 70 chars, summarises the user's task
- `body` — short summary + test-plan checklist (mirror the project's PR-body convention if visible in `git log`)

The response is `PrResponse { url, was_existing }`. Show the URL to the user. `was_existing == true` means a PR for `dk/<sid>` already existed and we returned its URL — that's idempotency, not an error.

After a successful `dkod_pr`, the active session is cleared. The user may now run `dkod_plan` again for a new task.

## Failure modes — what to do

### Partition produces only 1 group

The in-scope symbols are too coupled. Either:
- Fall back to single-agent (you, Claude main) sequential execution
- Ask the user to broaden scope so unrelated parts can parallelise

### A subagent returns BLOCKED

Check its escalation message. Common causes:
- Symbol name was wrong (the partition included a typo) → call `dkod_abort`, fix the in_scope list, re-plan
- Compile error from a parse-failed write → check `dkod_status` for `fallback` outcomes; the symbol may need a different replacement strategy

### `dkod_pr` returns `Error::VerifyFailed`

Your `verify_cmd` (configured in `.dkod/config.toml` at `dkod init` time) failed. Read the error's `tail` field — it's the last 10 lines of stderr from the verify command. Dispatch a fix-up subagent on the dk-branch, then retry `dkod_pr`.

### `dkod_pr` returns an existing URL with `was_existing: true`

This is normal idempotency — a previous run already opened the PR. Just show the URL to the user.

### Mid-flight crash / context exhaustion

When a fresh Claude session starts in the same repo, the dkod-swarm MCP server's `recover()` reads `.dkod/sessions/` and restores any session in `Executing` state. Call `dkod_status` to see what was done; either resume from there, or call `dkod_abort` to discard.

## Hard rules — never violate

1. **Never call raw `Edit` or `Write` on a file in any partition group.** Use `dkod_write_symbol` exclusively for those. Raw `Edit` / `Write` is acceptable only for files outside every group's symbol set (e.g., a brand-new file).
2. **Never merge the dk-branch yourself.** `dkod_pr` opens the PR; the user (or CI) merges it. Calling `git merge` directly bypasses the project's review gate.
3. **Never modify `main`.** Every write happens on `dk/<sid>`; `main` is read-only from this skill's perspective.
4. **Never call `dkod_abort` without explicit user confirmation** unless verification has failed unrecoverably (e.g., the dk-branch is in a state that cannot be fixed by a fix-up subagent).
5. **Never commit secrets.** The skill operates inside a real working tree; the same hygiene applies as to any user-driven commit.

## Quick reference — the 8 tools

| Tool | Caller | Purpose |
|------|--------|---------|
| `dkod_plan` | parent Claude | partition the task by call-graph coupling |
| `dkod_execute_begin` | parent Claude | mint session + dk-branch |
| `dkod_write_symbol` | Task subagent | AST-level symbol replacement (per-file lock) |
| `dkod_execute_complete` | Task subagent | mark group done with summary |
| `dkod_commit` | parent Claude | one commit per group on dk-branch |
| `dkod_pr` | parent Claude | verify, push, open PR (idempotent) |
| `dkod_status` | parent Claude or user | read-only session snapshot |
| `dkod_abort` | parent Claude or user | destroy dk-branch + clear state |
```

(That is the full body. If reformatting in the editor adjusts line wrapping, do not change the structure or the section headings — they are referenced by `commands/*.md` in PR M4-3.)

- [ ] **Step 2: Delete the placeholder.**

```sh
git rm plugin/skills/dkod-swarm/.gitkeep
```

- [ ] **Step 3: Stage + commit (docs-only, skip local CR).**

```sh
git add plugin/skills/dkod-swarm/SKILL.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add dkod-swarm SKILL.md instruction manual"
```

## Task 6: Skill validation in plugin_layout.rs

**Files:**
- Modify: `crates/dkod-cli/tests/plugin_layout.rs`

- [ ] **Step 1: Add a test that asserts SKILL.md has YAML frontmatter and a `name:` line.**

Append to `plugin_layout.rs`:

```rust
#[test]
fn skill_md_has_frontmatter_and_name_field() {
    let path = workspace_root().join("plugin/skills/dkod-swarm/SKILL.md");
    let text = std::fs::read_to_string(&path).expect("read SKILL.md");
    assert!(
        text.starts_with("---\n"),
        "SKILL.md must start with a YAML frontmatter delimiter `---`"
    );
    // Find the closing `---` and pull the frontmatter slice.
    let after_open = &text[4..];
    let close_idx = after_open
        .find("\n---")
        .expect("SKILL.md frontmatter has no closing delimiter");
    let frontmatter = &after_open[..close_idx];
    assert!(
        frontmatter.contains("name: dkod-swarm"),
        "SKILL.md frontmatter must declare `name: dkod-swarm`; got:\n{frontmatter}"
    );
    assert!(
        frontmatter.contains("description:"),
        "SKILL.md frontmatter must include a description"
    );
}
```

- [ ] **Step 2: Run — expected pass.**

```sh
cargo test -p dkod-cli --test plugin_layout
```

Four tests `ok`.

- [ ] **Step 3: Run `/coderabbit:code-review`.**

This commit IS Rust source. Local CR runs.

- [ ] **Step 4: Commit.**

```sh
git add crates/dkod-cli/tests/plugin_layout.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Validate SKILL.md frontmatter shape"
```

## PR M4-2 wrap-up

- [ ] `/coderabbit:code-review` clean on the validation-test commit.
- [ ] `cargo test --workspace` green.
- [ ] PR `M4-2: dkod-swarm plugin skill`. Body = summary + test plan + docs-only note.
- [ ] CR poller + autofix until clean. Merge autonomously.

---

# PR M4-3 — Slash commands + parallel-executor subagent

## Task 7: Slash commands

**Files:**
- Delete: `plugin/commands/.gitkeep`
- Create: `plugin/commands/plan.md`
- Create: `plugin/commands/execute.md`
- Create: `plugin/commands/pr.md`

- [ ] **Step 1: Create `plugin/commands/plan.md`.**

```markdown
---
description: Plan a multi-symbol code task — partition by call-graph coupling and present the groups for review without starting execution
---

The user wants to plan a code task using dkod-swarm but is NOT ready to execute yet. Drive the skill's Phase 1 only:

1. Read `$ARGUMENTS` (the user's task description). If empty, prompt: "What's the task? Name the symbols you want refactored or the broad goal."
2. Use code search to identify in-scope symbols + their files. Build the `dkod_plan` arguments.
3. Call `dkod_plan(task_prompt, in_scope, files, target_groups)`.
4. Present the partition as a markdown table (group id | symbol count | sample names) plus the warnings list.
5. STOP. Do NOT call `dkod_execute_begin`. Tell the user: "Run `/dkod-swarm:execute` to start, or refine scope and re-plan."

If the partition has only 1 group, tell the user the symbols are too coupled for parallel execution and recommend single-agent work.
```

- [ ] **Step 2: Create `plugin/commands/execute.md`.**

```markdown
---
description: Drive the full dkod-swarm flow end-to-end — plan, execute_begin, parallel write_symbol via Task subagents, commit, and stop just before pr
---

The user wants to run a multi-symbol code task end-to-end. Drive Phases 1–5 of the dkod-swarm skill:

1. Phase 1 — Plan: read `$ARGUMENTS` as the task description; call `dkod_plan`. If `groups.len() == 1`, fall back to single-agent execution and tell the user.
2. Phase 2 — Execute begin: call `dkod_execute_begin(task_prompt, groups)`.
3. Phase 3 — Spawn parallel Task subagents using the `parallel-executor` subagent template (this plugin's `agents/parallel-executor.md`). Each subagent owns one group; pass the group_id and symbol list verbatim. Include the hard rule: "use `dkod_write_symbol` for every edit on a partition-group file; raw `Edit` / `Write` is forbidden for those files."
4. Phase 4 — Wait for every subagent to return DONE. Call `dkod_status` to confirm.
5. Phase 5 — Commit: call `dkod_commit`. Show the commit count + SHAs to the user.
6. STOP after commit. Tell the user: "Run `/dkod-swarm:pr <title>` when you're ready to push."

Do NOT call `dkod_pr` from this command. The user finalises with a separate slash command (so they have a chance to review the diff first).
```

- [ ] **Step 3: Create `plugin/commands/pr.md`.**

```markdown
---
description: Finalize a dkod-swarm session — run verify_cmd, push the dk-branch with --force-with-lease, open a PR via gh (idempotent)
---

The user wants to push the current dk-branch and open a PR. Drive Phase 6:

1. Read `$ARGUMENTS` as the PR title. If empty, prompt: "What should the PR title be? (≤ 70 chars)"
2. Generate a short PR body — one-paragraph summary derived from the dk-branch's commit messages, plus a test-plan checklist.
3. Call `dkod_pr(title, body)`.
4. If the response has `was_existing: true`, tell the user: "PR already exists at `<url>`."
5. Otherwise, tell the user: "PR opened at `<url>`."

If `dkod_pr` returns `Error::VerifyFailed`, show the error tail and ask the user how to proceed (dispatch fix-up subagent, abort, or retry after manual fix). Do NOT silently retry.
```

- [ ] **Step 4: Branch + commit (docs-only, skip local CR).**

```sh
git checkout -b m4/plugin-commands-agents
git rm plugin/commands/.gitkeep
git add plugin/commands/plan.md plugin/commands/execute.md plugin/commands/pr.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add /dkod-swarm: slash commands"
```

## Task 8: parallel-executor subagent

**Files:**
- Delete: `plugin/agents/.gitkeep`
- Create: `plugin/agents/parallel-executor.md`

- [ ] **Step 1: Create `plugin/agents/parallel-executor.md`.**

```markdown
---
name: parallel-executor
description: Subagent that owns ONE partition group of a dkod-swarm session. Edits every symbol in its assignment via `dkod_write_symbol`, then calls `dkod_execute_complete`. Spawned by the parent Claude during the skill's Phase 3.
model: sonnet
---

You are a parallel executor for a dkod-swarm session. The parent Claude has already partitioned the user's task; you own ONE group and must rewrite every symbol assigned to you.

## What you receive

In your prompt the parent will include:

- `group_id` — your group's identifier (e.g. `g1`, `g2`)
- A list of qualified symbol names you own
- A specific task scoped to those symbols
- The session id (informational; you don't pass it explicitly — the MCP server infers it from the active session)

## Your loop

For each symbol in your assignment:

1. Read the current source via `Read` (or `dkod_file_read` if available).
2. Compute the new symbol body — full replacement item (signature + body + braces, not just the inner block).
3. Call `dkod_write_symbol(group_id, file, qualified_name, new_body)`. The response includes an `outcome` field — `parsed_ok` is the happy path; `fallback` means the AST-replace path triggered (still wrote the file, but tree-sitter couldn't re-verify) — note in your summary if any write fell back.

When all your symbols are written:

4. Call `dkod_execute_complete(group_id, summary)` where `summary` is a one-paragraph description of what you changed.
5. Return DONE to the parent.

## Hard rules

1. **Use `dkod_write_symbol` for every file in your symbol set.** Raw `Edit` / `Write` on a partition-group file would bypass the per-file lock and the AST-merge primitive — a parallel executor doing that breaks the whole protocol. Raw `Edit` is acceptable ONLY for files NOT in any partition group (e.g., a brand-new file you create as part of the rewrite).
2. **Never call `dkod_commit`, `dkod_pr`, or `dkod_abort`.** Those are the parent's responsibility. You do `dkod_write_symbol` + `dkod_execute_complete`, nothing else.
3. **Never read or modify `.dkod/sessions/`.** The MCP server owns that state.
4. **If you can't complete a symbol** (e.g., the new body fails to compile), report DONE_WITH_CONCERNS to the parent. Don't silently leave the rewrite half-done.
5. **Stay inside your assignment.** If you discover a related symbol that should also change, return DONE_WITH_CONCERNS suggesting it; don't rewrite outside your group.

## Why your scope is so narrow

Two parallel-executor subagents are running concurrently against the same worktree. The whole point is that you don't step on each other. The partition guarantees you don't share symbols; the per-file lock around `dkod_write_symbol` guarantees you don't race on file writes. Stay in lane.
```

- [ ] **Step 2: Commit (docs-only).**

```sh
git rm plugin/agents/.gitkeep
git add plugin/agents/parallel-executor.md
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Add parallel-executor subagent definition"
```

## Task 9: Validation tests for commands + agent + cleanup

**Files:**
- Modify: `crates/dkod-cli/tests/plugin_layout.rs`

- [ ] **Step 1: Add tests that assert each command + agent file has YAML frontmatter and a `description:` line (or `name:` for the agent).**

Append:

```rust
fn assert_md_frontmatter_has(path: &std::path::Path, required_keys: &[&str]) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    assert!(
        text.starts_with("---\n"),
        "{} must start with `---` frontmatter delimiter",
        path.display()
    );
    let after_open = &text[4..];
    let close_idx = after_open
        .find("\n---")
        .unwrap_or_else(|| panic!("{} has no closing `---`", path.display()));
    let frontmatter = &after_open[..close_idx];
    for key in required_keys {
        assert!(
            frontmatter.contains(key),
            "{} frontmatter missing `{key}`; got:\n{frontmatter}",
            path.display()
        );
    }
}

#[test]
fn slash_command_files_have_description_frontmatter() {
    let dir = workspace_root().join("plugin/commands");
    for name in ["plan.md", "execute.md", "pr.md"] {
        assert_md_frontmatter_has(&dir.join(name), &["description:"]);
    }
}

#[test]
fn parallel_executor_agent_has_required_frontmatter() {
    let path = workspace_root().join("plugin/agents/parallel-executor.md");
    assert_md_frontmatter_has(&path, &["name: parallel-executor", "description:", "model:"]);
}
```

- [ ] **Step 2: Run — expected pass.**

```sh
cargo test -p dkod-cli --test plugin_layout
```

Six tests `ok`.

- [ ] **Step 3: Run `/coderabbit:code-review`.**

- [ ] **Step 4: Commit.**

```sh
git add crates/dkod-cli/tests/plugin_layout.rs
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git commit -m "Validate command + agent frontmatter shapes"
```

## PR M4-3 wrap-up

- [ ] `/coderabbit:code-review` clean on the validation-test commit.
- [ ] `cargo test --workspace` green.
- [ ] PR `M4-3: slash commands + parallel-executor subagent`. Merge autonomously.
- [ ] After merge, controller tags `v0.4.0-m4` on `main`:

```sh
git checkout main && git pull
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
git tag -a v0.4.0-m4 -m "Milestone 4: plugin manifest + skill + slash commands"
git push origin v0.4.0-m4
```

---

## Milestone 4 exit criteria

1. `cargo test --workspace` green across all 3 PRs merged to `main`. The new `plugin_layout` test file passes (6 tests).
2. The `plugin/` directory contains a manifest, .mcp.json, README, the `dkod-swarm` skill, three slash commands, and the parallel-executor subagent — all with valid JSON / YAML frontmatter.
3. The repo-root `.claude-plugin/marketplace.json` advertises one plugin (`dkod-swarm`) with `source: "./plugin"`.
4. A user with the source tree checked out can run `/plugin marketplace add /path/to/dkod-swarm` and `/plugin install dkod-swarm@dkod-swarm`, and Claude Code launches the MCP server via `cargo run -p dkod-cli --bin dkod -- --mcp`.
5. All commits on `main` authored AND committed by `Haim Ari <haimari1@gmail.com>` — zero `Co-Authored-By` trailers across M4 history.

## Out of scope (M5+)

- E2E smoke against a real Rust sandbox repo measuring wall-clock vs serial. **M5.**
- Marketplace publish to the Claude Code registry — replaces the `cargo run`-based `.mcp.json` with a binary-distribution model. **M6.**
- Skill iteration based on real-world driving by Claude. Once M5 runs the skill against a sandbox, expect ~1–2 follow-up PRs to refine the wording.
- Hooks, uninstall scripts, plugin-side analytics — M6+ if at all.
