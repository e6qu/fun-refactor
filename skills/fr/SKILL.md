---
name: fr
description: Find code, refactor, check edits, and interactively write or repair Lean proofs with fr.
---
# Work with fr

Edits: [Author](references/author.md). Discovery: [Explore](references/explore.md). Read omissions.

Proofs: [Lean](references/lean.md). For comparisons or division read [Arithmetic](references/lean-arithmetic.md).
Recursive programs: [Algorithms](references/lean-algorithms.md).
Keep the claim, model and program fixed. Start `.fr/proof-tactics` with `skip` (no `by`). Inspect after it:
```sh
fr --json spec proof-state '<LEAN_TARGET>' --from .fr/proof-tactics --at 1:5 --token-limit 4096 --timeout-seconds 30
```
Edit, then inspect each step's goals and available facts. A closed branch is not an accepted proof: require `proof-check`'s `passed: true` before `prove --write`. Preserve `.fr/checks.json`.
