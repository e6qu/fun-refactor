# Give another checkout a saved change

Suppose you saved a repair so `invoice_total(5, 2)` returns `7` and want to
apply it in another checkout. A patch is a file containing proposed changes.
The receiving checkout is where you apply it.

Export the saved change and run a dry run, which checks application without
writing files. A conflict means the receiver no longer matches the patch's
assumptions. After applying the patch, run the receiving project's invoice tests.
Successful application establishes that the text fits; the tests check behavior.

## Commands, results and limits

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
Journal validation refuses unsafe parent symlinks. A selected leaf may be a recorded symlink.

The receiving files must match the recorded starting state closely enough for Git to apply the patch.
Export does not check the receiving workspace or promise that a patch will apply there.
Use the basis check below to compare complete affected-file snapshots before exporting.
`git apply --check` checks whether Git can apply the patch without changing files.
Git can accept contextual patches despite differences outside the hunks; this is not an exact snapshot check.
Use `fr history apply 1 --write` for the transaction engine's exact basis check in the original workspace.
Applying a patch with Git does not update `fr` history.

Illustration: the receiving checkout has a different comment outside the patch's changed lines.
Git may still apply the patch, while the complete-file snapshot check reports a mismatch.
Read which check passed, inspect the receiving changes and run the behavior test there.

Export the inverse change with `fr history patch 1 --reverse`.
It starts from the recorded result, regardless of the current history status.
Alternatively, Git can reverse the forward patch with `git apply --reverse`.

For agent consumption, `fr --json history patch 1` returns metadata and a `patch` string.
To keep a large patch out of agent context, write a new artifact directly:

```sh
fr history patch 1 --output ../artifacts/change.patch
```

