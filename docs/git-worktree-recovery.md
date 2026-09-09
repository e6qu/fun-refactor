# Recorded worktree recovery

`fr git worktree recover PATH` previews completion of a recorded, incomplete raw checkout on Unix.
It fills missing committed files and finalizes the ownership receipt.

```sh
fr git worktree recover ../task --limit 10
fr git worktree recover ../task --basis TOKEN --write
```

Relative paths resolve from the invoking repository root. Invocation from the target itself can use `recover .`.
The destination must belong to the same shared repository as the invoking worktree.
The command returns JSON in both output modes.

## Ownership receipts

New creation operations record `fr-creation.json` in the linked worktree's private Git metadata directory.
The receipt binds the canonical destination, its parent, shared Git directory and linked metadata directory to their filesystem identities.
It also binds the `.git` link file's identity and bytes, branch, commit, tree and repository-local `extensions.worktreeConfig` mode.
The receipt records whether creation used an existing branch; older receipts default to new-branch mode.
The file starts with `complete: false`. Creation or recovery changes it to `complete: true` after checkout verification and synchronization.
Successful creation reports its `ownership_record` path.

A receipt records local ownership observations; it is not a signed attestation against a malicious local user.
Moved or replaced worktree directories and Git links cause refusal. Malformed receipts and missing ownership data also refuse.
Completed receipts refuse recovery, including when an agent later deletes tracked files intentionally.
Older worktrees without receipts require manual inspection. Recovery does not adopt them automatically.

Before mutation, creation publishes a destination-keyed preparation record in the shared Git directory.
The record binds the complete reviewed proposal and stays below 64 MiB; it contains file identities and sizes without source bodies.
The receipt begins after Git registration and ownership checks, before index or file materialization.
A failed Git command that leaves a valid registration can still receive a pending receipt.
If a process exits after exact registration but before receipt publication, recovery accepts the matching preparation as provisional ownership evidence.
It requires an absent receipt plus matching preparation and registration, then publishes the receipt and removes the preparation before materializing files.
Successful creation and recovery leave no preparation record.

## What recovery accepts

Recovery reloads the original commit's regular blobs with the same limits as [raw creation](git-worktree-creation.md).
It inventories the entire destination, including ignored paths, and refuses extra files or directories.
Existing tracked files must match committed bytes and the Git executable bit.
Matching files retain their bytes, permissions and inode identities. Missing tracked files can be created.
Symlinks and other non-regular entries refuse.

When worktree configuration is enabled, recovery accepts an absent `config.worktree` or preserves an existing regular file of at most 1 MiB.
Its identity, mode and content digest participate in the basis, and recovery verifies that the file remains unchanged before completing the receipt.
Changing the repository-local `extensions.worktreeConfig` mode after creation refuses recovery.

An existing index must match the complete committed inventory with plain entries.
Staged changes, unmerged entries, assume-unchanged, skip-worktree and intent-to-add states refuse.
Recovery preserves a matching index byte for byte. It creates a private index only when the destination index is absent.
Changed branches, changed commits, missing registration locks and active merge, rebase or sequencer state also refuse.
The command retains the branch, registration lock, source worktree and other repository data.

## Review and writes

Previews include bounded missing paths, existing-file counts and an `index_action` of `create` or `preserve`.
`ownership_state` distinguishes an existing `receipt` from a durable `prepared` recovery, and the latter reports `preparation_record`.
`--limit` defaults to 20 and accepts 1 through 500. The report counts omitted missing paths; it has no continuation cursor.
The basis includes the receipt, invoking repository, registrations, captured file identities and content hashes, directories and index bytes.
Changing the row limit preserves the basis. A changed observation or different invoking worktree requires a new preview.
File bodies and index bytes stay out of the report.

Writes require the reviewed basis and repeat the observation before modifying the checkout.
A private `fr-creation.lock` coordinates creation completion and recovery.
The writer also holds Git's `index.lock` through index handling, checkout verification and receipt completion.
Git documents the cooperating writer convention in its [lockfile API](https://git-scm.com/docs/api-lockfile).
An absent index is prepared privately, synchronized and installed with a hard link that refuses an existing destination.
Index creation requires filesystem hard-link support. An existing index keeps its original bytes and inode.

Missing files use exclusive creation. Matching files receive no writes or permission changes.
The writer checks the resulting checkout before marking the receipt complete.
Hooks, content filters, replacement objects and inherited Git environment overrides remain disabled or bypassed.

## Failures and limits

Only `applied: true` confirms completion. Previews report `applied: false`.
Preflight refusals use the usual CLI error response; mutation-phase failures report `applied: null` with a bounded diagnostic.
These JSON outcomes exit successfully, so agents must inspect `applied`.
A failure can leave additional matching files or a newly installed index with the receipt still pending.
Inspect the result, resolve any conflicting content deliberately, and request a fresh recovery preview.
The command never removes conflicting files, deletes a branch or recursively cleans a destination.
Prepared recovery refuses an absent, changed or mismatched registration. It does not adopt an arbitrary unowned worktree or retry a registration that Git did not retain.

Lock cleanup removes only the lock inode held by the process. Existing or replaced locks remain untouched.
A process crash can leave a lock requiring manual ownership review before another attempt.
Receipt replacement and synchronization do not form an atomic crash transaction with all Git metadata and checkout files.
Readers do not take these locks, and hostile concurrent filesystem changes remain outside complete race protection.
Recovery does not guarantee restoration after partial writes that leave differing file bytes; it refuses those files for inspection.

The anchored recovery predicate has Lean proofs for missing-file acceptance and existing-file matching requirements.
The configuration predicate proves that an accepted state has the reviewed repository mode and a regular file whenever the per-worktree configuration path is present.
The prepared-recovery predicate requires an absent receipt, matching preparation and matching registration.
Abstract file models prove preservation of accepted existing contents and modes, and refusal of changed files.
Shared Rust/Lean tests exercise all boolean inputs for these predicates.
Filesystem ownership, locks, receipt durability and the complete Rust workflow remain outside general correspondence proofs.

Completed receipts support [reviewed removal](git-worktree-removal.md) of clean owned worktrees.
