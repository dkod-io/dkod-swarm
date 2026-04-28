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
   # Configure a local git identity so the seed commit doesn't abort
   # on machines without a global user.name/user.email.
   git config user.name "dkod-swarm sandbox"
   git config user.email "sandbox@example.invalid"
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
