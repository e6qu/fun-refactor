# Staging history

Changed `fr git stage PATH... --basis TOKEN --write` operations now record their selected index entries in a worktree-local journal.
The result's `durability.journal.id` identifies the transaction. All-unchanged writes create no record and keep the redo stack intact.
Earlier staging writes made before journal support have no retrospective record.

```sh
fr git stage-history list --limit 20
fr git stage-history show 1
fr git stage-history undo 1
fr git stage-history undo 1 --basis TOKEN --write
fr git stage-history redo 1
fr git stage-history redo 1 --basis TOKEN --write
```

List and show omit blob bodies. List returns newest records first, with a limit from 1 through 500, total count and next undo/redo identities.
Show returns the selected paths and their before/after modes and object IDs.
Undo and redo default to previews. Their `--write` requires the basis from that transition preview, using the same action and identity.
Output remains JSON in both CLI output modes.

## Transition scope

Undo restores the recorded prior index entries; redo restores the recorded staged result.
Neither operation reads or modifies working-file contents, resets HEAD, creates commits, or changes source history.
The working files can differ, be missing, or have become binary since staging.
Unrelated staged paths survive, including unrelated conflict stages and index flags.
The journal records only changed paths. An explicitly selected path with an unchanged action remains outside that transaction.

Undo and redo follow separate stacks. The newest applied transaction is undone first; the most recently undone transaction is redone first.
A new changed staging transaction abandons the redo branch only after successful installation and journal finalization.
Records remain available for inspection after abandonment.

The transition basis binds the full journal state, action, transaction and current selected index identities.
A later unrelated index change leaves the basis valid; a journal change invalidates it.
Selected content, mode, existence or conflict drift causes refusal. Changing selected assume-unchanged, skip-worktree or intent-to-add state also blocks replay.
The checks do not detect changes restored between observations.

Restoration concerns raw entries, not byte-for-byte index serialization or stat caches.
Undoing the first additions in an unborn repository can leave a valid empty index where no index file existed before.
The same regular-file, split-index, sparse-checkout and Unix-host restrictions as [staging application](git-staging.md) apply.
Writes involving changed selected assume-unchanged, skip-worktree or intent-to-add entries are unsupported.
Unchanged selected entries and unrelated entries can retain those flags.

## Storage and recovery

The journal lives at `fr-stage/state.json` beside the worktree's index, normally `.git/fr-stage/state.json`.
Linked worktrees use separate journals. Repository root and index location bind the journal; moving a repository requires a future migration workflow.
New journal directories use mode `0700`; state files use private temporary files and atomic replacement.
Symlink storage and malformed records cause refusal.

Each record retains before/after blob bytes, modes and identities, with a SHA-256 record digest.
Old blobs must be readable when staging begins; demand fetching remains disabled.
Replays recreate needed objects from recorded bytes and check their Git identities, including SHA-256 repositories.
This supports restoring an old staged blob after Git prunes it. Source bodies stay inside local storage and outside reports.
Record digests detect accidental corruption; the journal is trusted local state, not an authenticated audit log.

A write syncs a pending journal record before replacing the index, then syncs the index directory before finalizing the journal.
One index lock coordinates the journal and index operation with cooperating Git writers.
A pending operation blocks further staging writes and ordinary undo/redo.
Inspect it with:

```sh
fr git stage-history list
fr git stage-history recover
fr git stage-history recover --basis TOKEN --write
```

Recovery rolls the pending operation back to its starting selected state.
It accepts the whole selected state before or after installation, and refuses mixed or third states.
Thus a pending undo recovers to the staged result; a pending redo recovers to the undone result.
Recovering a pending new application abandons that record while preserving prior undo/redo stacks.
Recovery also syncs an already-restored index before clearing the pending marker.
Unrelated index changes remain outside recovery.

`durability.journal.finalized: false` means finalization did not complete durably.
If installation happened, the command reports `applied: true` and a warning, even when finalizing the journal fails.
Inspect history before retrying: a failed directory sync can leave either a completed or pending journal visible.
Failures before installation may leave a pending marker and unreachable Git objects; recovery handles a matching recorded basis.

After process termination, an owned `index.lock` or temporary preparation directory may remain.
Confirm the writer has exited before removing a stale lock, then inspect recovery. The tool does not guess whether an existing lock is stale.
Keep configuration, HEAD, journal storage and repository directory topology stable during operations.
The index lock cannot constrain writers that bypass it, and filesystem durability still depends on the host honoring sync and atomic rename.
There is no journal retention or compaction policy yet; reads validate and load the complete journal, despite bounded report rows.

## Formal coverage

The [Git Lean model](../kernels/FrKernels/Git.lean) anchors the transition acceptance predicate and checks every boolean input against Rust.
Its abstract index laws prove undo/redo round trips and preservation of unselected paths, including later unrelated changes.
These laws assume correct snapshot identities and replacement of exactly the selected entries.
The existing history stack laws also describe the intended undo/redo ordering.
Filesystem execution, journal parsing, object storage, crash durability and complete Rust correspondence remain outside these proofs.
Regression tests cover the implemented journal, replay and recovery paths.
