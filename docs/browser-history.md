# Undo a change in the browser playground

If a playground rename changes the wrong function, inspect the saved change and undo it. You can redo it if needed.
The browser checks that the affected files still match the saved change. Edited files require another review before it can safely reverse the operation.

WASM, or WebAssembly, lets the browser run the compiled tool.
Its workspace is an in-memory set of files. A transaction records one edit's before and after contents.
For a mistaken rename, inspect that transaction and request undo to restore those contents.
Browser history is bounded and belongs to that workspace; export a patch when you need to take the change elsewhere.

## Commands, results and limits

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

Illustration: rename a checkout helper, then change an unrelated note in the playground.
Undo can reverse the rename while keeping the note. If you edit the renamed helper itself,
undo refuses the conflict; inspect it before choosing another change.

`transaction_patch(id, reverse)` exports one transaction. `patch()` folds every currently applied
transaction back to the loaded workspace basis. Both call the same Rust Git text renderer as native
`fr history patch`; the playground's Download Patch button uses the cumulative form. Exports include
a SHA-256 digest, byte count and Git diff-section count.

Browser history retains at most 256 records, 1,024 changed paths per transaction, 16 MiB of snapshots
per transaction and 64 MiB total. `compact_history(keep)` retains the newest requested undo and redo
steps. It folds older applied steps into the cumulative patch basis, then discards old redo and
abandoned records. The cumulative patch remains byte-identical. Compacted transaction IDs cannot be
undone, redone or exported individually, and IDs are never reused.

The Download Patch button includes all currently applied changes since the loaded starting files.
For example, a rename followed by a body repair contributes both changes to that patch.
Keep a separate export before discarding history you may need to reverse individually.

`session()` exports `fr-browser-session-1`, a canonical checkpoint containing current files and the
complete retained history. `Workspace.from_session(text)` accepts at most 4 MiB and 4,096 files. It
checks the envelope digest, schema, relative paths, record bases, stack membership, size limits and
the complete applied and redo snapshot chains before constructing a workspace. Unknown fields,
changed bytes, unsafe paths and inconsistent intermediate states refuse the whole restoration.

The playground stores that envelope under one versioned `localStorage` key after loading a workspace
and after each completed apply, undo or redo. A page opened without `?repo=` restores it before any
network request. An explicit repository parameter loads that repository and replaces the checkpoint.
If storage is unavailable or full, the current edit remains applied and the page reports the storage
failure. A failed write leaves the prior checkpoint in place.

Rust mutations and browser storage replacement are synchronous. Only a fully completed transaction
can produce a checkpoint, so reload recovery selects the previous or next complete state. There is no
pending browser mutation to replay. Invalid saved state is removed and the ordinary loader continues.
Browser snapshots remain UTF-8 (the text encoding used to count source bytes) regular files projected as mode `0644`; executable modes and symlinks
remain native-only concerns.

[`FrKernels.MemoryHistory`](../kernels/FrKernels/MemoryHistory.lean) anchors the finite transition
predicate and proves that non-top or abandoned records cannot transition. Each live status permits
only its inverse action. A multi-snapshot mismatch refuses the whole abstract replacement, and
apply/undo restores the selected snapshot list. Restoration theorems require valid schema, digest,
history and finite file and payload bounds. Compaction theorems cover the retention boundary and the
zero and complete list cases. Executable comparisons include every transition input and machine-size
restoration and compaction boundaries. Reindexing, SHA-256, browser storage, JavaScript, Git and full
Rust/model correspondence (the stated connection between two descriptions) remain tested or trusted boundaries.
