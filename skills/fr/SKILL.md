---
name: fr
description: Use the fr CLI to inspect project hierarchy with bounded output, plan structural code changes, and work with its transaction history, Git patches, and Lean source checks. Use when fr is available or the user requests it.
---

# Work with fr

Use `fr` from the target project root, or select that root with `-C`. Check `fr --version` and command help when the installed build differs.

For a known declaration, start with `fr project find NAME --signature`; use `--in PATH` to narrow it.
Otherwise start with a small structural view:

```sh
fr project map --depth 2 --limit 12
```

Maps already return compact JSON. Read `columns` with `rows`, plus `coverage`, `omitted` and `page`; an omitted or unresolved result is not evidence of absence.
Narrow to the relevant subtree before requesting more rows. Output limits do not limit indexing cost.

Load only the reference needed for the task:

- [Explore](references/explore.md): handles, signatures, relationships and bounded source.
- [Author](references/author.md): targeted body edits, Rust declaration replacement and insertion.
- [Change](references/change.md): built-in refactorings and recipes.
- [Checks](references/checks.md): declared project validation and bounded execution reports.
- [History](references/history.md): apply, undo/redo, conflicts and interrupted source writes.
- [Git](references/git.md): patches, indexes and the separate worktree lifecycle.
- [Lean](references/lean.md): source drift, signature maps and proof evidence.

Keep the workspace root and review identity attached to each result.
Project handles expire after source changes. Refresh only when another source query or edit needs a handle; history and checks do not. Built-in refactorings take names or positions; `fr author` takes project handles.
Read only the necessary body slices when a header and relationships cannot answer the task.

Mutations preview by default. Within the user's authorized scope, inspect the result before applying the same saved transaction or Git basis.
`--save-plan` writes history but leaves source unchanged. Check reported outcomes as well as process status; some Git failures return `applied: null`.
A refusal calls for the named missing information or a different supported operation, not a forced retry.

Report syntax checks, project tests and Lean evidence separately. Parser acceptance does not establish behavioral equivalence; model proofs do not prove the entire implementation.
