# Reviewed worktree creation

Create a separate workspace from a committed snapshot while retaining the source worktree's files and index.
Creation is available on Unix and prints JSON in both output modes.

```sh
fr git worktree create ../task --branch agent/task
fr git worktree create ../task --branch agent/task --basis TOKEN --write
```

For a new branch, choose an explicit start revision with `--from REVISION`; the default is `HEAD`.
Use `--existing-branch NAME` instead of `--branch` to attach an unused local branch at its own tip.
The preview resolves it to a full commit ID and reports the tree, destination, branch and checkout size.
File rows contain paths, modes, blob IDs and sizes. They omit bodies.
`--limit` controls these rows, from 1 through 500, with a default of 20.
The page reports total, returned and omitted counts. Creation previews have no continuation cursor.

## Destination and branch

The destination must be absent, including any empty directory, regular file or dangling symlink.
Its parent must already exist outside Git repositories.
Relative destinations resolve from the invoking repository root, including invocation from a subdirectory.
The report exposes the canonical parent plus the final directory name; parent symlinks resolve during preview.
Directory identity checks bind that parent and the shared Git directory.
Destinations overlapping registered worktrees or the shared metadata directory refuse.
This includes missing registrations that Git considers prunable.

`--branch` names a new local branch. Existing branches, symbolic references and occupied unborn branch names refuse.
New-branch mode never resets or reuses a branch and disables automatic upstream tracking.
[Existing-branch mode](git-worktree-existing-branches.md) preserves the selected branch and its configuration.
Git performs its own branch and worktree checks during registration.
Successful creation retains a Git worktree lock with the reason `fr: reviewed raw worktree`.
The lock protects registration from ordinary pruning, removal and movement; it does not prevent edits or commits.

## Review basis

The creation basis covers the invoking root, shared metadata location and identity, destination and parent identity.
It records the repository-local `extensions.worktreeConfig` mode, and the result reports that mode as `worktree_config`.
It also covers the branch mode, branch name, start revision, resolved commit, tree and complete file inventory.
All worktree registration bytes participate, including registrations outside any displayed page.
Changing the row limit preserves the basis. Changes to working files and staged content also preserve it.
Creation always copies the selected committed tree.

`--write` requires the preview token. The writer captures the selected blobs and repeats the proposal checks before creating a directory.
It atomically publishes a bounded `fr-worktree-creation-*.json` preparation in the shared Git directory before the first destination or registration mutation.
That record contains the reviewed proposal and identifies the destination; it omits source bodies.
New-branch mode passes the pinned commit to Git. Existing-branch mode holds a verification lease on the reviewed branch tip.
Both modes install the raw index and files from the pinned commit.
No preview reserves the destination or branch. Concurrent Git or filesystem changes can still cause refusal or partial creation.

## Raw checkout

The writer registers the worktree with Git's `--no-checkout` mode and builds a separate index.
It records ownership after registration checks, prepares the index privately and installs it without replacing an existing index.
It holds Git's index lock through checkout verification and ownership receipt completion.
It creates each file exclusively, copies committed bytes and preserves Git executable modes.
Binary regular files and committed empty trees are supported, including SHA-256 repositories.
New workspace directories have mode `0700`; regular files have `0644` or `0755`.
Inherited Git environment overrides, hooks and filesystem monitors are disabled.
Replacement objects are disabled, and content filters never run.

This raw checkout bypasses line-ending conversion, encodings, ident expansion and smudge filters.
Attribute-dependent Git commands may later report differences or refuse under their own filter policy.
An LFS pointer remains a pointer. The command does not fetch remote content or initialize submodules.

The raw checkout accepts UTF-8 paths and regular blobs only.
Symlinks, submodules, unsafe `.git` path components and paths that collide under case folding refuse.
`fr` accepts repositories with `extensions.worktreeConfig` enabled. Creation does not add a per-worktree configuration file; later Git commands may create `config.worktree` in the linked worktree's private metadata.
The committed tree must fit 20,000 files, 256 MiB total and 32 MiB per blob.
These are checkout payload limits. Git metadata collection and temporary blob copies can use additional memory.
Other filesystem naming restrictions can still cause a failure during materialization.

## Completion and partial failures

Only `applied: true` confirms that creation passed the file, index, branch, commit and directory checks.
Previews report `applied: false`. Read-only preflight failures use the ordinary CLI error response.
Once the mutation phase starts, a failure reports `applied: null`, a bounded diagnostic and inspection guidance.
It also reports the observed branch commit and destination presence when those observations succeed.
These JSON outcomes exit successfully; agents must inspect `applied`.

A failed operation can leave a branch, a locked registration, a partial index or an incomplete checkout.
`fr` does not recursively remove the destination or delete a branch after failure.
Git may perform its own cleanup if registration fails internally.
Inspect before retrying:

```sh
fr git worktree list
fr -C ../task git status
```

The second command requires a usable checkout and follows the status command's filter policy.
After successful creation, ordinary project inspection, edits and reviewed commits can run with `-C ../task`.
Use `git worktree unlock ../task` when you intend to allow ordinary worktree removal or movement.

Creation now records ownership receipts for checked forward recovery of incomplete registered checkouts.
After receipt publication, creation removes its preparation record before installing the index and files.
Successful creation leaves no preparation record. A partial result reports `preparation_record` when durable evidence remains.
See [recorded worktree recovery](git-worktree-recovery.md) for receipt scope, refusals and crash limits.
[Reviewed removal](git-worktree-removal.md) uses completed receipts and retains the branch. Worktree undo/redo remains pending.
File and directory synchronization does not establish an atomic crash transaction across Git refs, registrations and checkout files.
Directory and file checks detect observed replacements; they do not protect against every hostile concurrent filesystem race.

## Formal coverage

The source-anchored `worktree_budget_allows` kernel has Lean proofs for accepted limits, empty trees and smaller payloads.
A second anchored predicate requires the repository configuration mode to match the reviewed mode and any observed per-worktree configuration path to be a regular file.
Lean proves both requirements, and all sixteen boolean states agree with Rust.
Shared Rust/Lean cases exercise the limits and machine-size boundaries.
Abstract fresh-destination laws prove refusal of occupied entries and preservation of unrelated entries.
Those laws assume a disjoint namespace and an atomic installation step; the host workflow has multiple steps.
Git subprocess behavior, filesystem races, blob identity and crash recovery remain outside complete correspondence proofs.
CLI tests cover stale bases, raw modes and bytes, linked worktrees, refusals and injected partial failures.
