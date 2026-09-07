# Reviewed Git commits

```sh
fr git changes --staged
fr git commit -m "Describe the change"
fr git commit -m "Describe the change" --basis TOKEN --write
```

`fr git commit` previews a commit of the entire index and prints JSON in both output modes.
The preview reports `operation: "commit-preview"` and `applied: false`.
Pass its basis back with the same message and `--write` to create and publish the reviewed commit.
A write requires a basis. This command is currently available on Unix.

## Scope and review

The commit includes all staged entries, including staging created outside `fr`.
It preserves working-file bytes, untracked files, index bytes and completed staging history.
It supports initial commits and ordinary single-parent commits on attached local branches, including linked worktrees and SHA-256 repositories.
It supports indexed regular files, executable files, binary blobs and symlink blobs without reading their working replacements.

`head` identifies the branch and nullable parent commit. `tree` identifies the complete proposed tree.
`index_paths` counts the complete inventory. `changed_paths` contains a bounded list of changed names without source bodies or rename pairing.
`--limit` defaults to 20 and accepts 1 through 500. `page` reports total, returned and omitted path counts.
The limit affects presentation, not the basis or committed tree.
Use `fr git changes --staged` for separate paginated inspection when the changed-path list is incomplete.

The basis binds the repository, index location, tool version, complete index bytes, branch, parent, tree, normalized message and author/committer identities.
Even a stat-cache or index-format change invalidates it. Working-file edits alone do not.
Checks cannot detect states changed and fully restored between observations.

Messages require nonempty UTF-8 text without NUL, at most 16 KiB before normalization.
A missing final newline is added. Other whitespace and message contents remain unchanged; there is no editor, template or cleanup pass.
The preview displays the normalized message and identities.
Git timestamps are chosen at write time and are outside the basis; a preview therefore has no predetermined commit object ID.

## Configuration and restrictions

Author and committer names and email addresses come from repository-local Git configuration, including configured includes.
System/global configuration and inherited Git environment overrides remain disabled by the shared runner.
Configure the repository's `user.name` and `user.email`, or its author/committer settings, before using this command.
Identity autodetection is disabled. Names and addresses are rechecked before publication.

The command disables Git hooks, signing, replacement-object interpretation, content conversion, optional index refreshes and demand fetching.
The report names disabled hooks and signing explicitly. It does not run project validation commands.
Already-indexed content is committed as stored, so configured working-file filters do not rewrite the proposal.

Detached HEAD, symbolic branch chains, submodules, non-UTF-8 index paths and unmerged entries cause refusal.
Selected assume-unchanged, skip-worktree and intent-to-add entries remain unsupported; every index path participates in this selection.
Writes also retain the index-lock restrictions on split indexes, sparse checkouts and nonregular indexes.
Active merge, cherry-pick, revert, rebase or sequencer state causes refusal.
Pending or corrupt staging history must be resolved before committing.
Empty commits and empty initial repositories are unsupported. Deleting every tracked file from an existing commit is supported.

## Preparation and publication

Preview builds a separate index from captured inventory rows and calculates its tree in a temporary object directory.
It validates referenced blobs in the real object database. Preview leaves repository objects, refs, reflogs, index and working files unchanged.
Temporary preparation files are discarded normally; process termination can leave temporary files behind.
The implementation uses Git's [tree plumbing](https://git-scm.com/docs/git-write-tree), with missing-object checks disabled only inside the temporary preview object store.

Writes acquire the worktree's `index.lock`, verify the basis and materialize the reviewed tree and commit object.
The created object's tree, parent, message and identities are checked before publication.
Git's [reference transaction](https://git-scm.com/docs/git-update-ref) prepares an update through HEAD with the expected old commit.
That preparation locks HEAD and its referent. While Git holds those locks, `fr` rechecks the branch, parent, index, identities, operation markers and journal.
Only then does it request transaction commit. An error before that request closes the transaction without publication and releases its locks.
Existing locks remain the responsibility of their owners.

On successful publication, `commit` reports the new object ID and `applied` is true.
`reference_update` distinguishes an acknowledgment from a subsequent branch observation:

| Field | Meaning |
|---|---|
| `confirmed` | True when publication is confirmed; null when it remains uncertain |
| `branch` | Reviewed branch |
| `expected_parent` | Reviewed old commit, or null for an initial commit |
| `acknowledged` | Whether Git emitted its commit acknowledgment |
| `observed_commit` | Branch identity observed when the acknowledgment was missing; otherwise null |
| `warning` | Explanation of an incomplete or unconfirmed Git result |
| `diagnostic` | Bounded Git stderr for an incomplete result |

If Git fails after publication, an acknowledgment or observation of the new commit can still confirm `applied: true`, accompanied by a warning.
If publication cannot be confirmed, the report has `applied: null`, the candidate commit ID and a warning.
These completed reports can have exit status zero. Agents must inspect `applied`; exit status alone does not confirm publication.
Inspect the branch and reflog before retrying an uncertain result. Changes fully restored between observations remain undetectable.
Failures during preparation can leave unreachable tree or commit objects for Git to collect.

There is no `fr` commit journal, signing workflow, merge/amend support, push, or automatic Git-history reversal.
Source-history and staging-history undo do not remove a published commit. Staging undo after a commit changes the index relative to the new HEAD.
Reference persistence follows Git and the host filesystem's guarantees; `fr` does not claim a separate crash-durability protocol.
Keep repository configuration and directory topology stable. Writers bypassing Git's locks remain outside the cooperation guarantee.

## Formal coverage

The [Git Lean model](../kernels/FrKernels/Git.lean) anchors the branch/parent acceptance predicate.
It proves rejection of branch switches and parent drift, with shared Rust/Lean cases covering equal and different branches, unborn states and existing parents.
Abstract publication laws preserve the index and unrelated refs under the accepted basis.
These laws assume correct captured identities and Git's prepared-reference locking behavior.
Object interpretation, subprocess I/O, lock execution, publication reconciliation, crash durability and complete Rust correspondence remain regression-tested rather than fully proved.
