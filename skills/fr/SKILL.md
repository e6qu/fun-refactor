---
name: fr
description: Find code, refactor, check edits, and write Lean proofs.
---
# Work with fr

Lean: a goal is a claim; tactics are proof steps. Discover the target:
```sh
fr --json spec goals specs --limit 8 --token-limit 4096
```
Copy `target` as `<LEAN_TARGET>`. Write `.fr/proof-tactics` without `by`:
```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```
Read `passed` and diagnostics. [Proofs](references/lean.md) covers repairs and saving.

Code edits: [Author](references/author.md); [Explore](references/explore.md).
Copy IDs and bases; read omissions.
