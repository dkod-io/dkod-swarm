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
3. Explicit instruction: **"For every file listed in your symbol set, you MUST use `dkod_write_symbol(group_id, file, qualified_name, new_body)` for the edit. The `new_body` is the full replacement item including its leading `///` outer doc-comments and `#[…]` single-line outer attributes — write each prefix line exactly once. DO NOT use raw `Edit` or `Write` on those files. Raw `Edit` / `Write` is only acceptable for files that are NOT in any partition group (e.g., a genuinely new file you're creating)."**
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
