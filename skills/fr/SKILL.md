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
Draft `.fr/proof-tactics` without `by`. Check before library searches.
[Proofs](references/lean.md): check, repair, save, deliver.
Keep `.fr/checks.json`.

[Author](references/author.md); [Explore](references/explore.md).
Copy IDs; read omissions.
