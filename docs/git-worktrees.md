# Worktree inspection

`fr git worktree list` discovers registered workspaces before an agent chooses where to work.
It returns JSON in either output mode.

```sh
fr git worktree list --limit 10
fr git worktree list --limit 10 --cursor TOKEN
fr -C ../task-worktree git worktree list
```

Each row includes the absolute registered path, full HEAD object ID and full local branch reference.
Detached worktrees have no branch. Unborn branches have `unborn: true` and a null HEAD.
The main entry comes first; linked entries follow Git's order.
`main` identifies the primary registration and `current` identifies the invoking working tree.
Invocation from a subdirectory or a symlink resolves to that working tree.
`common_directory` identifies the shared repository metadata directory.
A linked checkout can report a bare primary repository. Direct invocation from a bare repository refuses.

`locked` and `prunable` describe Git's registration metadata.
Optional reasons occupy at most 512 UTF-8 bytes each, with separate truncation flags.
Missing reasons are null; empty reasons remain empty strings.
Paths and reasons preserve spaces and newlines through NUL-delimited parsing.
Non-UTF-8 metadata, unknown attributes and incomplete or contradictory records refuse with a diagnostic.

The command uses Git's `worktree list --porcelain -z --expire=now` format.
Git annotates eligible missing registrations immediately under that expiry policy.
This command does not prune, repair, unlock, create or remove worktrees.
See the [Git worktree manual](https://git-scm.com/docs/git-worktree.html) for registration and lock semantics.

## Page identity

`--limit` accepts 1 through 500 and defaults to 20.
Counts cover all registrations; `page` reports returned and omitted rows plus the next cursor.
The revision binds the canonical invoking root, common directory and complete raw registration output.
Changes to paths, HEADs, branches, lock reasons or pruning annotations invalidate an earlier cursor.
This includes bytes beyond the displayed reason limit and rows outside the current page.
Changing the page limit preserves the cursor identity.
A cursor from another invoking worktree refuses, even when both share the same repository.

## Observation limits

This is a registration observation, with `content_inspected: false` and `atomic_snapshot: false`.
It does not assess dirty files, index contents, conflicts, build health or branch availability for a later write.
Working files and staged edits do not affect its revision.
Use `fr -C PATH git status` to inspect a selected checkout's changes.
The list has no repository lock: concurrent Git operations can change metadata during or after the observation.
A list cursor supplies no authorization or creation basis.

Inspection uses the guarded Git runner, disabling hooks, filesystem monitors and inherited Git environment overrides.
It preserves working files, indexes, references and worktree registrations.
It reads repository-local configuration but does not inspect file content or invoke content filters.
Git still reads all registrations to construct a page; output bounds do not imply constant memory use.

Pagination reuses the source-anchored `page_length` kernel and its Lean bound and progress laws.
The parser, Git subprocess and complete observation protocol have tests, without a general formal correspondence proof.

`fr git worktree create` previews and applies a new branch and raw checkout with a separate review basis.
See [reviewed creation](git-worktree-creation.md) for supported destinations, checkout limits and partial-failure reporting.
