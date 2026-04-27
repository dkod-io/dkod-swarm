---
description: Plan a multi-symbol code task — partition by call-graph coupling and present the groups for review without starting execution
---

The user wants to plan a code task using dkod-swarm but is NOT ready to execute yet. Drive the skill's Phase 1 only:

1. Read `$ARGUMENTS` (the user's task description). If empty, prompt: "What's the task? Name the symbols you want refactored or the broad goal."
2. Use code search to identify in-scope symbols + their files. Build the `dkod_plan` arguments.
3. Call `dkod_plan(task_prompt, in_scope, files, target_groups)`.
4. Present the partition as a markdown table (group id | symbol count | sample names) plus the warnings list.
5. STOP. Do NOT call `dkod_execute_begin`. Tell the user: "Run `/dkod-swarm:execute` to start, or refine scope and re-plan."

If the partition has only 1 group, tell the user the symbols are too coupled for parallel execution and recommend single-agent work.
