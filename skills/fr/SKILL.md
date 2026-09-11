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
Pass full `find` handles directly to `project select`. `project show` takes a handle and byte options, not line ranges. Read `coverage`, omissions, pages, and selection statuses. Missing or clipped rows do not prove absence. Retain full basis reports; stale identities refuse.

Load only the needed route: [Explore](references/explore.md) structure; [Author](references/author.md) edits; [Change](references/change.md) refactors; [Workflow](references/workflow.md) checked delivery; [Checks](references/checks.md) commands; [History](references/history.md) transitions; [Git](references/git.md) patches or state; [Lean](references/lean.md) proofs.

Authoring uses revision-bound handles; refactors use names or positions. Mutations preview by default. Prefer one reviewed saved transaction for a coordinated change, then apply it. Preserve refusals, gaps, and uncertainty as evidence limits.
