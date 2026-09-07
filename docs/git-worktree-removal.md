# Reviewed worktree removal

`fr git worktree remove PATH` previews removal of an owned, clean linked worktree on Unix.
It retains the branch and shared repository data, and archives the private worktree metadata before deleting files.

```sh
fr git worktree remove ../task --limit 10
fr git worktree remove ../task --basis TOKEN --write
```

Relative paths resolve from the invoking repository root. The command returns JSON in both output modes.
The invoking worktree and its ancestors cannot be removed.
Only worktrees with completed [creation receipts](git-worktree-recovery.md) are eligible.
The target must remain in the same shared repository, with its recorded directory identities and `.git` link intact.
A receipt is local ownership evidence, not a signed attestation.

## Accepted state

The target must be attached to its original, direct local branch with its registration lock still present.
Later commits on that branch are supported. Removal reviews the current commit and retains the branch at that commit.
The entire checkout must match the current commit's raw blobs and executable bits.
All files must be present. Extra files and directories refuse, including ignored paths and local history directories.
The regular-file, path and payload restrictions from [raw creation](git-worktree-creation.md) apply.

The index must match the committed inventory with plain entries.
Staged changes, conflicts, intent-to-add, assume-unchanged and skip-worktree entries refuse.
Active Git operations, existing locks and unknown private metadata also refuse.
The supported private files are `HEAD`, `index`, `commondir`, `gitdir`, `locked`, `fr-creation.json`, `logs/HEAD`, `COMMIT_EDITMSG` and `ORIG_HEAD`.
Private file contents are limited to 16 MiB in total. An empty private `refs` directory is accepted. Private refs, other logs, staging journals and additional files require manual inspection.
Only the files reference backend is supported. Per-worktree configuration must be disabled.

## Review and deletion

The preview includes the retained branch, commit, tree and bounded file metadata.
`--limit` accepts 1 through 500 and defaults to 20. Omitted paths are counted; there is no continuation cursor.
The basis binds ownership, registrations, current commit, checkout identities and contents, index bytes and all accepted private metadata.
Changing the row limit preserves the basis. Changes to the observation require a new preview.
Source bodies and private metadata bytes stay out of the preview.

The writer acquires owned receipt, index and private HEAD locks. An archive lease coordinates removal with later resumption.
A prepared, verify-only [Git reference transaction](https://git-scm.com/docs/git-update-ref) holds the branch while removal runs.
Closing that transaction releases its locks without updating the branch or reflog.
Hooks, content filters, replacement objects and inherited Git environment overrides remain disabled or bypassed.

Before deleting anything, the writer saves `fr-worktree-removal-*/record.json` under the shared Git directory.
This private archive contains the proposal, receipt, snapshot and exact bytes of supported private metadata, including the index and Git link.
The source bytes remain available in the retained commit. The archive directory and record are synchronized before deletion starts.

Each unlink checks the reviewed file's identity, bytes and complete Unix mode again.
Parent directory identities are checked before accessing selected files.
Directories are removed only when empty; cleanup never recursively removes a destination.
Unexpected content that arrives after review can stop cleanup and remains for inspection.
The target directory is removed before its private registration metadata. Shared refs and the invoking index receive no writes.
Git describes this private registration layout in its [worktree documentation](https://git-scm.com/docs/git-worktree).

## Outcomes and recovery limits

Only `applied: true` confirms completion. A successful removal archive gains a synchronized `complete` marker.
Previews report `applied: false`. Preflight failures use the usual CLI error response.
Failures after archiving report `applied: null`, a bounded diagnostic and `removal_record`.
These JSON outcomes exit successfully, so agents must inspect `applied`.

A failure can leave missing committed files, empty directories or partially removed private metadata.
The archive retains the reviewed metadata and identifies the retained source commit for inspection, checked resumption or manual reconstruction.
Absence of a `complete` marker requires inspection; it does not establish which deletions occurred.
[Removal inspection and checked resumption](git-worktree-removal-resumption.md) accept missing paths and matching survivors. Worktree undo/redo remains pending.
Creation recovery refuses completed receipts and must not be used to reverse partial removal.
Archives currently have no retention or compaction command.

Locks coordinate cooperating Git and `fr` writers. Existing and replacement locks are preserved.
Crashes can leave locks requiring manual ownership review. Readers do not take these locks.
Filesystem observations and unlinks are separate operations; hostile concurrent path replacement is outside complete race protection.
Removal is not an atomic transaction across checkout files, registration metadata and its archive.

## Formal coverage

The anchored deletion predicate requires matching identity, bytes and mode; Lean proves all three are necessary.
A separate abstract namespace model proves that removing selected paths preserves unselected paths.
Shared Rust/Lean cases cover every boolean predicate input.
These results do not prove correspondence for filesystem observation, unlinking, Git transactions or crash durability.
