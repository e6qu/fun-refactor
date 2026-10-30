---
name: fr
description: Find code, refactor, check edits, and write Lean proofs.
---
# Work with fr

Tactics are proof steps. Find a claim to prove:
```sh
fr --json spec goals specs --limit 8 --token-limit 4096
```
Copy `target` as `<LEAN_TARGET>`:
```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```
Read `claim.parts`: assumptions and conclusions.
Write `.fr/proof-tactics` without `by`. Check before library searches:
```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```
Repair until `passed` is true. Save with `fr --json spec prove '<LEAN_TARGET>' --from .fr/proof-tactics --write`.
Keep `.fr/checks.json`. [Proofs](references/lean.md): diagnostics, arithmetic, delivery.

[Author](references/author.md); [Explore](references/explore.md).
Copy IDs; read omissions.
