---
name: fr
description: Find code, refactor, check edits, and write Lean proofs.
---
# Work with fr

Lean goals are claims; tactics are proof steps. Find targets:
```sh
fr --json spec goals specs --limit 8 --token-limit 4096
```
Copy `target` as `<LEAN_TARGET>`. Draft `.fr/proof-tactics` without `by`.
Check before library searches:
```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```
Read `passed` and diagnostics. [Proofs](references/lean.md): repair, save, deliver.

[Author](references/author.md); [Explore](references/explore.md).
Copy IDs/bases; read omissions.
