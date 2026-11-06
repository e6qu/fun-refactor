---
name: fr
description: Find code, refactor, check edits, and write or repair Lean proofs with fr.
---
# Work with fr

Edits: [Author](references/author.md). Discovery: `fr project find`, see [Explore](references/explore.md). Read omissions.

Proofs: [Lean](references/lean.md). [Arithmetic](references/lean-arithmetic.md) for comparisons or division; [Algorithms](references/lean-algorithms.md) for recursion.
Keep claims, models and programs fixed. With a named target, start at once; read references when stuck.
Otherwise choose one with `fr proof list specs --limit 8 --bytes 4096`.
```sh
fr --json proof start '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30
```
Inspect edits with `proof state --at end`: goals have `context` and `goal` fields.
`proof check` rejects unfinished drafts with exit 1. Once a check passes, deliver in one call:
`proof save` with `--deliver change.patch` checks, writes, runs `.fr/checks.json` and writes the patch.
Details: [Lean](references/lean.md).
