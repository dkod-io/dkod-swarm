# bench/

Benchmarking and end-to-end fixtures for dkod-swarm.

## Layout

- `sandboxes/auth/` — a 4-module Rust crate (login, logout, session,
  passkey) used by both the automated E2E tests and the manual
  driving guide. Not a workspace member; never built by
  `cargo build --workspace`.
- `sandboxes/parsers/` — an 8-parser Rust crate with stub bodies and
  paired `mod tests` stubs in a single `src/lib.rs`. Used as the
  starting state for the head-to-head benchmark. Not a workspace
  member; never built by `cargo build --workspace`.
- `MANUAL_E2E.md` — step-by-step guide for driving the auth sandbox
  end-to-end through real Claude Code with the dkod-swarm plugin.
- `HEAD_TO_HEAD.md` — driving guide for the dkod-swarm-vs-baseline
  head-to-head benchmark using the parsers sandbox.

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

## Why sandbox crates that aren't built by the workspace

The sandboxes are *fixture content* — files dkod-swarm reads in
order to exercise the partitioner, the AST-merge primitive, and the
end-to-end flow. They are not part of the dkod-swarm product. Building
them on every `cargo test --workspace` run would slow the suite for
zero correctness signal.

If you want to build one standalone (each command is self-contained
from the repo root — no `cd` needed):

```sh
cargo build --manifest-path bench/sandboxes/auth/Cargo.toml
cargo build --manifest-path bench/sandboxes/parsers/Cargo.toml
```
