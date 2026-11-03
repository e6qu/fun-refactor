# Saved edits

Inspect `<TX>` (a saved edit's ID) and preview undo.

```sh
fr history show '<TX>'
fr history undo '<TX>'
fr history undo '<TX>' --write --no-diff
fr history redo '<TX>'
fr history redo '<TX>' --write --no-diff
```
`--no-diff` keeps metadata. An author's `transaction_context_basis` becomes history's
`context_basis`. Reuse it for apply/redo; review undo in full.
Undo the latest edit; redo the next. New edits abandon redo. Conflicts refuse.
Undo/redo preserve unrelated files and the Git index.

Record validation with [Checks](checks.md).
Combine explicit IDs in application order:
`fr history patch <FIRST_TX> <SECOND_TX> --output change.patch`.
Match file states between edits; review the combined patch.
Multi-file writes are not atomic. History omits timestamps, ownership, extended attributes and empty directories.
Pending writes: [Recovery](recovery.md). Patch checks and Git: [Git](git.md).
