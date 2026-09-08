---
name: fr
description: Use the fr CLI for bounded project structure, structural code changes, source history, Git patches, checks, and Lean evidence. Use when fr is available or requested.
---

# Work with fr

Run at the project root or pass `-C`. Find a known declaration directly; request source only when needed:

```sh
fr project find greet --signature
fr project map --depth 2 --limit 12
```

Read `columns` with `rows`, `coverage`, `omitted`, and `page`. Gaps or omitted/unresolved rows are not evidence of absence. Retain a full report's `context_basis` and pass it to related project/author calls; a stale basis refuses.

Load only the relevant reference:

- [Author](references/author.md): structural body edits and Rust declaration insertion/replacement.
- [Explore](references/explore.md): pagination, relationships, and broader discovery.
- [Change](references/change.md): built-in refactorings and recipes.
- [Checks](references/checks.md): declared validation and bounded output.
- [History](references/history.md): apply, undo, redo, conflicts, and recovery.
- [Git](references/git.md): patches, indexes, commits, and worktrees.
- [Lean](references/lean.md): source drift, signature maps, and proof evidence.

Handles expire after source changes. Built-in refactorings use names or positions; `fr author` uses handles. Mutations preview by default. `--save-plan` records a checked plan for later history application. Inspect omissions and refusals as evidence limits.

Keep full basis reports. Report parser, project-check, and Lean results separately because they prove different properties.
