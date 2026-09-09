# Apply or reverse a source transaction

The examples assume `<TX>` is applied:

```sh
fr history show '<TX>'
fr history undo '<TX>'
fr history undo '<TX>' --write --no-diff
fr history redo '<TX>'
fr history redo '<TX>' --write --no-diff
```

Preview first. `--no-diff` keeps outcome and change metadata. A complete saved author diff supplies `transaction_context_basis`; detailed `history show` calls it `context_basis`. On forward apply/redo, pass that value to `--context-basis` to replace reviewed diffs with `diff_bytes`. Keep the full basis report. Reverse transitions require full review; stale or different bases refuse before writing.

Undo/redo verify affected contents or link targets, entry kinds, existence, and regular-file modes while preserving unrelated edits. Undo must be the latest applied transaction; redo must be next on its stack. A new applied transaction abandons the redo branch. Never erase a conflicting user edit to force a transition.

For an interrupted write or lock, load [Recovery](recovery.md). The journal retains source snapshots but not timestamps, ownership, extended attributes, empty directories, or multi-file filesystem atomicity. Git has separate journals; see [Git](git.md).
