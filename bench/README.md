# bench/

Benchmarking and end-to-end fixtures for dkod-swarm.

## Layout

- `sandboxes/auth/` — a 4-module Rust crate (login, logout, session,
  passkey) used by both the automated E2E tests and the manual
  driving guide. Not a workspace member; never built by
  `cargo build --workspace`.
- `MANUAL_E2E.md` — step-by-step guide for driving the auth sandbox
  end-to-end through real Claude Code with the dkod-swarm plugin.

## Automated counterparts

The automated tests live next to the rest of the workspace tests:

- `crates/dkod-mcp/tests/bench_sandbox_e2e.rs` — full plan→pr flow
  against the auth sandbox via the in-process rmcp client. PATH-shimmed
  `gh` and `git push`; no GitHub credentials touched.
- `crates/dkod-mcp/tests/bench_parallel_vs_serial.rs` — wall-clock
  benchmark asserting that three parallel writes (each carrying a
  100ms simulated LLM delay) complete > 1.5× faster than the same
  three writes done sequentially.

Run them with:

```sh
cargo test -p dkod-mcp --test bench_sandbox_e2e
cargo test -p dkod-mcp --test bench_parallel_vs_serial -- --nocapture
```

## Why a sandbox crate that isn't built by the workspace

The auth sandbox is *fixture content* — files dkod-swarm reads in
order to exercise the partitioner, the AST-merge primitive, and the
end-to-end flow. It's not part of the dkod-swarm product. Building it
on every `cargo test --workspace` run would slow the suite for zero
correctness signal.

If you want to build it standalone:

```sh
cd bench/sandboxes/auth && cargo build
```
