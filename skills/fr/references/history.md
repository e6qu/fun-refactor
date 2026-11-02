# Apply or reverse source history

Inspect edit `<TX>` and its before/after files. Preview undo first.

```sh
fr history show '<TX>'
fr history undo '<TX>'
fr history undo '<TX>' --write --no-diff
fr history redo '<TX>'
fr history redo '<TX>' --write --no-diff
```
`--no-diff` keeps outcomes and metadata. An author plan's `transaction_context_basis` becomes history's `context_basis`.
Apply/redo can reuse it to omit reviewed diffs; undo needs full review.

Check content, links, kinds, existence and modes; keep unrelated files and the Git index.
Undo the latest edit; redo the next. A new applied edit abandons redo. Keep conflicting edits.

Record checks with `fr checks --run NAME --basis '<CHECK_BASIS>' --record-for '<TX>'`.
Pending writes and locks: [Recovery](recovery.md).
History omits timestamps, ownership, extended attributes and empty directories. Multi-file writes are not atomic.
Git has [its own journal](git.md).
