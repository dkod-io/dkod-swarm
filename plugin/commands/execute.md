---
description: Drive the full dkod-swarm flow end-to-end — plan, execute_begin, parallel write_symbol via Task subagents, commit, and stop just before pr
---

The user wants to run a multi-symbol code task end-to-end. Drive Phases 1–5 of the dkod-swarm skill:

1. Phase 1 — Plan: read `$ARGUMENTS` as the task description; call `dkod_plan`. If `groups.len() == 1`, fall back to single-agent execution and tell the user.
2. Phase 2 — Execute begin: call `dkod_execute_begin(task_prompt, groups)`.
3. Phase 3 — Spawn parallel Task subagents using the `parallel-executor` subagent template (this plugin's `agents/parallel-executor.md`). Each subagent owns one group; pass the group_id and symbol list verbatim. Include the hard rule: "use `dkod_write_symbol` for every edit on a partition-group file; raw `Edit` / `Write` is forbidden for those files."
4. Phase 4 — Wait for every subagent result.
   - If every subagent returned DONE, call `dkod_status` to confirm and proceed to Phase 5.
   - If any subagent returned DONE_WITH_CONCERNS, summarize the concerns to the user and STOP — do NOT call `dkod_commit`. The user decides whether to retry the affected group, re-scope, manually fix, or `dkod_abort`.
   - If any subagent returned BLOCKED or NEEDS_CONTEXT, surface that to the user and STOP. Same options as DONE_WITH_CONCERNS.
5. Phase 5 — Commit (only on the all-DONE path): call `dkod_commit`. Show the commit count + SHAs to the user.
6. STOP after commit. Tell the user: "Run `/dkod-swarm:pr <title>` when you're ready to push."

Do NOT call `dkod_pr` from this command. The user finalises with a separate slash command (so they have a chance to review the diff first).
