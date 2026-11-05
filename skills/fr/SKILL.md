---
name: fr
description: Find code, refactor, check edits, and write or repair Lean proofs with fr.
---
# Work with fr

Edits: [Author](references/author.md). Discovery: [Explore](references/explore.md). Read omissions.

Proofs: [Lean](references/lean.md). [Arithmetic](references/lean-arithmetic.md) for comparisons or division; [Algorithms](references/lean-algorithms.md) for recursion.
Keep claims, models and programs fixed. Choose a target with `fr proof list specs --limit 8 --bytes 4096`.
```sh
fr --json proof start '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30
```
Inspect edits with `proof state --at end`: goals have `context` and `goal` fields.
`proof check` rejects unfinished drafts with exit 1; `proof save` checks and writes.
Follow [Lean](references/lean.md) through package checking and patch delivery.
