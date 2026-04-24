# dkod-swarm — session conventions

This repo is the local-first, plugin-shipped variant of dkod. Scope and positioning live in `README.md`; the authoritative spec is `docs/design.md`. Every architectural decision belongs in that doc.

## Scope boundary

- **Work happens only inside this repo.** Do not modify `dkod-engine`, `dkod-platform`, `dkod-plugin`, `dkod-app`, `apps/web`, or any other dkod repo. Private platform repos are entirely off-limits.
- **No engine API petitions.** If `dk-engine`/`dk-core` (public, on crates.io) don't expose what we need, vendor locally — do not ask the engine to expose new surfaces. Engine dependency is version-pinned via crates.io; `dk-engine::{parser, conflict::ast_merge}` + `dk-core::{Symbol, RawCallEdge, ...}` are the currently-relied-on surfaces.
- **Design drift ships first.** If implementation forces a spec revision, update `docs/design.md` in its own PR and merge it *before* the code change that depends on it.

## Git & GitHub

- **Identity:** author *and* committer must be `Haim Ari <haimari1@gmail.com>`. Never `Co-Authored-By`. Never modify local/global `git config`. Override per-commit with env vars:
  ```sh
  GIT_AUTHOR_NAME="Haim Ari" GIT_AUTHOR_EMAIL="haimari1@gmail.com" \
  GIT_COMMITTER_NAME="Haim Ari" GIT_COMMITTER_EMAIL="haimari1@gmail.com" \
  git commit -m "..."
  ```
- **GitHub account:** `haim-ari`. Run `gh auth switch --user haim-ari` if the active account drifts.
- **Branching:** every change goes through a feature branch + PR. Never push to `main` directly. No exceptions — including docs-only changes.
- **PRs:** title ≤ 70 chars. Body = short summary + test plan checklist. One logical unit per PR.

## Rust

- **Edition:** `edition = "2024"` where the toolchain supports it; `"2021"` otherwise. Pick one per crate, do not mix within a crate.
- **Workspace layout** (per design §Repo layout): crates under `crates/`, plugin assets under `plugin/`, future Studio under `studio/`.
- **Tests:** `cargo test --workspace` stays green on every PR. Planner tests are fixture-based — goldens under `crates/dkod-orchestrator/tests/fixtures/`. TDD: write the failing test first, then make it pass.

## Review gate (CodeRabbit)

Fixed workflow for every code change. Do **not** short-circuit it.

1. **Local pre-commit review.** Before committing, run `/coderabbit:code-review` against the local untracked/unstaged changes (compared to `main`). Use `/coderabbit:code-review` specifically — not `/coderabbit:review`.
2. **Fix every finding.**
3. **Re-run `/coderabbit:code-review` locally.** Iterate until the local review is clean.
4. **Only then commit and open the PR.** One logical unit per PR.
5. **Wait for CodeRabbit's PR review** to post on the PR.
6. **Run `/coderabbit:autofix`** to apply CodeRabbit's PR-side fixes. Iterate (re-review → autofix) until the PR is clean.
7. **Merge autonomously** once the PR is clean: `gh pr merge <n> --merge --delete-branch`. Do not pause for a human. During a running milestone, proceed straight to the next PR. The hard bar is zero open CodeRabbit findings, zero spec-compliance issues, and zero blocking code-quality issues. "Close enough" is never enough — if an issue cannot be resolved cleanly, STOP and ask the human rather than merge through ambiguity.

**Docs-only commits** (`.md`/`.toml`/`.yaml`-only changesets) skip CodeRabbit — it doesn't meaningfully review those. State that explicitly rather than claiming a clean review.

## License

MIT. `LICENSE` is at the repo root. No per-file license headers.

## MVP ship order

Per design §Ship order — do not skip ahead:

1. `dkod-worktree` + `dkod-orchestrator` (planner, commit, state), unit-tested. ← **Milestone 1 stops here.**
2. `dkod-mcp` with the 8-tool surface.
3. `dkod-cli` wrapping 1+2.
4. Plugin manifest + skill + slash commands.
5. E2E smoke test (3-symbol parallel refactor on a Rust sandbox, measure wall-clock vs serial).
6. Publish to Claude Code marketplace as `dkod-swarm`.

## Settled open questions (design §Open questions)

1. **Engine API** — `dk-engine` 0.3.x on crates.io exposes `parser::{LanguageParser, ParserRegistry, langs::rust::RustConfig, engine::QueryDrivenParser}` and `conflict::ast_merge`. Depend on it version-pinned. No engine changes.
2. **Skill enforcement** (M4): subagents must use `dkod_write_symbol` for any file in their partition. Raw `Edit`/`Write` is permitted only for files outside every partition group (e.g. genuinely new files). The skill authored in M4 encodes this.
3. **Commit authorship**: commits attribute to the user (normal git behaviour), not synthetic per-agent identities. Authorship is set via the env vars above for every commit the orchestrator or any agent makes.

## Stop-and-ask list

- Design-doc ambiguity — don't guess; ask.
- About to touch a repo outside `dkod-swarm` — don't; ask.
- PR ready to merge — ask before merging.
- Think the design itself is wrong — ask; update the doc together first.

## Session bootstrap checklist

When a fresh session picks this repo up:

1. Read `docs/design.md` in full, then `README.md`.
2. Read this file.
3. Check `gh auth status` — ensure active account is `haim-ari`.
4. `git status` — confirm clean working tree before starting work.
5. Match your next action to the MVP ship order. Do not skip ahead.
