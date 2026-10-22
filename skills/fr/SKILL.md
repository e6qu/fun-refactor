---
name: fr
description: Use fr to find code and callers, preview and check edits, or undo a reviewed change.
---
# Work with fr

Run at the project root or use `-C PATH`. New to fr? Follow [your first checked change](references/first-change.md).

```sh
fr project find greet --signature
```

Read omissions. Copy handles, which identify code in one revision, unchanged.
For a rename, read [Change](references/change.md); for implementation edits, [Author](references/author.md).
Then [check and deliver](references/workflow.md). A preview shows proposed edits; `--write` applies them.
