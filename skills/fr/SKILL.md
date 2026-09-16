---
name: fr
description: Use fr for bounded inspection, structural edits, history, Git patches, checks, and Lean evidence.
---
# Work with fr

Run at the root or pass `-C`. Use `fr audit` when support is unclear. For a task, run
`fr guide --from GOAL` or `FrClient.guide(AgentGoal(...))`; follow ready actions and their reference.
Read [Guide](references/guide.md) for goal authoring. For direct discovery:
```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```
Read coverage, omissions and statuses. Missing or clipped rows do not prove absence. Retain bases;
stale identities refuse.

Load the guide's returned reference only when needed. Direct routes: [Explore](references/explore.md),
[Runtime](references/runtime.md), [Intents](references/intents.md), [Author](references/author.md),
[Workflow](references/workflow.md), [Checks](references/checks.md), [History](references/history.md),
[Git](references/git.md), or [Lean](references/lean.md).

Author with handles. With targets and checks, use one reviewed
task-change session for checks, apply, reversal and patch. Preview first. Preserve
refusals, gaps and uncertainty.
