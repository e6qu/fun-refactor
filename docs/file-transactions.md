# File transactions

Use explicit paths relative to the workspace selected by `-C`:

```sh
fr -C /project file delete obsolete.txt empty.txt
fr -C /project file delete obsolete.txt empty.txt --save-plan
fr -C /project history patch 1
fr -C /project history apply 1 --write
fr -C /project history undo 1 --write
fr -C /project history redo 1 --write
```

`fr file delete` removes complete regular text files. Empty files count as deletions.
It does not inspect references or determine whether the project still builds.
Use the existing symbol-level `fr delete` when its reference checks match the intended change.

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

Each command accepts 1 through 500 explicit paths and reports JSON in both output modes.
The default only previews. `--save-plan` records a plan; `--write` records and applies it.
Those two flags are mutually exclusive.
Use the returned transaction ID for history inspection, application, undo, redo, recovery and patch export.
Running a fresh command after a preview observes the files again; applying a saved plan checks its recorded basis.

The report includes `schema: 1`, `workspace_root`, `operation`, `set`, `target`, `mode_scope`, `validation`, `basis` and `transaction`.
`set` and `mode_scope` are populated only for executable changes. `target` is populated only for symlink changes.
`requested` counts selected paths; `changed` counts recorded changes.
`applied` and `saved` describe completed actions. Both are false for a no-op, whose `transaction` is null.
Sorted `entries` include path, changed status, before/after existence, entry kinds, Unix modes and original byte length.
Regular-file modes are JSON integers; symlink and absent modes are null. Reports contain no source bodies or patch hunks.
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
Deletion selections must name existing regular files or UTF-8 symlinks. Executable changes require regular files.
Directories, recursive deletion, binary regular files and missing deletion targets are unsupported.
Explicit paths can name ignored files; scanner filters do not select or exclude file-operation targets.
All selected paths must pass preflight checks before the command records or applies a batch.

The commands use the existing durable journal and full snapshots; they require no Git installation and leave the Git index unchanged.
Handled write failures restore the batch's starting state when recovery succeeds.
An interrupted operation leaves a pending transaction for `fr history recover ID --write`.
Tests interrupt apply, undo and redo after every write in two-file deletion and executable batches, including empty files and special modes, and at the replacement boundary for regular-to-symlink transitions.
The [native history guarantees](../CLI.md#write-guarantees) and their filesystem assumptions apply here too.
Writes can expose intermediate files to concurrent observers and replace file inodes.
Snapshots omit ownership, timestamps, extended attributes and symlink permission bits; deleting files does not remove their directories.

## Patches and verification

`fr history patch ID` exports both operations from recorded snapshots, including reverse patches.
Git modes encode regular (`100644`), owner-executable (`100755`) and symlink (`120000`) entries. Applying a Git patch does not reproduce other recorded Unix permission bits.
History apply/undo/redo preserve complete recorded regular-file modes instead.
See [patch compatibility and checks](git-patches.md) for receiving-workspace validation.

The owner-execute setter has a Lean-anchored model proving the requested bit, preservation of other bits, idempotence and supported Git mode changes.
Further laws establish the requested projected Git mode and preservation of the journal's mode bound.
Shared execution compares 8,258 setter results between Rust and Lean.
Snapshot checks reuse the existing anchored history predicate; [the verification guide](lean-specs.md#existing-kernels) records proof assumptions and correspondence limits.
The file planner, filesystem observation and complete transaction implementation remain outside those proofs.
