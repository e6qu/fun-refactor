---
name: fr
description: Use fr for bounded project inspection, structural edits, reversible history, Git patches, declared checks, and Lean evidence when fr is available or requested.
---
# Work with fr

Run at the project root or pass `-C`. Query known declarations directly; request source only when needed:
```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```
Pass full `find` handles to `project select`. `project show` takes a handle and byte options. Read `coverage`, omissions, pages, and selection statuses. Missing or clipped rows do not prove absence. Retain full bases; stale identities refuse.

For a structural task needing several views, use [Task](references/task.md) to obtain targets,
checks and delivery templates in one request.

Load only the needed route: [Explore](references/explore.md), [Author](references/author.md), [Change](references/change.md), [Workflow](references/workflow.md), [Checks](references/checks.md), [History](references/history.md), [Git](references/git.md), or [Lean](references/lean.md).

Authoring uses revision-bound handles; refactors use names or positions. Mutations preview by default. Prefer saved transactions for coordinated changes, then apply them. Preserve refusals, gaps, and uncertainty as evidence limits.
