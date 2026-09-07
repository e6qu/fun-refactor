# Inspecting and resuming worktree removal

`fr git worktree resume-removal RECORD` inspects an archived removal on Unix.
It reports remaining paths, already missing paths and blockers without writing files.
A reviewed write finishes deleting the surviving recorded paths and marks the archive complete.

```sh
fr git worktree resume-removal /repo/.git/fr-worktree-removal-ABC123/record.json --limit 20
fr git worktree resume-removal /repo/.git/fr-worktree-removal-ABC123/record.json --basis TOKEN --write
```

Use the `removal_record` returned by [reviewed removal](git-worktree-removal.md).
Relative record paths resolve from the invoking repository root. Another worktree in the same shared repository may invoke the command.
The command returns JSON in both output modes. File bodies and archived metadata bytes stay out of its report.

## Archive validation

The record must be a regular `record.json` file in a removal archive directly under the shared Git directory.
The reader accepts the existing version-one removal format and limits the record to 128 MiB.
It checks ownership fields, parent identities, allowed private metadata paths and the recorded Git links.
Archived metadata contents retain the removal limit of 16 MiB in total.
The inventory and source hashes must agree with the retained commit; private metadata hashes must agree with the archived bytes.
The original completed creation receipt must match its archived copy.
Malformed, inconsistent, foreign and unsafe records refuse before deletion.

These checks establish internal consistency and local ownership observations.
Archives are not signed attestations against a malicious local user.
Changing the archive or its directory identity requires another inspection.
The destination's parent and shared repository must retain their recorded identities.
The invoking worktree and its ancestors cannot be removed.

## Inspection and review

Rows identify their scope, path, kind and observed state.
Surviving reviewed files must retain their identity, bytes and full Unix mode.
Surviving directories must retain their identities. Missing files and directories count as finished work.
A replaced directory is not traversed; its recorded descendants are reported as uninspected.
Extra paths, changed files, unsupported entries and existing locks are blockers.
Blockers appear first, followed by remaining paths and then missing paths.

`--limit` defaults to 20 and accepts 1 through 500. Counts cover all observed rows, including omitted rows.
There is no continuation cursor. Changing the row limit preserves the basis.
The basis binds the caller, archive bytes and identity, path states and current Git registrations.
Only an incomplete observation with no blockers reports `can_resume: true`.
A completion marker prevents further writes, including when its contents are invalid or paths have reappeared.
The owned branch must still be direct and point to the commit reviewed for removal.

Resumption accepts partial checkout deletion, partial private metadata deletion and absent checkout or metadata roots.
It can also confirm a removal whose directories disappeared before its completion marker was written.
It does not restore source files, reconstruct metadata or adopt replacement files.
Resolve blockers deliberately, then request a fresh inspection and use its basis.

## Writes and failures

An owned `resume.lock` in the archive coordinates initial removal and resumption.
When private metadata survives, the writer also acquires receipt, index and HEAD locks there.
The prepared branch verification lease retains the recorded branch and commit through deletion.
Existing locks are not broken automatically. Cleanup preserves replacement lock files.

The writer repeats inspection after acquiring locks.
It deletes only files observed as remaining, with fresh identity, bytes, mode and parent-directory checks before unlinking.
Missing paths are skipped. Directories are removed only when empty.
New content or changed bytes that arrive after review can stop deletion and remain for inspection.
The private registration is removed after the checkout. Branches and the invoking worktree's index receive no writes.

Only `applied: true` confirms completion. It requires both target directories to be absent and a synchronized completion marker.
Preflight refusals use the usual CLI error response.
Mutation failures return `applied: null`, `can_resume: null`, a bounded diagnostic and the same `removal_record`.
These JSON outcomes exit successfully, so agents must inspect `applied`.
The returned counts describe the reviewed observation; inspect again to learn the state after a partial failure.

Resumption does not recreate deleted paths or perform worktree undo/redo.
Crashes can leave locks requiring manual ownership review.
Observation and unlinking are separate filesystem operations; hostile concurrent path replacement remains outside complete race protection.
Neither deletion nor completion is an atomic transaction across all involved paths.
Completed records support [reviewed compaction](git-worktree-archive-compaction.md). Bulk archive retention remains pending.
When a compaction summary is present, this command returns a small audit report with `can_resume: false` and no per-file rows.
Use the original record path with `compact-removal` to inspect or finish compaction.

## Formal coverage

An anchored resume predicate accepts absent paths and requires identity, byte and mode matches for present files.
Lean proves both properties. Shared Rust/Lean cases cover all sixteen boolean inputs.
The abstract selected-removal model also proves idempotence: applying the same selected removal twice has the same result as applying it once.
Archive parsing, host observations, Git leases, filesystem deletion and crash durability remain outside general correspondence proofs.
