---
name: fr
description: Use fr for bounded inspection, structural edits, history, Git patches, checks, and Lean evidence.
---
# Work with fr

Run at the root or pass `-C`. Request source when needed:
```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```
Read coverage, omissions and statuses. Missing or clipped rows do not prove absence. Retain bases;
stale identities refuse.

Load only the needed route: [Task](references/task.md), [Explore](references/explore.md),
[Surfaces](references/surfaces.md),
[Disclosure](references/disclosure.md), [Semantic](references/semantic.md), [Semantic intent](references/semantic-intent.md),
[Semantic change](references/semantic-change.md), [Author](references/author.md), [Change](references/change.md),
[Workflow](references/workflow.md), [Checks](references/checks.md), [History](references/history.md),
[Git](references/git.md), or [Lean](references/lean.md).

Author with handles; refactor with names or positions. With targets and checks, use one reviewed
task-change session for checks, apply, reversal and patch. Preview writes. Preserve
refusals, gaps and uncertainty.
