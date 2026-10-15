# Recover an interrupted source write

A journal records a pending write. After interruption, inspect it before editing again. Recovery repairs pending work; undo reverses a completed edit.

Use `fr history` and `fr history show ID` to inspect the state.
Recover only pending operations with `fr history recover ID --write`.
Recovery restores its starting snapshots; recovering an interrupted undo can therefore restore the applied result.
If recovery refuses a conflict, preserve the journal and report affected paths. Retry after the blocker changes.
A crash may leave a lock; inspect ownership before deleting it.

Deleting the journal loses recovery data. Recovery does not make multiple file replacements atomic.
For ordinary undo/redo, return to [History](history.md). Git staging and worktree recovery use [separate commands](git-admin.md).
