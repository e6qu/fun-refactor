# Recorded transaction patches

Export a saved transaction from the workspace root:

```sh
fr history patch 1 > /tmp/change.patch
git apply --check /tmp/change.patch
git apply /tmp/change.patch
```

The command writes only the patch to stdout. Errors go to stderr.
It renders stored snapshots and requires neither Git nor a Git repository.
It does not modify source files, history, the index or repository configuration.
Export stays the same when a plan becomes applied, undone or abandoned, or when working files change.
Journal validation still refuses unsafe paths, including current symlinks.

The receiving files must match the recorded starting state closely enough for Git to apply the patch.
Export does not check the receiving workspace or promise that a patch will apply there.
`git apply --check` performs that check without applying changes.
Git can accept contextual patches despite differences outside the hunks; this is not an exact snapshot check.
Use `fr history apply 1 --write` for the transaction engine's exact basis check in the original workspace.
Applying a patch with Git does not update `fr` history.

Export the inverse change with `fr history patch 1 --reverse`.
It starts from the recorded result, regardless of the current history status.
Alternatively, Git can reverse the forward patch with `git apply --reverse`.

For agent consumption, `fr --json history patch 1` returns metadata and a `patch` string.
Fields include transaction identity, status, validation label, direction, file count and format.
`record_basis` always identifies the original transaction's before snapshots, even in a reverse export.
It is a history digest, not a Git object ID or a claim about the receiving files.
`source_revision` and `validation` describe the recorded transaction; export does not rerun validation.

## Supported scope

The exporter handles UTF-8 text modifications, additions and deletions represented in history.
It preserves content bytes, including CRLF and missing final newlines.
Empty file creation and deletion, multiple hunks and unusual UTF-8 filenames are supported.
Paths use Git quoting, and output is sorted by path.
Recorded deletion/addition pairs can express moves without rename detection.
This command does not add deletion or move operations to the transaction recorder.

Git mode output distinguishes regular files (`100644`) from executable files (`100755`), using the owner execute bit.
Other Unix permissions are not reproduced. New files use Git modes rather than private history creation permissions.
An existing file's permission change is accepted only when it changes executable bits and toggles the owner execute bit.
Changes to other permission bits are refused, including when accompanied by content changes.
Use history apply/undo/redo when full recorded permissions must be restored.

Binary snapshots containing NUL, non-UTF-8 source, symlinks and submodules are outside this text patch scope.
An unsupported change fails the entire export before any patch is printed.
The exporter does not include blob IDs, binary hunks or three-way merge support.
Git status summaries, staging, commits and worktree workflows remain roadmap work.

The format follows Git's [patch format documentation](https://git-scm.com/docs/diff-format).
Application behavior is described in [git apply](https://git-scm.com/docs/git-apply).
Tests run Git checks and forward/reverse applications against recorded contents and executable modes.
They also cover conflicting files and preservation of unrelated staged, unstaged and untracked changes.
These tests provide compatibility evidence; patch rendering has no Lean correspondence proof yet.
