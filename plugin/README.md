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
