# Apply or reverse source history

```sh
fr history show '<TX>'
fr history undo '<TX>'
fr history undo '<TX>' --write --no-diff
fr history redo '<TX>'
fr history redo '<TX>' --write --no-diff
```
Preview transitions. `--no-diff` retains outcomes and metadata. A complete author plan's `transaction_context_basis` is `context_basis` in detailed history. Forward apply/redo may reuse it to omit reviewed diffs; undo needs full review.

Transitions verify affected content, links, kinds, existence, and regular-file modes while preserving unrelated files and the Git index. Undo must be latest; redo must be next; a new applied transaction abandons redo. Preserve conflicting edits.

Attach a receipt with `fr checks --run NAME --basis '<CHECK_BASIS>' --record-for '<TX>'`. For a pending write or lock, read [Recovery](recovery.md). History lacks filesystem-wide atomicity, timestamps, ownership, extended attributes, and empty directories. Git has [separate journals](git.md).