This mode returns JSON metadata with `patch_bytes`, `patch_sha256` and the requested `output` path, omits the patch string, and refuses to replace an existing path.
The `transactions` array lists each selected edit's ID, status, `record_basis`,
`source_revision` and `validation` label. Direction, file count and format describe the complete patch.
The outer `record_basis` identifies the combined starting snapshots, even in a reverse export.
It is a history digest, not a Git object ID or a claim about the receiving files.
The per-edit revision and validation describe past checks; export does not rerun them.
This information has a context cost. A basis report compares receiving files with
saved snapshots. On the fixed six-statement Rust example, that one-edit report grows
from 689 to 954 UTF-8 bytes. It lists each edit alongside the combined starting files. The [earlier measurement](../tests/agent-eval/semantic-delta.json)
stays intact beside the [current measurement](../tests/agent-eval/semantic-delta-delivery.json).
Both editing methods still produce the same source and pass behavior, undo and redo
checks. This compares metadata responses from the earlier and current fr, with one
edit in each. The current measurement uses the
[qualified CI-built executable](https://github.com/e6qu/fun-refactor/actions/runs/41516510047).
CI repeats the fixture with its current checker. These counts measure neither
program runtime nor agent success.

The [four-route comparison](../tests/agent-eval/semantic-intent-delivery.json) also
records 265 more response bytes per route. Each route changes `value + 1` to
`value + 7`, using a full function body, an expression edit, a direct request or
a Python-generated request. Their source and behavior checks still agree.
The [earlier counts](../tests/agent-eval/semantic-intent.json) stay intact.
The extra bytes all come from the same patch metadata report.

## Deliver several saved edits together

Suppose edit 4 saved an algorithm's termination proof and edit 7 saved its correctness
proof in the same file. Both belong in the delivered patch:

```sh
fr history patch 4 7 --output change.patch
fr history patch 4 7 --check --against /path/to/receiver
fr history patch 4 7 --reverse --git-check --against /path/to/completed/receiver
```

These IDs select exactly two edits. They do not include edits 5 and 6 implicitly.
For each shared file, the second input must exactly match the first result,
including its kind and permissions. If edit 5 changed that file, omitting it
refuses when edit 7 expects its result. Unrelated files need no intervening edit.
Each file appears once in the patch, from its first selected input to its final result.
A temporary file created and then removed contributes no final change.
Repeated IDs, conflicting order, unavailable snapshots and an entirely cancelled
selection refuse. Current working files are never used to fill a gap.
Run the requested project checks on the combined result before delivering it.

## Check a receiving workspace

```sh
fr history patch 1 --check
fr history patch 1 --check --against /path/to/receiving/workspace
fr history patch 1 --check --against /path/to/receiving/workspace --reverse
```

UTF-8 encodes text as bytes. For example, `é` occupies two bytes, so byte limits can differ from character counts.

`--check` prints a JSON report in both output modes, with no patch or stored source text.
It compares every affected entry's complete UTF-8 content or link target, existence, kind and Git mode with the starting snapshots.
The exit code is zero when all files match that basis, and one when any file differs.
Each sorted file row reports content and Git mode matches, expected/actual existence and kinds, and recorded/observed Unix permission modes.
`matches_recorded_snapshot` also requires all recorded permission bits to match.
The report totals files in `checked_files` and aggregates both comparisons.
A permission difference outside Git's executable distinction can pass `matches_patch_basis` while failing `matches_recorded_snapshots`.

The receiving directory defaults to the history workspace.
`--against` requires a check mode; relative receiving paths resolve from the process working directory.
The command canonicalizes that directory and does not require history or Git there.
The source history stays under `-C`, including when the receiving directory differs.
`--reverse` checks the recorded result as the starting state; `record_basis` still names the original before snapshots.

Checks read affected paths and leave source, history, Git state and unrelated files unchanged.
They refuse parent symlinks, directories, unreadable/non-UTF-8 entries and unsupported exports.
These errors fail the command before it prints a report; `--json` uses the standard error object.

This command checks complete starting snapshots, independently of Git's contextual hunk matching.
It does not run Git, inspect the index, evaluate Git attributes or check whether a later write can succeed.
It does not check the transaction's project source digest or freeze the files against concurrent changes.
Use `git apply --check` for Git application rules, or history apply for transaction validation and writes.
The full-permission comparison reuses the anchored history snapshot predicate.
Patch-basis equality and Git mode projection now use separate anchored helpers with Lean models and shared execution cases.
See [patch verification evidence](lean-specs.md#existing-kernels) for the model laws and compiler-trust assumptions.
Filesystem observation and report aggregation remain outside the model; agreement with the complete Rust implementation remains unproved.

## Ask Git to check application

```sh
fr history patch 1 --git-check
fr history patch 1 --git-check --against /path/to/receiving/workspace
fr history patch 1 --git-check --index --reverse
```

`--git-check` runs Git's application check and prints a JSON report in both output modes.
It requires Git and a receiving directory inside a Git working tree.
Export and `--check` continue to work without Git.
Choose either `--check` or `--git-check`; `--index` requires `--git-check`.

The default scope is `worktree`.
Adding `--index` selects `index-and-worktree` and requires matching index entries and working files for affected paths.
Git's documented [`--check` and `--index` rules](https://git-scm.com/docs/git-apply) govern that verdict.
`applicable` reflects Git's exit status; the CLI exits zero for success and one for a failed check.
The report includes `git_exit_code`, direction, transaction identity, file count, receiving root and repository root.
`stdout` and `stderr` retain the first 16 KiB of each Git stream, with `diagnostics_truncated` indicating omitted bytes.
Git diagnostics can contain source excerpts. The report does not include the patch.
Setup errors use the standard error response instead of an application verdict.

Checks run from the repository root and prefix patch paths for nested receiving directories.
This avoids Git's subdirectory filtering silently skipping affected paths.
Linked worktrees are supported. The command preserves the worktree, history, index and Git worktree pointer.

The `configuration` field identifies the selected configuration scope.
Git reads repository configuration, including its includes, and repository attributes.
The command clears inherited `GIT_*` overrides, disables system/global configuration and ignores external attributes files.
It also disables filesystem-monitor hooks, optional index locks and demand fetching of missing objects.
This makes the selected receiving directory control repository discovery.
Results can differ from a Git command using user or system settings.

Git's [content conversion path](https://github.com/git/git/blob/master/apply.c) can invoke configured filters while checking a file.
The command refuses affected paths with content filters before running the application check.
Configured drivers named `unset` or `unspecified` also cause refusal because their names overlap Git's attribute-report states.
Other filter drivers on unrelated paths do not prevent a check.
Affected regular files and UTF-8 symlinks are supported. Directories remain unsupported.

Checks observe current state and do not freeze files or configuration against concurrent changes.
A successful Git verdict does not establish exact agreement between saved file states, write permissions or project validation.
Use the snapshot check when complete starting content or link targets, entry kinds and Git modes must match.
Git execution and attribute handling have regression tests, without a Lean correspondence proof.

## Supported scope

The exporter handles UTF-8 text modifications, additions and deletions represented in history.
It preserves content bytes, including CRLF and missing final newlines.
Empty file creation and deletion, multiple hunks and unusual UTF-8 filenames are supported.
Paths use Git quoting, and output is sorted by path.
Recorded deletion/addition pairs express moves without rename detection.
Use [`fr file delete`, `fr file move`, `fr file executable` and `fr file symlink`](file-transactions.md)
to record deletion, moves, owner-execute and link changes.
Use [`fr file move`](file-transactions.md) to bind both endpoints to one checked transaction.

Git mode output distinguishes regular files (`100644`), executable files (`100755`) and symlinks (`120000`), using the owner execute bit for regular files.
Other Unix permissions are not reproduced. New files use Git modes rather than private history creation permissions.
An existing file's permission change is accepted only when it changes executable bits and toggles the owner execute bit.
Changes to other permission bits are refused, including when accompanied by content changes.
Use history apply/undo/redo when full recorded permissions must be restored.

Binary snapshots containing NUL, non-UTF-8 regular contents or link targets, and submodules are outside this text patch scope.
An unsupported change fails the entire export before any patch is printed.
The exporter does not include blob IDs, binary hunks or three-way merge support.
[Git status pages](git-status.md) report repository changes independently of history.
[Git administration](git-staging.md) covers current staging, commits and owned worktree workflows.

The format follows Git's [patch format documentation](https://git-scm.com/docs/diff-format).
Application behavior is described in [git apply](https://git-scm.com/docs/git-apply).
Tests run Git checks and forward/reverse applications against recorded contents, link targets and regular, executable and symlink modes.
They also cover conflicting files and preservation of unrelated staged, unstaged and untracked changes.
These tests provide compatibility evidence; patch rendering has no Lean correspondence proof yet.
The mode-change acceptance helper also has an anchored Lean model covering 32-bit masks and forward/reverse symmetry.

Git inspection requires support for [`--no-lazy-fetch`](https://git-scm.com/docs/git); older Git versions refuse the command.
