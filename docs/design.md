# Design: dkod-swarm

**Date:** 2026-04-24
**Status:** Design approved; implementation plan and 3 open questions pending (see end of doc)

---

## Problem

The current hosted dkod stack (dk-platform, dk-mind, dk-server, apps/web, K8s infrastructure) was built to support multi-user/multi-session parallel work against a shared upstream repo. In practice:

- Every MCP round-trip incurs a network hop: agent → dk-mcp → gRPC → platform → Postgres/Qdrant/NATS → back.
- The network overhead of the managed pipeline *cancels out* the AST-merge speedup that is supposed to be dkod's selling point.
- Multi-tenancy is a feature we don't yet monetize and that very few real users actually use — we built coordination machinery for a usage pattern the product doesn't have.

The pitch we actually want — *"N agents editing the same files in parallel without merge hell, fast"* — requires the opposite architecture: zero network hops, zero coordination server, everything local.

This design specifies a **new** product that ships alongside the existing hosted stack. **The existing code is not modified.** If the new product succeeds, the hosted stack can be wound down later; if it fails, nothing was risked.

---

## Decisions (from brainstorming, 2026-04-24)

| # | Question | Decision |
|---|----------|----------|
| 1 | Motivation for the pivot | All of: ops/cost reduction, product focus, distribution. Core realization: the platform is *defeating* the speed it was meant to sell. |
| 2 | Planning intelligence | Deterministic symbol-graph partitioning via tree-sitter + codanna-style symbol extraction. **No embeddings, no vector DB, no LLM in planning path.** Claude scopes which symbols are relevant; orchestrator does the graph math. |
| 3 | Code review | Dropped as a separate feature. Claude reviews its own merged diff using the Task tool (a reviewer subagent with the diff). No `dk_review` equivalent. |
| 4 | Product shape | Plugin + local Studio desktop app. The Studio (wave 2) provides the "watch N agents land symbols live" visualization. |
| 5 | Repo/code layout | **Strict isolation** — completely new repo, separate Claude Code plugin name. Zero changes to `dkod-engine`, `dkod-platform`, `dkod-plugin`, `dkod-app`, or `apps/web`. |
| 6 | Crate granularity | `dkod-planner` and `dkod-orchestrator` merged into one crate. |
| 7 | Studio in MVP | No. CLI + plugin first; Studio is wave 2. |
| 8 | Worktree isolation per agent | No. **Single shared worktree**, AST-mediated writes via `dkod_write_symbol`. 10× disk duplication is unacceptable, especially for build artifacts. |
| 9 | Merge granularity on dk-branch | One commit per agent (preserves provenance, `git blame` works). |

---

## Topology

### Before (current hosted stack)

```text
[Claude Code] ──MCP stdio──▶ [dk-mcp in engine]
                                    │ gRPC
                                    ▼
[apps/web + dkod-app] ──HTTPS──▶ [dk-server: Platform + Mind + gRPC]
                                    │
                      ┌───────┬─────┴─────┬─────────┬────────┐
                      ▼       ▼           ▼         ▼        ▼
                 [Postgres][Qdrant][Garage S3][Valkey][NATS]
                      │
                      ▼
                   [GitHub]
```

### After (local-first)

```text
[Claude Code] ──MCP stdio──▶ [dkod-cli --mcp, local]
                                    │ in-process (Rust)
                                    ▼
                              [one git worktree]
                                    │
                                    ▼
                     [dk-branch → one PR → GitHub via `gh`]
                                    ▲
                                    │ read-only IPC (wave 2)
                            [Studio (Tauri app)]
```

Two processes (Claude Code + `dkod-cli`), one worktree, one PR. No database, no queue, no cache, no S3, no auth server, no backend API, no cluster. Studio observes; nothing depends on it being running.

---

## Repo layout (`dkod-swarm`)

