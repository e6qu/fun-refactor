---
name: fr
description: Use fr for bounded project inspection, structural edits, reversible history, Git patches, declared checks, and Lean evidence when fr is available or requested.
---

# Work with fr

Run at the project root or pass `-C`. Query known declarations directly and request source only when needed:

```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```

Read `coverage`, `omitted`, pagination, and every selection status. Missing or clipped rows do not prove absence. Retain full `context_basis` reports for related calls; stale bases refuse.

Read only the route needed now:

- [Author](references/author.md) for implementation edits.
- [Explore](references/explore.md) for unknown structure, relations, or pagination.
- [Change](references/change.md) for built-in refactors, recipes, or feature migration.
- [Checks](references/checks.md) before running declared project commands.
- [History](references/history.md) before applying, undoing, or redoing.
- [Git](references/git.md) for patch export or Git state.
- [Lean](references/lean.md) for specifications and proof evidence.

`fr author` uses revision-bound handles; built-in refactors use names or positions. Mutations preview by default. Prefer one reviewed saved transaction for a coordinated change, then apply that exact transaction. Preserve full basis reports and treat refusals, gaps, and uncertainty as limits on the evidence.
