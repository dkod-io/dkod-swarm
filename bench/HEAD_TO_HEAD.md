# Head-to-head: dkod-swarm vs. baseline Claude Code

This guide is the human-driven companion to the M5 micro-benchmark in
`crates/dkod-mcp/tests/bench_parallel_vs_serial.rs`. The micro-benchmark
proves the orchestrator's parallel-write mechanism is faster than serial
in isolation. This guide proves the *end-to-end* product win — wall-clock,
parallel-agent count, and total tokens — against a baseline Claude Code
session that does NOT have the dkod-swarm plugin installed.

Two Claude Code sessions are run from identical starting state with the
identical prompt. The only difference is the plugin install. Each session
implements 8 string parsers and their tests in a single `src/lib.rs`,
runs `cargo test`, and opens a PR.

## What this benchmark exercises

The dkod-swarm advantage is **symbol-level parallelism in a single
shared file**. The parser sandbox is engineered to maximise that
advantage:

- 8 mutually independent parsers + 8 corresponding test stubs (16
  symbols total), all in `src/lib.rs`.
- No call graph between parsers — the partitioner produces a disjoint
  set of groups capped at the v0 maximum of ~4.
- The test stubs sit in `mod tests` *in the same file* — every group
  the partitioner picks will involve same-file concurrent writes.
- Naive parallel approaches without AST-merge will either serialise
  on the file or fight a multi-way text-level `git merge` on it.

## Setup — run identically for each session

```sh
# Pick A or B per session; do this twice (once in each terminal).
NAME=A   # change to B for the other terminal
DIR=/tmp/dkod-bench-$NAME

rm -rf "$DIR"
cp -R bench/sandboxes/parsers "$DIR"
cd "$DIR"

git init -q -b main
git add .
GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
  git commit -q -m "scaffold text_parsers"

gh repo create "haim-ari/dkod-bench-$NAME" \
  --private --source=. --remote=origin --push \
  -d "dkod-swarm head-to-head bench"
```

The two sessions must start from byte-identical scaffolds. Verify with:

```sh
diff -r /tmp/dkod-bench-A /tmp/dkod-bench-B
```

(Modulo `.git/`, the trees should be identical.)

The dkod-swarm side has the plugin installed; the baseline side does
not. Do not tell either Claude session about dkod-swarm — the prompt
makes no mention of it.

## The prompt — paste verbatim into both sessions

Open Claude Code in `/tmp/dkod-bench-A` (dkod-swarm-enabled) and
`/tmp/dkod-bench-B` (baseline). Paste this prompt into both:

> You are in a fresh Rust library crate `text-parsers-sandbox` at the
> current working directory.
>
> Starting state:
> - `Cargo.toml` — edition = "2024", no dependencies.
> - `src/lib.rs` — 8 public parser stubs (bodies = `unimplemented!()`)
>   plus a `mod tests` block containing 8 `#[test] fn test_<name>() {
>   unimplemented!() }` stubs (one per parser). Each parser has a
>   doc-comment spec; the `mod tests` doc-comment lists the 4 assertion
>   classes every test must contain.
> - `git`: on `main` with one initial commit. `origin` is a private
>   GitHub repo you can push to via `gh`.
>
> Task — produce ONE PR:
>
> 1. Replace every `unimplemented!()` body in `src/lib.rs` so each
>    parser satisfies its doc-comment spec, and each test contains
>    exactly four assertions: (a) one valid input, (b) one valid edge
>    case, (c) one invalid input, (d) one explicit example from the
>    parser's doc-comment.
> 2. `cargo test` must pass.
> 3. Use ONLY `std`. Do not edit `Cargo.toml` to add dependencies.
> 4. Do NOT change function signatures, doc-comments, or test names.
> 5. Do NOT add new files or new modules. Everything stays in `src/lib.rs`.
> 6. Open a PR titled `Implement text-parsers` via `gh pr create`. The
>    body lists each parser with one sentence on its implementation and
>    one sentence on what its tests cover.
>
> Performance directive — minimise wall-clock time.
>
> The 8 parsers are mutually independent — none calls another, none
> shares helpers. There is no reason to implement them serially. Use
> whatever parallel-execution mechanism you have available (the `Task`
> tool, plus any installed plugins) and spawn as many parallel agents
> as the structure of the task allows.
>
> Falling back to a single sequential agent defeats the purpose of this
> benchmark. Only do so if the parallel approach is genuinely blocked,
> and explain why in the report.
>
> At the end, print this BENCH_REPORT block exactly:
>
> ```
> BENCH_REPORT
> ============
> PR URL:                 <url>
> Subagents spawned:      <integer count of Task subagents you ran in parallel>
> Conflicts/retries hit:  <integer — count of times a parallel write or merge had to be redone>
> Approach summary:       <one paragraph: how you partitioned the 8 parsers
>                          across agents, what tooling you used, and any
>                          per-file write coordination>
> ```
>
> Do not ask clarifying questions — proceed with reasonable assumptions
> and document any in the PR body.

