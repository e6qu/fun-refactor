---
name: fr
description: Use the fr CLI to inspect project hierarchy with bounded output, plan structural code changes, and work with its transaction history, Git patches, and Lean source checks. Use when fr is available or the user requests it.
---

# Work with fr

Run `fr` at the project root or pass `-C`. For a known declaration, use `fr project find NAME --signature`; add `--in PATH`, `--contains`, or `--source --bytes N` only as needed. For broader discovery start with:

```sh
fr project map --depth 2 --limit 12
```

Read `columns` with `rows`, plus `coverage`, `omitted` and `page`. Gaps and unresolved or omitted results are not evidence of absence. Retain a full report's `context_basis`; pass it as `--context-basis` on related project and author calls to omit unchanged project context. A stale basis refuses.

Load only the reference needed for the task:

- [Author](references/author.md): targeted body edits, Rust declaration replacement and insertion.
- [Explore](references/explore.md): pagination, relationships and broader discovery.
- [Change](references/change.md): built-in refactorings and recipes.
- [Checks](references/checks.md): declared project validation and bounded execution reports.
- [History](references/history.md): undo/redo and conflicts; links to recovery for interrupted writes.
- [Git](references/git.md): patches, indexes and the separate worktree lifecycle.
- [Lean](references/lean.md): source drift, signature maps and proof evidence.

Project handles expire after source changes. Built-in refactorings take names or positions; `fr author` takes handles. Mutations preview by default; `--save-plan` records an unchanged-source plan for later history application. Inspect results and omissions before applying. Treat refusals as missing evidence or unsupported scope.

Keep full basis reports in the audit trail. Report parser checks, project tests and Lean evidence separately; each proves a different property.
