# Reverse or recover a source transaction

Use the transaction ID from the change report. The examples assume it has just been applied.

```sh
fr history show '<TX>'
fr history undo '<TX>'
fr history undo '<TX>' --write
fr history redo '<TX>'
fr history redo '<TX>' --write
```

Preview the transition and inspect the resulting report after writing.
Undo requires the latest applied transaction; redo requires the next ID on the redo stack.
Both check affected contents, existence and modes. They preserve unrelated edits and refuse conflicts in affected files.
Do not overwrite a conflicting user's edit to make undo succeed; inspect the conflict and select a repair within the task's scope.
Applying a new transaction abandons the previous redo branch. Saving a plan alone preserves it.

For interrupted writes, use `fr history` to inspect the pending operation and `fr history show ID` for its recorded state.
Only a pending operation calls for `fr history recover ID --write`.
Recovery restores that operation's starting snapshots; recovering an interrupted undo can therefore restore the applied result.
If recovery refuses a conflicting state, preserve the journal and report the affected paths for repair. Retry only after the blocker changes.
A crash may leave a lock; inspect ownership instead of automatically deleting it.

The `.fr-history` journal retains source snapshots and recovery data. Deleting it loses those records.
History does not restore timestamps, ownership, extended attributes or empty directory topology.
Multiple file replacements are not one atomic filesystem transaction.
Git staging journals and worktree recovery use different commands and identifiers; see [Git](git.md).