## Measurement protocol

Per session, record:

- **Wall-clock** — stopwatch from prompt-paste to PR-URL printed in
  the `BENCH_REPORT`.
- **Subagents spawned** — read from `BENCH_REPORT`.
- **Conflicts / retries** — read from `BENCH_REPORT` (corroborate by
  scanning the session log for repeated edits to `src/lib.rs`).
- **Tokens** — sum `usage.total_tokens` across assistant turns in
  `~/.claude/projects/<mapped-cwd>/<session-id>.jsonl`. The Claude
  Code `/cost` command works too where available.
- **Diff sanity** — run `gh pr diff <n>` on both PRs. If one is
  missing parsers, missing tests, or obviously buggier than the other,
  the comparison is invalid; re-run.

## Predicted outcomes — and where they may not hold

| Outcome | Likelihood | Why |
|---------|------------|-----|
| dkod-swarm uses **more parallel subagents** | Very high | The skill auto-fires on multi-symbol tasks and `dkod_plan` mints up to 4 disjoint groups. The baseline must reason its way to parallelism. |
| dkod-swarm has **shorter wall-clock** | High | True concurrent same-file writes vs. either serial work or text-merge thrash. The M5-2 micro-benchmark already shows >1.5× on the mechanism. |
| dkod-swarm uses **fewer total tokens** | Plausible — not guaranteed | If the baseline parallelises and hits text-merge conflicts, retry rounds cost real tokens and dkod-swarm wins. If the baseline plays it safe and goes serial, it may use *fewer* tokens (one big context vs. driver+subagents) at the cost of a worse wall-clock. The "minimise wall-clock" directive nudges the baseline toward parallelism. |

## Failure modes that would invalidate the test

1. **Baseline goes single-shot and writes everything sequentially.**
   Then there is no parallelism to compare. Mitigation: harden the
   "minimise wall-clock" directive in the prompt, or upgrade the
   parsers to heavier ones (e.g. RFC-light email, full URL).
2. **dkod-swarm partitioner returns 1 group.** With 8 disjoint stubs
   and `target_groups = 4` it should not, but worth confirming with a
   smoke test before the head-to-head.

   First, build the dkod CLI from the dkod-swarm repo root if you
   have not already:

   ```sh
   cargo build --release -p dkod-cli --bin dkod
   ```

   Then in `/tmp/dkod-bench-A` (or any one of the two sandbox dirs):

   ```sh
   DKOD=/abs/path/to/dkod-swarm/target/release/dkod   # adjust to your checkout
   "$DKOD" init --verify-cmd "cargo test"
   ```

   Then in Claude Code (still in the same dir):

   ```text
   /dkod-swarm:plan Implement parsers + tests in src/lib.rs
   ```

   Confirm the partition has at least 3 groups before running the real
   benchmark.

## Cleanup

```sh
rm -rf /tmp/dkod-bench-A /tmp/dkod-bench-B
gh repo delete haim-ari/dkod-bench-A --yes
gh repo delete haim-ari/dkod-bench-B --yes
```

This guide is intentionally not run in CI — it requires two Claude Code
sessions and real LLM round-trips. Treat it as the human-in-the-loop
counterpart to `crates/dkod-mcp/tests/bench_parallel_vs_serial.rs`.
