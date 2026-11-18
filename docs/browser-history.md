# Review and recover browser changes

The playground keeps files in memory and saves completed edits in browser storage.
Review a proposed change, apply it, then export its patch for checks in a complete checkout.

## Review before applying

Every editing action in the playground shows a diff before writing.
**Apply reviewed change** accepts that review; **Cancel** leaves files and history alone.
The review includes warnings, proposed file contents and the transaction patch.

The JavaScript API exposes the same route:

```javascript
const review = workspace.preview(JSON.stringify({
  operation: "rename", path: "app.py", line: 1, col: 5, name: "render"
}));
const proposal = JSON.parse(review);
if (proposal.error) throw new Error(proposal.error);
// Inspect proposal.review.result and proposal.review.patch before continuing.
const result = JSON.parse(workspace.apply_review(review));
if (result.error) throw new Error(result.error);
```

Requests cover rename, remove-flag, extract-variable, extract-function, inline-variable,
inline-call, signature, move-symbol, organize-imports, rewrite, restructure, delete and translate.
Unknown operations, fields and invalid arguments refuse.

A review binds the entire current session, request, warnings, proposed result and patch.
Apply repeats the operation on a private copy and compares the new review before replacing live state.
Changed source, undo/redo history or compaction requires a new review.
A saved review still works after restoring the identical session.
A review digest detects altered contents; it does not authenticate who approved the change.

The existing direct mutation methods still perform their named operation immediately.
Use `preview` and `apply_review` when the caller needs a separate review step.
Both routes apply refactoring rules and reparse changed files.
Neither runs native compilers, project tests or Lean. The review reports an empty `native_checks` list.

Requests have a 64 KiB limit; reviews have a 16 MiB limit.
The starting and proposed sessions must fit the checkpoint limits below.
A refused preview or apply leaves the live workspace unchanged.

## Undo, redo and patch delivery

A successful edit returns its transaction ID and `frmb1:` basis.
`history()` lists retained records and the applied and redo stacks.
`undo(id)` accepts the latest applied transaction; `redo(id)` accepts the latest undone one.
Both compare every selected file with its recorded snapshot before writing.
A conflict refuses the whole transition. Unrelated files remain untouched.
Starting another transaction abandons the redo stack.

Snapshots distinguish absent files from empty ones, so undo removes newly created files.
`transaction_patch(id, reverse)` exports one transaction.
`patch()` exports all currently applied changes since the starting workspace.
Both use the native Git text renderer and return a digest, byte count and section count.
The Download Patch button uses the cumulative patch. Check it in a complete native checkout.

History retains at most 256 records, 1,024 paths per transaction, 16 MiB per transaction
and 64 MiB overall. `compact_history(keep)` retains the newest requested undo and redo steps.
It folds older applied edits into the cumulative patch basis, preserving the patch bytes.
Discarded IDs cannot be undone, redone or exported individually; IDs never repeat.

## Restart

`session()` exports a canonical `fr-browser-session-1` checkpoint.
`Workspace.from_session(text)` accepts at most 4 MiB and 4,096 distinct current or historical paths.
It checks the digest, schema, relative paths, record bases, stacks, limits and snapshot chains.
Unknown fields or inconsistent state refuse the entire restoration.
Limits count UTF-8 bytes, not JavaScript characters.

The playground replaces one versioned `localStorage` entry after loading, applying, undoing or redoing.
Without `?repo=`, a reload restores that checkpoint before making a network request.
An explicit repository parameter loads that repository and replaces the checkpoint.
Invalid saved state is removed before the ordinary loader continues.

If storage fails, the edit remains in memory and the page reports the failure.
A failed storage write keeps the prior checkpoint. Export the current patch before closing.
Mutations and checkpoint replacement are synchronous: reload selects the previous or next complete state.
Browser snapshots contain UTF-8 regular files with mode `0644`; native history also handles modes and symlinks.

The [interface coverage guide](interface-coverage.md) links restart, stale-review and delivery tests.
The [Lean memory-history model](../kernels/FrKernels/MemoryHistory.lean) proves finite transition,
restoration and compaction properties. Tests cover the Rust adapter, hashing and browser storage.
These model theorems do not prove the browser or filesystem implementation.
