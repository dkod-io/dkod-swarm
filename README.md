<p align="center">
  <a href="https://dkod.io">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/dkod-io/dkod-engine/main/.github/assets/banner-dark.svg">
      <img alt="dkod — Agent-native code platform" src="https://raw.githubusercontent.com/dkod-io/dkod-engine/main/.github/assets/banner-dark.svg" width="100%">
    </picture>
  </a>
</p>

<p align="center">
  <b>Local-first parallel AI coding agents. N agents, same files, one PR.</b>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-06b6d4?style=flat-square&labelColor=0f0f14"></a>
  <a href="docs/design.md"><img alt="Status" src="https://img.shields.io/badge/status-design_phase-06b6d4?style=flat-square&labelColor=0f0f14"></a>
  <a href="https://code.claude.com/docs/en/plugins"><img alt="Claude Code Plugin" src="https://img.shields.io/badge/claude_code-plugin-06b6d4?style=flat-square&labelColor=0f0f14"></a>
  <a href="https://dkod.io"><img alt="Website" src="https://img.shields.io/badge/dkod.io-website-06b6d4?style=flat-square&labelColor=0f0f14"></a>
  <a href="https://discord.gg/q2xzuNDJ"><img alt="Discord" src="https://img.shields.io/badge/discord-community-06b6d4?style=flat-square&labelColor=0f0f14"></a>
  <a href="https://twitter.com/dkod_io"><img alt="Twitter" src="https://img.shields.io/badge/twitter-@dkod__io-06b6d4?style=flat-square&labelColor=0f0f14"></a>
</p>

<p align="center">
  <a href="docs/design.md">Design</a> &nbsp;&bull;&nbsp;
  <a href="#how-it-works">How It Works</a> &nbsp;&bull;&nbsp;
  <a href="#status">Status</a> &nbsp;&bull;&nbsp;
  <a href="https://discord.gg/q2xzuNDJ">Discord</a>
</p>

<br>

## Status

**v0 in flight — milestones 1, 2, 3, 4, and 5 merged.** `cargo test --workspace` is green across 8 PRs of M1, 8 of M2, 3 of M3, 3 of M4, and 3 of M5. Empirical proof of the parallel-vs-serial speedup lives in `crates/dkod-mcp/tests/bench_parallel_vs_serial.rs`; a human-driven counterpart is documented in `bench/MANUAL_E2E.md`.

The full design lives in [`docs/design.md`](docs/design.md). Milestone 6 (marketplace publish — replaces the `cargo run`-based `.mcp.json` with binary distribution) is the remaining ship item.

<br>

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

<br>

## The Problem

Running N AI coding agents in parallel usually breaks one of two ways:

- **File-level partition.** Each agent owns whole files. Safe, but limits parallelism — real tasks touch overlapping files, so you end up with 2-3 parallel agents when you could have 10.
- **Free-for-all writes.** Agents edit whatever they want. Merge-at-the-end is a conflict nightmare that often produces broken code.

**Your agents are fast. Text-level merges are holding them back.**

## The Fix

dkod-swarm partitions work at the **symbol** level — function, struct, method — not the file level. Two agents rewrite different functions in the same `auth.rs` at the same time. An AST-aware merge composes their symbol-level edits into one coherent file. The partition is computed from the call graph, so coupled symbols stay in the same group — correctness preserved, parallelism unlocked.

No server. No database. No cloud coordination. One shared worktree on your machine. N agents. One PR to your upstream.

<br>

<table>
<tr>
<td width="50%" valign="top">

### Symbol-Level Partition

Not "agent A gets `file1.rs`, agent B gets `file2.rs`." Instead: agent A gets `login` + `logout`, agent B gets `validate_credentials`. Same file. Parallel. AST-merge composes them at the end.

**One worktree. N agents. Full parallelism.**

</td>
<td width="50%" valign="top">

### Local-First, Zero Network Hops

No platform, no API, no cloud. Planner, orchestrator, and AST-merge all run in-process inside a single local binary. The only network I/O is the final `gh` push.

**Your code never leaves your machine.**

</td>
</tr>
</table>

<br>

## What's Safe in Parallel

| Scenario | Result |
|----------|--------|
| Two agents edit different functions in the same file | **Parallel** |
| Two agents add different fields to the same struct | **Parallel** (AST-merge composes) |
| Two agents add the same import | **Deduplicated** |
| Caller and callee both change with a new signature | **Partitioned together** (planner keeps coupled symbols in one group) |
| Agent A deletes a function Agent B calls | **Conflict** (partition bug, surfaced pre-merge) |

<br>

## How It Works

1. **Plan** — dkod-swarm reads the call graph, partitions the task into N independent symbol groups (`dkod_plan`).
2. **Execute** — Claude Code spawns N Task subagents, each working a group in the same shared worktree (`dkod_execute_begin`).
3. **Write** — agents write through `dkod_write_symbol`, an AST-aware tool that holds a brief per-file lock and replaces symbols at the AST level.
4. **Finalize** — one commit per agent on the dk-branch. No separate merge step — the worktree is already in merged state (`dkod_commit`).
5. **Review & Ship** — Claude reviews the diff via a reviewer subagent, runs your verify command, pushes one dk-branch, opens one PR via `gh` (`dkod_pr`).

Two agents editing different functions in the same file? **Composed at the AST level, in-place.** Same import added twice? **Deduplicated.** True semantic conflict? **Caught before the PR, surfaced with context.**

<br>

## MVP Scope (v0)

```
Languages:  Rust only (tree-sitter-rust + codanna-style symbol extraction)
Parallelism: up to ~4 groups per task
MCP tools:  dkod_plan, dkod_execute_begin, dkod_write_symbol, dkod_execute_complete,
            dkod_commit, dkod_pr, dkod_status, dkod_abort
CLI:        dkod init, dkod status, dkod abort, dkod --mcp
Plugin:     Claude Code plugin + skill
Verify:     configured command, runs once pre-PR
```

Deferred to later waves: multi-language support, a desktop Studio for live visualization of parallel agents on a worktree, AST-aware rebase when main moves. Full scope in [`docs/design.md`](docs/design.md).

<br>

## Relationship to dkod

dkod-swarm is a **new product** under the dkod brand. It does **not** share code, protocol, or state with the hosted dkod platform — it's a local-first variant, built greenfield. The two products coexist; users of one do not migrate to the other.

The open-source [dkod-engine](https://github.com/dkod-io/dkod-engine) provides the AST-merge primitives that both products build on.

<br>

## Community

<p align="center">
  <a href="https://discord.gg/q2xzuNDJ"><img src="https://img.shields.io/badge/Discord-Join_the_community-06b6d4?style=for-the-badge&labelColor=0f0f14" alt="Discord"></a>
  &nbsp;&nbsp;
  <a href="https://twitter.com/dkod_io"><img src="https://img.shields.io/badge/Twitter-Follow_@dkod__io-06b6d4?style=for-the-badge&labelColor=0f0f14" alt="Twitter"></a>
</p>

<br>

## License

MIT — free to use, fork, and build on.

<br>

<p align="center">
  <sub>Built for the age of agent-native development &bull; <a href="https://dkod.io">dkod.io</a></sub>
</p>
