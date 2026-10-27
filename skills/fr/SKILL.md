---
name: fr
description: Use fr to refactor code, check targeted edits, or write and check Lean proofs for supported code.
---
# Work with fr

Existing Lean theorem (`FILE::THEOREM`): start here with its target.
```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```
[Proofs](references/lean.md): write, check and save your tactics.

Source edit: [Author](references/author.md); [Workflow](references/workflow.md) needs a saved plan.
Copy handles, IDs and bases unchanged. Read omissions.
[Explore](references/explore.md) code; [First change](references/first-change.md) gives an example.
