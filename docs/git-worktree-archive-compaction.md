# Compacting completed worktree removal archives

`fr git worktree compact-removal RECORD` previews compaction of one completed removal archive on Unix.
A reviewed write saves a small audit summary, then deletes the full recovery record.

```sh
fr git worktree compact-removal /repo/.git/fr-worktree-removal-ABC123/record.json
fr git worktree compact-removal /repo/.git/fr-worktree-removal-ABC123/record.json --basis TOKEN --write
```

Use the `removal_record` returned by [worktree removal](git-worktree-removal.md).
Keep using that original path after compaction, even though `record.json` no longer exists.
Relative paths resolve from the invoking repository root. Another linked worktree in the same repository may invoke the command.

For explicit bulk retention, pass 1 through 32 record paths to the plural command:

```sh
fr git worktree compact-removals RECORD_A RECORD_B
fr git worktree compact-removals RECORD_A RECORD_B --basis TOKEN --write
```

The preview validates every archive independently and binds their normalized paths, states and individual bases into one `frwtacs1:` basis.
Duplicate, completed-summary, incomplete or otherwise ineligible archives refuse the whole preview.
The write rechecks the entire selection, then invokes the same per-archive compactor in the reviewed order.
Unselected archives and repository state remain unchanged.
The command returns a compact JSON report in both output modes, without source bodies or private metadata contents.

## Eligibility and review

Compaction requires the exact completion marker, absent checkout and private metadata roots, and no active archive lock.
An incomplete removal must first finish through [removal resumption](git-worktree-removal-resumption.md).
Reappeared files, directories and dangling symlinks at either root block compaction.
Existing locks are preserved and never broken automatically.

Before discarding a full record, the reader applies removal archive validation, including inventory and source hashes against the original commit.
The original destination parent and shared repository must retain their recorded identities.
Records are limited to 128 MiB; archived private metadata remains limited to 16 MiB.
The historical branch may have advanced or no longer exist. Compaction neither changes nor recreates refs.
The original commit must still be available while the full record exists.

The basis binds the invoking root, archive location and identity, record bytes and mode, completion marker, summary and observed path states.
A changed observation requires a fresh preview. A write requires `--basis TOKEN`.
The report includes `can_compact`, `completion_confirmed`, individual checks, branch, commit, tree and archive state.
`record_bytes` gives the original JSON length; `summary_bytes` gives the stored summary length when present.
`discard` describes the full inventory and private metadata removed by compaction.
These are logical payload sizes, not measured disk-space savings.

## Durable summary and deletion

The writer holds the same owned `resume.lock` used by initial removal and resumption.
It writes and synchronizes `summary.json`, publishes it without replacing an existing file, then synchronizes the archive directory.
The summary retains paths, ownership identities, branch, commit, tree, inventory counts and fingerprints of the original record and completion marker.
It contains no full file inventory, source bodies, index bytes or other private metadata contents.

Only after saving the summary does the writer recheck eligibility and unlink the full record.
The unlink requires the original file identity, exact bytes and complete Unix mode to match.
The archive directory is synchronized again after deletion.
The summary, completion marker, archive directory and unrelated sibling files remain.
No checkout files, refs, shared objects, invoking index or repository configuration receive writes.

Compaction discards recovery data and cannot be undone through `fr`.
The retained summary supports audit inspection, not reconstruction of the removed worktree's private metadata.
Bulk writes are sequential rather than filesystem-atomic.
`applied: true` means every selected archive compacted; `applied: null` returns completed paths and the archive where processing stopped.
There is no automatic retention period or archive-directory purge.

## Outcomes and interrupted writes

- `full`: the original record remains and no summary has been published.
- `compaction-pending`: both files remain; a fresh reviewed write can finish after blockers are resolved.
- `compacted`: only the summary remains; `can_compact` is false and another write refuses.
- `unconfirmed`: a mutation failed or its result could not be confirmed; inspect the original record path again.

Only `applied: true` confirms successful compaction. Previews return `applied: false`.
Preflight refusals use the usual CLI error response.
Mutation failures return `applied: null`, `can_compact: null` and a bounded diagnostic with a successful process exit.
Agents must inspect `applied`, then request a new preview after an uncertain outcome.
If both files remain, their provenance and fingerprints must agree before retrying.
Retries synchronize the existing summary and archive directory again before deleting the full record.

`resume-removal RECORD` recognizes pending and completed compaction summaries and returns a small audit report with `can_resume: false`.
That report has the compaction fields instead of per-file rows; `--limit` does not change it.
Completed summary inspection validates local ownership and the completion marker without loading the historical commit or current branch tip.
A summary is local audit data, not a signed attestation.

Locks coordinate cooperating writers. A crash can leave a lock requiring manual ownership review.
Publication and unlinking are separate operations; hostile concurrent path replacement remains outside complete race protection.
The summary-before-deletion ordering does not establish an atomic filesystem transaction or a general crash-durability proof.

## Formal coverage

The anchored compaction guard requires completion, absent checkout and metadata paths, and an available archive lease.
Lean proves all four conditions are necessary. Shared Rust/Lean cases cover all sixteen boolean inputs.
A separate abstract archive model proves audit preservation and compaction idempotence.
Those model results do not prove correspondence for JSON validation, Git observations, filesystem operations or crash durability.
