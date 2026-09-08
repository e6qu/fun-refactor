# Reverse or recover a source transaction

Use the transaction ID from the change report. The examples assume it has just been applied.

```sh
fr history show '<TX>'
fr history undo '<TX>'
fr history undo '<TX>' --write --no-diff
fr history redo '<TX>'
fr history redo '<TX>' --write --no-diff
```

Preview the transition, then use `--no-diff` with `--write` to omit repeated diff text from the completion report.
The report retains paths, existence, modes and the operation outcome; `history show TX` and patch export keep the full diff.
Undo requires the latest applied transaction; redo requires the next ID on the redo stack.
Both check affected contents, existence and modes. They preserve unrelated edits and refuse conflicts in affected files.
Do not overwrite a conflicting user's edit to make undo succeed; inspect the conflict and select a repair within the task's scope.
Applying a new transaction abandons the previous redo branch. Saving a plan alone preserves it.

For a pending operation or a lock left after interruption, load [Recovery](recovery.md) before another write.

The `.fr-history` journal retains source snapshots and recovery data. Deleting it loses those records.
History does not restore timestamps, ownership, extended attributes or empty directory topology.
Multiple file replacements are not one atomic filesystem transaction.
Git staging journals and worktree recovery use different commands and identifiers; see [Git](git.md).
