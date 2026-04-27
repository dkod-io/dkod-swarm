---
description: Finalize a dkod-swarm session — run verify_cmd, push the dk-branch with --force-with-lease, open a PR via gh (idempotent)
---

The user wants to push the current dk-branch and open a PR. Drive Phase 6:

1. Read `$ARGUMENTS` as the PR title. If empty, prompt: "What should the PR title be? (≤ 70 chars)". If non-empty and longer than 70 characters, ask the user to shorten it before proceeding — `dkod_pr` does not enforce the limit, so the gate lives here.
2. Generate a short PR body — one-paragraph summary derived from the dk-branch's commit messages, plus a test-plan checklist.
3. Call `dkod_pr(title, body)`.
4. If the response has `was_existing: true`, tell the user: "PR already exists at `<url>`."
5. Otherwise, tell the user: "PR opened at `<url>`."

If `dkod_pr` returns `Error::VerifyFailed`, show the error tail and ask the user how to proceed (dispatch fix-up subagent, abort, or retry after manual fix). Do NOT silently retry.
