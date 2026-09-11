---
name: fr
description: Use fr for bounded inspection, structural edits, history, Git patches, checks, and Lean evidence.
---
# Work with fr

Run at the project root or pass `-C`. Query declarations directly; request source only when needed:
```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```
Pass full `find` handles to `project select`. `project show` takes a handle and byte options. Read `coverage`, omissions, pages, and selection statuses. Missing or clipped rows do not prove absence. Retain full bases; stale identities refuse.

For multi-view structural work, read [Task](references/task.md). It also covers reviewed execution
after fragments exist and checks are set.

Load only the needed route: [Semantic](references/semantic.md), [Explore](references/explore.md), [Author](references/author.md), [Change](references/change.md), [Workflow](references/workflow.md), [Checks](references/checks.md), [History](references/history.md), [Git](references/git.md), or [Lean](references/lean.md).

Authoring uses revision-bound handles; refactors use names or positions. Mutations preview by default. Prefer saved transactions for coordinated changes, then apply them. Preserve refusals, gaps, and uncertainty as evidence limits.