```text
dkod-swarm/
├── Cargo.toml                         # workspace
├── crates/
│   ├── dkod-cli/                      # `dkod` binary; --mcp flag for stdio mode
│   ├── dkod-worktree/                 # git worktree + dk-branch lifecycle
│   ├── dkod-orchestrator/             # planner (symbol graph) + execution coordinator
│   └── dkod-mcp/                      # stdio MCP server, rmcp-based
├── plugin/
│   ├── plugin.json                    # Claude Code plugin manifest (name: dkod-swarm)
│   ├── commands/                      # /dkod:plan, /dkod:execute, /dkod:pr
│   ├── skills/
│   │   └── dkod-swarm/SKILL.md        # instructs Claude how to drive the flow
│   └── agents/                        # subagent definitions for parallel executors
└── studio/                            # wave 2 — Tauri desktop app
    ├── src-tauri/                     # Tauri backend (Rust)
    └── src/                           # React frontend (copied bits of apps/web, then forks)
```

### Crate responsibilities

- **`dkod-cli`** — user-facing binary. `dkod init`, `dkod status`, `dkod abort`, `dkod --mcp` (stdio mode launched by Claude Code).
- **`dkod-worktree`** — create/list/destroy git worktrees, manage dk-branch off `main`, track per-session state as files under `.dkod/`.
- **`dkod-orchestrator`** — combined planner + execution coordinator. Planner subset is a pure function: given in-scope symbols + call graph, produces N disjoint symbol groups. Executor subset tracks group progress, commits, and finalizes.
- **`dkod-mcp`** — stdio MCP server using `rmcp`. Exposes the tool surface.

### Plugin layer

- `plugin.json` — plugin manifest with name `dkod-swarm` (distinct from the existing `dkod` plugin).
- Skill — the instruction manual Claude reads. Tells Claude: "when a codebase task arrives, call `dkod_plan`, review the partition, spawn N Task subagents with partitions attached, wait, call `dkod_commit`, review diff, call `dkod_pr`."
- Slash commands — optional explicit entry points.

### Studio (wave 2)

- Tauri app that watches `.dkod/` via `notify`.
- Shows: worktree tree, N agent lanes writing files, live AST-merge events, conflicts, final diff.
- Read-only — never mutates. Nothing depends on it running.
- Initial UI borrows from `apps/web` by **copy** (not reference). After ship, it's a fork — stays consistent with "don't change existing code."

---

## Engine dependency assumption

The new product depends on `dkod-engine` via Cargo git dep (like `dkod-platform` does today). **No engine changes.** If the engine's public API doesn't expose codanna-style symbol extraction, we re-parse within `dkod-orchestrator` using tree-sitter directly. This is the strict reading of "don't touch existing code" — we don't petition the engine to expose new APIs.

This is a flagged assumption to verify at the start of implementation: *does `dk-core` (or another engine public crate) already expose the symbol-graph data we need?*

---

## Session lifecycle

Six phases, from user prompt to PR.

### Phase 0 — Init (one-time per repo)

User runs `dkod init` in their repo. Creates `.dkod/`, records main branch, writes `config.toml`. No server, no auth, no signup.

### Phase 1 — Task arrives

User in Claude Code: "refactor auth to use passkeys." Claude, guided by the skill, calls `dkod_plan(task)` via MCP.

### Phase 2 — Plan

Two steps:

1. **Claude scopes.** Returns a draft list of relevant symbols/files. This requires semantic understanding of the task — Claude does it. The orchestrator does *not* try to scope deterministically.
2. **Orchestrator partitions.** Given the in-scope symbols, walks the call graph (tree-sitter + codanna-style extraction), identifies disjoint subgraphs, returns N candidate groups with edge-case warnings ("groups X and Y both touch a trait — watch for signature changes").

Claude reviews the partition. Re-plans if unsatisfactory.

### Phase 3 — Execute

1. Claude calls `dkod_execute_begin(partition)`.
2. Orchestrator creates dk-branch `dk/<session-id>` in a single parent worktree. Writes session state to `.dkod/sessions/<id>/`. Returns `{group_id → group_spec}`.
3. Claude spawns N Task subagents, each with its group spec + instruction to write via `dkod_write_symbol`.
4. Subagents work in parallel **in the same worktree**. Writes go through `dkod_write_symbol(file, symbol, new_body)`:
   - Acquire brief per-file lock.
   - Parse current file via AST.
   - Locate the symbol by AST path.
   - Replace only that symbol's body.
   - Write back, release lock.
