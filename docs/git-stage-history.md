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
fr git stage-history compact --keep 100
fr git stage-history compact --keep 100 --basis TOKEN --write
fr git stage-history inspect --stale-after 3600
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
[Reviewed commits](git-commit.md) preserve completed staging history. A later staging undo changes the index relative to HEAD without removing the commit.

## Retention and compaction

`compact --keep N` previews removal of old replay payloads while retaining every record identity, status, path count and audit digest.
It keeps the top `N` detailed records on the undo stack and the top `N` on the redo stack.
Abandoned payloads are eligible immediately because they cannot be replayed.
The default is 100 per stack; zero retires all current undo and redo payloads.

The preview reports selected records and paths, journal bytes before and after, the resulting stack tops and a `frstagecompact1:` basis.
The checked write requires that basis, takes the worktree's Git index lock and rechecks the complete journal before replacing it.
It does not read or change the index, working files, HEAD, Git objects or source history.
A pending staging transition refuses compaction until recovery finishes.
Only `applied: true` confirms compaction. A storage or sync failure returns `applied: null` with bounded diagnostics because the replacement may already be visible.

Compacted records remain visible to `list` and `show` with `compacted: true`.
They retain their original content digest plus a digest over the compacted audit summary, but omit `entries` because their before/after bytes are gone.
Compaction cannot be undone, and a retired record cannot later become an undo or redo target.
New records continue with the next monotonic ID.

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

After process termination, an owned `index.lock` or temporary `fr-stage-*` preparation directory may remain.
`stage-history inspect` reports their paths, file types, identities, modes, sizes and ages without reading their contents.
It combines that evidence with the pending journal action and classifies the state as `clean`, `manual-review` or `recovery-required`.
`--stale-after SECONDS` changes the age threshold from its one-hour default; `--limit` bounds preparation rows while retaining the full count.

An old regular lock or directory is only a `stale_candidate`; every row keeps `safe_to_remove: false`.
Age does not prove that its writer exited, and symlinks are never candidates.
Confirm the writer has exited before handling a candidate manually, preserve the reported identity, then inspect or recover the journal again.
The command never removes locks or preparation data.
Keep configuration, HEAD, journal storage and repository directory topology stable during operations.
The index lock cannot constrain writers that bypass it, and filesystem durability still depends on the host honoring sync and atomic rename.
Compaction bounds retained replay payloads, while record summaries and their IDs remain in the journal for audit.
The command is explicit rather than time based and does not remove summaries.

## Formal coverage

The [Git Lean model](../kernels/FrKernels/Git.lean) anchors transition, record-compaction and crash-review predicates and checks every boolean input against Rust.
Its abstract index laws prove undo/redo round trips and preservation of unselected paths, including later unrelated changes.
Its payload model proves that selected compaction discards detail, preserves unselected detail and is idempotent.
The crash-review model proves that only the absence of pending, lock and preparation evidence is classified clean.
These laws assume correct snapshot identities and replacement of exactly the selected entries.
The existing history stack laws also describe the intended undo/redo ordering.
Filesystem execution, journal parsing, object storage, crash durability and complete Rust correspondence remain outside these proofs.
Regression tests cover the implemented journal, replay, recovery and compaction paths.
