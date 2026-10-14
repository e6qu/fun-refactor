# Continue an existing branch in a separate folder

If the checkout fix already has a branch, open that branch in a new working folder.
Review which branch and commit the folder will use. The command cannot attach a branch that another worktree already uses.

## Commands, results and limits

`fr git worktree create PATH --existing-branch NAME` previews a fresh raw checkout of an unused local branch on Unix.
It preserves the branch tip, branch reflog and upstream configuration.

```sh
fr git worktree create ../task --existing-branch agent/task
fr git worktree create ../task --existing-branch agent/task --basis TOKEN --write
```

Use `--branch NAME` to create a new branch instead. The two options are mutually exclusive.
`--from` applies only to new branches; existing branches always use their own reviewed tip.
The preview reports `branch_action: retain`, a full local reference in `from`, the pinned commit and bounded file metadata.
New-branch previews report `branch_action: create`.

## Accepted branches and destinations

The branch must be a literal, unambiguous local name backed by a direct reference to a commit.
Symbolic branches, missing branches, ambiguous short names and branches occupied by registered worktrees refuse.
Missing or prunable worktrees still reserve their branches until their registrations are resolved.
A remote-tracking reference alone does not count as an existing local branch.
No remote branch guessing, fetching, branch reset or forced checkout occurs.

The destination and raw file restrictions from [reviewed creation](git-worktree-creation.md) apply unchanged.
The destination must be fresh, with an existing parent outside Git repositories.
Files come from the reviewed commit, including when the invoking worktree has staged, unstaged or untracked changes.
Packed refs, SHA-256 repositories and invocation from another linked worktree are supported.
Regular files and supported symbolic links use the same checkout rules as new-branch creation.
Submodules remain unsupported. Repository worktree configuration uses the same lifecycle checks too.

## Review and reference preservation

The basis includes the branch mode, branch tip, destination, inventory and complete registration observation.
A moved branch tip or changed registration requires another preview.
Changing `--limit` preserves the basis; file bodies remain outside the report.

Before creating a directory, the writer acquires a prepared, verify-only transaction on the existing branch.
It repeats the proposal checks while holding that lease, then registers the worktree using the branch name.
The raw index and file installation still use the pinned commit.
The lease prevents cooperating Git reference writers from moving the branch during creation.
Closing it releases its locks without updating the branch or reflog.
Existing foreign locks cause refusal and remain untouched.

Git also checks branch occupancy during registration. Its [worktree documentation](https://git-scm.com/docs/git-worktree) describes that default refusal.
The writer checks the resulting branch, commit and registration, including that the existing branch has only the intended worktree.
The branch lease does not atomically reserve a worktree registration against all concurrent filesystem or Git operations.
Observed registration races can still produce partial creation; they do not authorize deleting another checkout or resetting a branch.

## Receipts and lifecycle

Creation receipts record whether the branch existed before creation.
Recovery fills an incomplete checkout from its recorded commit and preserves the existing branch.
Reviewed removal and removal resumption retain the branch after deleting the owned workspace.
The invoking worktree's index and source files receive no writes.

Only `applied: true` confirms completion. Inspect `applied` even when the CLI exits successfully.
Preflight refusals use the ordinary error response. Mutation failures return `applied: null` and inspection guidance.
A registered failure with an ownership receipt or matching durable preparation can use
[creation recovery](git-worktree-recovery.md). Inspect the destination, branch and registration first.
Without that recorded ownership evidence, recovery refuses.

For example, you already committed the checkout repair on `agent/task`. Creating its worktree copies
that branch's reviewed commit; it does not reset the branch or copy unfinished edits from another folder.
If a later commit moves the branch after preview, prepare a new preview before creation.
See [removal](git-worktree-removal.md) and [removal resumption](git-worktree-removal-resumption.md) for later cleanup.

## Formal coverage

The anchored branch-selection predicate requires an unused branch and presence matching the selected mode.
Lean proves both requirements; shared Rust/Lean cases cover all eight boolean inputs.
An abstract attachment model proves preservation of the reference map when attaching to the expected commit.
Git registration, reference locking, branch configuration and filesystem behavior remain outside general correspondence proofs.
CLI tests supply host evidence for preservation, stale reviews, occupied branches, locks and interrupted creation.
