---
name: fr
description: Find code, refactor, check edits, and write or repair Lean proofs with fr.
---
# Work with fr

Edits: [Author](references/author.md). Discovery: [Explore](references/explore.md). Read omissions.

Proofs: [Lean](references/lean.md). [Arithmetic](references/lean-arithmetic.md), [Division](references/lean-division.md) and [Algorithms](references/lean-algorithms.md) for recursion.
Keep claims, models and programs fixed. Choose targets with `fr proof list specs --limit 8 --bytes 4096`.
```sh
fr --json proof start '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30
```
Run this first: it lists goals and `library_facts`. Inspect edits with `proof state --at end`.
`proof check` rejects unfinished drafts with exit 1; `proof save --deliver change.patch`
checks, writes and delivers.