5. Each subagent ends with `dkod_execute_complete(group_id, summary)`.

**Key invariants:**
- **No two subagents write the same symbol** — enforced by the partition.
- **Files can be written by multiple subagents concurrently** — different symbols within the same file.
- **Builds and tests do NOT run during this phase** — concurrent cargo would race on `target/`.
- **Raw `Write` and `Edit` are disallowed** for partition-scoped files — all writes (including within-symbol-body tweaks) must go through `dkod_write_symbol`, which holds the per-file lock. The skill enforces this. Raw `Edit` is permitted only for files outside any partition group (e.g., genuinely new files created during the task).

### Phase 4 — Finalize

When all N subagents report done (Task tool returns naturally), Claude calls `dkod_commit()`. Orchestrator:

1. Writes one commit per group on the dk-branch (preserves authorship/provenance).
2. Proceeds to review.

There is **no big "merge" step** — the worktree is already in merged state because every write went through AST-level symbol replacement as it happened.

### Phase 5 — Review

Per decision #3, Claude reviews itself. Spawns a reviewer Task subagent with the diff vs. main + a checklist. The reviewer either approves or dispatches fix-up subagents. No separate LLM pipeline; no `dk_review`.

### Phase 6 — PR

Claude calls `dkod_pr(title, body)`. Orchestrator:

1. Runs the configured `verify_cmd` (e.g., `cargo check && cargo test --workspace`) once against the final committed state. If it fails, surfaces to Claude; no PR is created.
2. Checks GitHub for an existing PR on `dk/<session-id>` (the branch name is deterministic from the session id). If one exists, returns its URL — repeat calls are idempotent.
3. Otherwise pushes dk-branch via `gh` using `--force-with-lease` (safe if a prior call already pushed the same state; fails loud if the remote has diverged).
4. Creates the PR via `gh pr create`. On failure, re-queries for an existing PR before retrying; if one now exists, returns its URL rather than erroring.
5. Returns URL.

The orchestrator never deletes an already-pushed branch on failure; it either returns the existing PR URL or an explicit conflict error. Calling `dkod_pr` twice in the same session produces the same PR, not a duplicate.

---

## Coordination model

Three parties:
- **Parent Claude** (driver, main Claude Code session).
- **N subagents** (Task-tool children, parallel).
- **Orchestrator** (Rust service inside `dkod-cli --mcp`, reachable via stdio).

Synchronization pattern:
- Parent calls MCP tools → orchestrator responds. Pure request-response.
- Subagents are spawned via parent's Task tool. Task tool return values give the parent natural synchronization — no polling, no websockets, no daemon.
- Writes route through `dkod_write_symbol` MCP calls. Orchestrator records each write in `.dkod/sessions/<id>/groups/<id>/writes.jsonl`.

---

## MCP tool surface (v0)

Eight tools total:

| Tool | Caller | Purpose |
|------|--------|---------|
| `dkod_plan(task)` | parent Claude | returns partition |
| `dkod_execute_begin(partition)` | parent Claude | creates dk-branch, returns group specs |
| `dkod_write_symbol(file, symbol, body)` | subagent | AST-level symbol replacement with file lock |
| `dkod_execute_complete(group_id, summary)` | subagent | marks group done |
| `dkod_commit()` | parent Claude | writes one commit per group on dk-branch |
| `dkod_pr(title, body)` | parent Claude | verifies, pushes, creates PR |
| `dkod_status()` | parent Claude or CLI | reads current session state from `.dkod/` |
| `dkod_abort()` | parent Claude or CLI | destroys dk-branch + session state |

Distinct from the existing hosted plugin's `dk_*` tools — this is a separate product with separate verbs. No interop, no shared protocol, no migration path: users of the hosted product continue with `dk_*`; `dkod-swarm` users adopt `dkod_*` fresh. The two products coexist under the dkod brand but do not talk to each other.

---

## State & persistence

All session state lives in files under `.dkod/` (gitignored):

