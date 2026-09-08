# Recover an interrupted source write

Use `fr history` to inspect the pending operation and `fr history show ID` for its recorded state.
Only a pending operation calls for `fr history recover ID --write`.
Recovery restores that operation's starting snapshots; recovering an interrupted undo can therefore restore the applied result.
If recovery refuses a conflicting state, preserve the journal and report the affected paths for repair. Retry only after the blocker changes.
A crash may leave a lock; inspect ownership instead of automatically deleting it.

The journal contains recovery data; deleting it loses those records. Recovery does not make multiple file replacements atomic.
For ordinary undo/redo, return to [History](history.md). Git staging and worktree recovery use [separate commands](git-admin.md).
