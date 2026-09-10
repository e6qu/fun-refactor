# Apply or reverse source history

For an applied `<TX>`:

```sh
fr history show '<TX>'
fr history undo '<TX>'
fr history undo '<TX>' --write --no-diff
fr history redo '<TX>'
fr history redo '<TX>' --write --no-diff
```

Preview transitions. `--no-diff` retains outcome and change metadata. A complete author plan supplies `transaction_context_basis`; detailed `history show` calls it `context_basis`. A forward apply or redo may pass that retained basis with `--context-basis` to omit reviewed diffs. Undo always needs its own full review.

Transitions verify affected contents, links, kinds, existence, and regular-file modes. They preserve unrelated files and the Git index. Undo must be the latest applied transaction; redo must be next, and a new applied transaction abandons the redo branch. Conflicting user edits must remain untouched.

Attach a passing check receipt with `fr checks --run NAME --basis '<CHECK_BASIS>' --record-for '<TX>'`. Required checks bind an exact configuration and ordered names.

For a pending write or lock, read [Recovery](recovery.md). The journal lacks filesystem-wide atomicity, timestamps, ownership, extended attributes, and empty directories. Git uses [separate journals](git.md).