```text
.dkod/
├── config.toml                        # repo config (main branch, verify_cmd)
├── sessions/
│   └── <session-id>/
│       ├── manifest.json              # task prompt, partition, timestamps
│       ├── groups/
│       │   └── <group-id>/
│       │       ├── spec.json          # symbols, agent prompt, status
│       │       └── writes.jsonl       # append-only log of symbol writes
│       └── conflicts/                 # orchestrator-flagged issues (if any)
└── cache/
    └── symbols.db                     # optional tree-sitter symbol index (SQLite)
```

No database, no daemon, no cross-session memory. If the orchestrator or Claude Code crashes, `dkod status` re-reads state and resumes. Session bounded by "task arrives → PR pushed."

---

## Verification

- User configures `verify_cmd` in `.dkod/config.toml` (e.g., `cargo check && cargo test --workspace`).
- Runs **once**, as part of `dkod_pr`, against the final committed state. Blocks PR creation if it fails.
- No verification during the parallel phase — concurrent builds would race on `target/` and produce unreliable results.
- If verification fails, parent Claude dispatches a fix-up subagent on the same dk-branch and retries.

---

## Edge cases & failure modes

1. **Partition bug (two agents own same symbol):** second `dkod_write_symbol` errors with `ConflictError { symbol, held_by: <agent_id> }`. Subagent reports; parent re-plans. This should be rare by construction.
2. **Subagent crashes mid-work:** Task tool returns failure; `writes.jsonl` is partial. Parent retries or halts. `dkod status` shows "group X: 3/5 symbols written."
3. **Verification fails pre-PR:** surfaced; parent dispatches fix-up subagent, re-verifies, then PRs.
4. **User abandons mid-session:** `dkod abort` destroys dk-branch + session state. Main untouched.
5. **AST-merge can't handle a construct:** `dkod_write_symbol` falls back internally to a text-based replacement (still holding the per-file lock), records an `UnsupportedConstruct` warning, and succeeds. Subagents never invoke raw `Edit` on partition-scoped files — the fallback stays inside the locked write path.
6. **Parent context exhausts:** a fresh Claude session runs `dkod status`, sees what's done, resumes. `.dkod/sessions/<id>/manifest.json` is enough state.
7. **Main moves during execution:** detected on `dkod_pr`; orchestrator offers plain `git rebase` onto new main (AST-aware rebase is a future optimization, not v0).
8. **`gh` auth broken:** error verbatim to parent.

---

## MVP scope (v0)

**In:**
- Rust only (planner uses tree-sitter-rust + codanna-style extraction).
- Up to ~4 groups per partition.
- 8 MCP tools above.
- CLI: `dkod init`, `dkod status`, `dkod abort`, `dkod --mcp`.
- Plugin manifest + skill + slash commands.
- Verification via configured command, pre-PR only.
- `gh` for push/PR via user's own auth.

**Out (explicit deferrals):**
- Studio (Tauri app) — wave 2.
- Multi-language support — wave 2 (unlocked as tree-sitter grammars and symbol extraction for additional languages are built into `dkod-orchestrator`).
- AST-aware rebase — plain git rebase if main moves.
- Auto-resolving partition conflicts — v0 surfaces and waits for Claude.
- Team features, sharing, auth — **never**, by design.

## Ship order

1. `dkod-worktree` + `dkod-orchestrator` (planner, commit, state), unit-tested against fixture Rust repos.
2. `dkod-mcp` with the 8-tool surface, tested against a mock Claude Code harness.
3. `dkod-cli` wrapping (1) + (2).
4. Plugin manifest + skill + slash commands.
5. E2E smoke test: 3-symbol parallel refactor on a small Rust sandbox repo. Measure wall-clock improvement vs. serial baseline.
6. Publish plugin to Claude Code marketplace under name `dkod-swarm`.

---

## Open questions for implementation

1. **Engine API availability** — does `dk-core` (or another engine public crate) already expose the symbol-graph data the planner needs? If not, we re-parse in-repo via tree-sitter — no engine changes.
2. **Skill enforcement of `dkod_write_symbol`** — how strict should the skill be about disallowing plain `Write`? Probably: "use `dkod_write_symbol` for any file in the partition; `Write` OK for new files not in any group." Finalize during skill authoring.
3. **Per-agent commit authorship** — do we want commits to attribute to the parent Claude, to synthetic per-agent identities, or to the user? Probably the user (same as today's git behaviour). Finalize during implementation.
