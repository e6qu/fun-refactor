# Browser transaction history

Every successful refactoring in the WASM `Workspace` records one in-memory transaction. The apply
report returns its numeric `transaction` and `frmb1:` transaction basis. `history()` lists retained
records and the applied and redo stacks.

`undo(id)` accepts only the latest applied transaction. `redo(id)` accepts only the latest undone
transaction. Before either operation writes, the workspace compares every selected path with the
recorded expected snapshot. A mismatch refuses the complete transition, preserving all current
files. Starting a new transaction abandons the redo stack.

The record distinguishes an absent file from an empty file. Undo therefore removes a file created by
a translation or other refactoring, and redo recreates it before the changed paths are reindexed.
Unrelated files are outside the transaction and remain untouched.

`transaction_patch(id, reverse)` exports one transaction. `patch()` folds every currently applied
transaction back to the loaded workspace basis. Both call the same Rust Git text renderer as native
`fr history patch`; the playground's Download Patch button uses the cumulative form. Exports include
a SHA-256 digest, byte count and Git diff-section count.

Browser history retains at most 256 records, 1,024 changed paths per transaction, 16 MiB of snapshots
per transaction and 64 MiB total. It lasts only for the current loaded `Workspace`. Browser snapshots
are UTF-8 regular files projected as mode `0644`; executable modes, symlinks, persistence, compaction
and crash recovery remain native-only concerns.

[`FrKernels.MemoryHistory`](../kernels/FrKernels/MemoryHistory.lean) anchors the finite transition
predicate and proves that non-top or abandoned records cannot transition, each live status permits
only its inverse action, a multi-snapshot mismatch refuses the whole abstract replacement, and
apply/undo restores the full selected snapshot list. Exhaustive executable cases compare the Rust
and Lean transition predicates. Filesystem mutation, reindexing, hashing, patch rendering, JavaScript,
the browser runtime, Git and the correspondence outside those finite cases remain trusted.
