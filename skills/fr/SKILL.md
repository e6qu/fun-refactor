---
name: fr
description: Find code, refactor, check edits, and write Lean proofs.
---
# Work with fr

Tactics are proof steps. Find a claim:
```sh
fr --json spec goals specs --limit 8 --token-limit 4096
```
Copy `target` as `<LEAN_TARGET>`:
```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```
Read `claim.parts`: assumptions and conclusions.
Start `.fr/proof-tactics` with `refine ?_` (no `by`) and check it now:
```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```
`passed` must be true to save: `fr --json spec prove '<LEAN_TARGET>' --from .fr/proof-tactics --write`.
Keep `.fr/checks.json`. [Proofs](references/lean.md); [Arithmetic](references/lean-arithmetic.md).

[Author](references/author.md); [Explore](references/explore.md).
Read omissions.
