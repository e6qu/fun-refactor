---
name: fr
description: Use fr to find functions and callers, preview and check code changes, and resume or undo reviewed edits. Use when fr is installed and a task needs code investigation or a coordinated change.
---
# Work with fr

Run at the project root or use `-C PATH`. Start with the function name when known:

```sh
fr project find greet --signature
fr project select greet render validate --signature --source --bytes 2048
fr project map --depth 2 --limit 12
```

New to fr? Follow [your first checked change](references/first-change.md), including its files and undo.
A handle identifies code in one revision. A basis identifies reviewed inputs. Copy returned values unchanged; repeat stale queries.
Read omitted and unresolved results before claiming absence. A preview shows proposed changes; `--write` applies them.

Choose the reference for the task:
- [Explore](references/explore.md) for callers and bounded source reads.
- [Change](references/change.md) for supported renames; [author](references/author.md) for implementation edits.
- [Check and deliver](references/workflow.md) for a saved change; [history](references/history.md) to undo it.
- [Guide](references/guide.md) when the next command is unclear.
- [Investigate](references/investigations.md) for input-to-output questions; [Lean](references/lean.md) for proof tasks.
- [Python](references/runtime.md) to retain large reports outside model context.
