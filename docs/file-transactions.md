# Edit, move or delete files with a review and an undo

Suppose you want to move `old/name.rs` or delete an unused file. Inspect the proposed file operation before writing.
`fr` checks the files against that review and keeps recovery information for supported operations.

A transaction saves the starting and proposed file states for one change.
A file mode includes whether the owner can execute it. A symbolic link points to another path instead of containing regular file contents.
For a script move, review its destination, contents and executable setting together.
Undo restores recorded states only when later changes do not conflict; it does not authorize overwriting newer work.

## Commands, results and limits

Use explicit paths relative to the workspace selected by `-C`:

```sh
fr -C /project file delete obsolete.txt empty.txt
fr -C /project file delete obsolete.txt empty.txt --save-plan
fr -C /project file move old/name.rs new/name.rs --save-plan
fr -C /project history patch 1
fr -C /project history apply 1 --write
fr -C /project history undo 1 --write
fr -C /project history redo 1 --write
fr -C /project history compact --keep 100
```

`fr file delete` removes complete regular text files. Empty files count as deletions.
It does not inspect references or determine whether the project still builds.
Use the existing symbol-level `fr delete` when its reference checks match the intended change.

UTF-8 encodes text as bytes. For example, `é` occupies two bytes, so byte limits can differ from character counts.

`fr file move SOURCE DESTINATION` moves one regular text file or UTF-8 symlink to an absent path.
It preserves contents or link target, entry kind and the complete recorded regular-file mode. The
source deletion and destination creation share one transaction, basis, patch and reversal history.
It does not rewrite imports, references or package declarations.

For example, moving `old/name.rs` can leave an import pointing at the old path.
The move preserves the file and supports undo, but you must review the imports and run the build.
Each move or deletion command records its own transaction; the sequence above contains separate plans.

## Exact text replacements

Suppose you renamed a Python function from `clamp` to `clamp_value`. Its documentation
contains `.. autofunction:: boltons.mathutils.clamp`, a Sphinx instruction that imports
the function to show its documentation. That instruction must change too.

```sh
fr -C /project file replace docs/mathutils.rst \
  --before '.. autofunction:: boltons.mathutils.clamp' \
  --after '.. autofunction:: boltons.mathutils.clamp_value' --matches 1 --save-plan
fr -C /project history patch 1
fr -C /project history apply 1 --write
```

Use the returned transaction ID in place of `1`. The saved plan keeps the exact
replacement; applying it refuses a file that has changed since review. Undo and
redo preserve the recorded contents and permission bits.

Both texts and the expected number of matches are required. Matches are literal,
case sensitive and nonoverlapping. A count mismatch refuses the edit. An explicit
empty `--after ''` deletes the matched text. The source and resulting file are each
limited to 64 KiB; the two argument texts together are limited to 64 KiB, with
between 1 and 500 matches. Binary files and symbolic links are refused.

This command checks text and snapshots. It does not resolve Python imports or
understand Sphinx instructions. Run the project's documentation checks after the
edit; a misspelled replacement could otherwise leave a broken reference.

## Executable permissions

```sh
fr file executable scripts/build.sh --set on
fr file executable scripts/build.sh --set on --save-plan
fr file executable scripts/build.sh --set off --write
fr file symlink public/current --target releases/v2 --save-plan
```

`on` sets the owner-execute bit; `off` clears it.
Both preserve complete file contents and every other recorded permission bit, including group/other execute and special bits.
For example, `0644` becomes `0744`, while `0755` becomes `0655` when owner execute is cleared.
This command does not set arbitrary Unix modes or normalize all execute bits.
Already-correct files remain unchanged and do not enter the transaction.
An entirely unchanged request creates no transaction, including with `--save-plan` or `--write`.

## Symbolic links

`fr file symlink PATH --target TARGET` creates a link or replaces an existing regular file or link.
The target is literal: it may be relative, absolute or dangling, and `fr` never follows it.
Targets contain 1 through 1,023 UTF-8 bytes without NUL.
`fr file delete` also records deletion of a link; `file executable` continues to require a regular file.

## Planning and reports

Delete and executable commands accept 1 through 500 explicit paths. Move accepts one source and one
destination, while symlink accepts one path. Every command reports JSON in both output modes.
The default only previews. `--save-plan` records a plan; `--write` records and applies it.
Those two flags are mutually exclusive.
Use the returned transaction ID for history inspection, application, undo, redo, recovery and patch export.
Running a fresh command after a preview observes the files again; applying a saved plan checks its recorded basis.

