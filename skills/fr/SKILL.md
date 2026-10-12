---
name: fr
description: Use fr to find relevant code, preview and check changes, undo mistakes, deliver patches, and inspect supported proofs.
---
# Work with fr

For a checkout bug, find the calculation and callers, preview the fix and check behavior.
Run at the root or pass `-C`. Start with `fr guide --from GOAL`; follow ready actions and
their references. See [Guide](references/guide.md). Direct discovery:
```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```
Read coverage and omissions; missing rows can mean an unfinished answer. Retain handles and
bases for edits. See [Author](references/author.md) and [Workflow](references/workflow.md).
Preview first, run checks, preserve refusals and uncertainty.
