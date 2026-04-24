# dkod-swarm

> N AI coding agents. Same files. Parallel. One PR.

**dkod-swarm** is a local-first Claude Code plugin + CLI that runs multiple coding agents concurrently on the same codebase. It partitions work at the AST symbol level, so agents edit different functions in the same file *simultaneously* without merge conflicts. An AST-aware merge composes their symbol-level edits into one coherent result, delivered as a single PR to your upstream.

No server. No database. No cloud coordination. Zero network hops outside the final PR push.

---

## Status

**Design phase.** No code yet. See [`docs/design.md`](docs/design.md) for the full design specification.

Watch/star this repo for updates as the implementation lands.

---

## How it works

1. You give Claude Code a task — *"refactor auth to use passkeys"*.
2. dkod-swarm partitions the work by symbol graph: N independent symbol groups with no shared call-graph edges.
3. N Task subagents run in parallel in a single shared git worktree, writing through an AST-aware tool that holds per-file locks.
4. When all subagents finish, the worktree is already merged — writes were composed at the AST level as they happened. No big "merge" step.
5. Claude reviews the diff, runs your verification command (`cargo check && cargo test`, or whatever you configure), pushes one dk-branch, opens one PR via `gh`.

The selling point: *multiple agents edit different functions in `auth.rs` at the same time, and you get one coherent file out.* Plain `git merge` can't do that. AST-merge can.

---

## Why this exists

Running N AI coding agents in parallel usually means one of two bad outcomes:

- **File-level partition.** Agents each own whole files. Safe, but limits parallelism — most real tasks touch overlapping files. You end up with 2-3 parallel agents when you could have 10.
- **Free-for-all writes.** Agents edit whatever they want. Fast in theory, but the merge at the end is a conflict nightmare that often produces broken code.

dkod-swarm partitions at the **symbol** level (function, struct, method), not the file level. Two agents can rewrite different functions in the same file at the same time. An AST-aware merge composes their symbol-level edits into one coherent file. The partition is computed from the call graph, so agents whose symbols depend on each other end up in the same group — preserving correctness without sacrificing parallelism.

---

## MVP scope

The first release (v0) targets:

- **Rust codebases only** (tree-sitter-rust + codanna-style symbol extraction).
- **Up to ~4 parallel groups per task.**
- **Eight MCP tools:** `dkod_plan`, `dkod_execute_begin`, `dkod_write_symbol`, `dkod_execute_complete`, `dkod_commit`, `dkod_pr`, `dkod_status`, `dkod_abort`.
- **A Claude Code plugin + skill** that drives the flow end-to-end.
- **A CLI** (`dkod init`, `dkod status`, `dkod abort`, `dkod --mcp`).

Deferred to later waves: multi-language support, a desktop Studio for live visualization, AST-aware rebase when main moves. See [`docs/design.md`](docs/design.md) for the full scope.

---

## Relationship to dkod

dkod-swarm is a new product under the dkod brand. It does **not** share code, protocol, or state with the hosted dkod platform — it's a local-first variant, built greenfield. The two products coexist; users of one do not migrate to the other.

The open-source engine at [dkod-io/dkod-engine](https://github.com/dkod-io/dkod-engine) provides the AST merge primitives that both products build on.

---

## License

TBD — the project is in design phase. Expected: MIT, matching `dkod-engine`.