The report includes `schema: 1`, `workspace_root`, `operation`, `set`, `target`, `mode_scope`, `validation`, `basis` and `transaction`.
`set` and `mode_scope` are populated only for executable changes. `target` is populated only for
symlink changes. Move reports `source` and `destination`.
`requested` counts selected paths; `changed` counts recorded changes.
`applied` and `saved` describe completed actions. Both are false for a no-op, whose `transaction` is null.
Sorted `entries` include path, changed status, before/after existence, entry kinds, Unix modes and original byte length.
Regular-file modes are JSON integers; symlink and absent modes are null.
Entries contain no source bodies or patch hunks. A replacement report also includes
the requested `before` text, `after` text and match count under `replacement`.
`basis` identifies the changed paths' complete starting snapshots, including full modes.

The validation label is `file-snapshots`.
The recorder checks selected content or link targets, entry kinds, existence and regular-file permission modes again under the shared transaction locks before saving.
History apply also checks its recorded project source revision, with the same recognized-file scope and generated-directory exclusions as other transactions.
Undo and redo check affected snapshots and stack order while preserving unrelated changes.
Source-history transitions never write the Git index. Tests cover an affected path staged before planning.
They also retain unrelated staged and unstaged content, a later tracked source edit and a later untracked file.
These checks do not establish syntax, import, dependency, compilation or behavioral correctness after a file deletion.

## Scope and recovery

Paths must use UTF-8 and remain relative to the selected workspace.
Absolute paths, parent traversal, duplicate targets, parent symlinks and paths through `.git` or `.fr-history` cause refusal.
Deletion and move sources must name existing regular files or UTF-8 symlinks. A move destination
must be absent and differ from its source. Executable changes require regular files.
Directories, recursive deletion, binary regular files and missing deletion targets are unsupported.
Explicit paths can name ignored files; scanner filters do not select or exclude file-operation targets.
All selected paths must pass preflight checks before the command records or applies a batch.

The commands use the existing durable journal and full snapshots; they require no Git installation and leave the Git index unchanged.
Handled write failures restore the batch's starting state when recovery succeeds.
An interrupted operation leaves a pending transaction for `fr history recover ID --write`.
Tests interrupt apply, undo and redo after every write in two-file deletion and executable batches, including empty files and special modes, and at the replacement boundary for regular-to-symlink transitions.
The [native history guarantees](../CLI.md#write-guarantees) and their filesystem assumptions apply here too.
Use a reviewed [`history compact`](../CLI.md#fr-history) operation to remove old replay payloads.
Compaction keeps transaction summaries but
permanently gives up replay and patch export for the selected IDs.
Writes can expose intermediate files to concurrent observers and replace file inodes.
Snapshots omit ownership, timestamps, extended attributes and symlink permission bits; deleting files does not remove their directories.

## Patches and verification

`fr history patch ID` exports these operations from recorded snapshots, including reverse patches.
Git modes encode regular (`100644`), owner-executable (`100755`) and symlink (`120000`) entries. Applying a Git patch does not reproduce other recorded Unix permission bits.
History apply/undo/redo preserve complete recorded regular-file modes instead.
See [patch compatibility and checks](git-patches.md) for receiving-workspace validation.

The owner-execute setter has a Lean-anchored model proving the requested bit, preservation of other bits, idempotence and supported Git mode changes.
Further laws establish the requested projected Git mode and preservation of the journal's mode bound.
Shared execution compares 8,258 setter results between Rust and Lean.
Snapshot checks reuse the existing anchored history predicate; [the verification guide](lean-specs.md#existing-kernels) records the proof assumptions and which connections to the implementation remain unchecked.
The file planner, filesystem observation and complete transaction implementation remain outside those proofs.
The history compaction predicate is also Lean anchored. It proves and exhaustively checks that a
record is eligible only when it has detail, is outside the retained stack window, is not pending and
is not planned. Journal serialization, locking, hash collision resistance and durable replacement
remain tested trust boundaries.
The file-move admission predicate is also anchored. Lean proves that admission requires a present
supported source, absent destination and distinct paths. Rust and Lean agree on all sixteen finite
boundary cases. Snapshot transfer, locking and filesystem writes reuse the tested history engine.

The [host recovery contract](host-recovery.md) covers staged snapshot validation and structured transition failures.
