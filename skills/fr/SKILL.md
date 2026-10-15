---
name: fr
description: Find code, review, check, undo and deliver changes with fr.
---
# Work with fr

Find the bug's code, preview the fix and test behavior. Run at the project root or use `-C PATH`.
A handle selects code; a basis ties actions to reviews. Copy them unchanged.
`fr guide --from GOAL` starts from a JSON task file. See [Guide](references/guide.md).

```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```

Read omissions. [Review edits](references/author.md), then [check and deliver](references/workflow.md).
