# Agent context protocol

Project, author and forward source-history reports can omit facts that the caller has already reviewed. The omission is opt-in and bound to the exact retained facts; a normal call always remains self-contained.

## Project and author reports

A full `fr project` or `fr author` report contains `context_basis` alongside `revision`, `handle_prefix` and `coverage`. Retain that complete report, then pass its basis to later calls over the same project snapshot:

```sh
fr project find parse --source --bytes 2048
fr project calls '<HANDLE>' --context-basis '<CONTEXT_BASIS>'
fr author replace-body '<HANDLE>' --from '<FRAGMENT>' --context-basis '<CONTEXT_BASIS>'
```

The compact response retains its query-specific result and adds:

```json
{
  "context_basis": "frcb1:...",
  "context_omitted": ["coverage", "handle_prefix", "revision"]
}
```

Copy the three declared fields from the reviewed full report and remove `context_omitted` to reconstruct the full response exactly. The basis is SHA-256 over the JSON encoding of `["fr-context-1", revision, handle_prefix, coverage]`; callers may treat it as opaque. `fr` recomputes it from the current project. Source, manifest, scan-policy or tool-version drift changes the project revision and refuses the compact call. Authoring validates the basis before saving or applying a transaction.

Do not use an omitted response without its complete reviewed basis. A compact response is evidence only when its basis matches that retained report and none of the omitted fields is already present with a competing value.

## Reviewed plan context

An author or feature-migration preview with a complete diff includes `plan_context_basis`.
It is a `frpb1:` SHA-256 identity over the full plan report and exact ordered before/after
source payloads. Retain that preview, then compact the repeated persistence call:

```sh
fr author batch --from '<MANIFEST>'
fr author batch --from '<MANIFEST>' --save-plan --plan-basis '<PLAN_CONTEXT_BASIS>'
```

The compact response keeps the plan basis and persistence outcome, including the transaction
and its context basis. `plan_context_omitted` names unchanged top-level fields copied from the
preview. Overlay the remaining response fields on the preview and remove that list to reconstruct
the ordinary saved or applied response. A clipped diff cannot create or consume a plan basis.
Changed source, fragments, manifests, migration options or generated output change the digest and
refuse before history or source writes. Project and plan bases can be used together; reconstruction
then applies both retained reports.

## Saved transaction context

A saved `fr author` report with a complete diff includes `transaction_context_basis`. A detailed `fr history show TX` report exposes the same `frtb2:` basis. It is bound to the ordered paths and complete before/after snapshots in that transaction. Retain the complete author diff or detailed record, then use the basis to omit repeated diff strings from a forward apply or redo:

```sh
fr author batch --from '<MANIFEST>' --save-plan
fr history apply '<TX>' --write --no-diff --context-basis '<TRANSACTION_CONTEXT_BASIS>'
# The same retained basis can compact a later redo.
fr history redo '<TX>' --write --no-diff --context-basis '<TRANSACTION_CONTEXT_BASIS>'
```

The compact report keeps the transaction, action, outcome, paths, existence and modes. Each change replaces `diff` with its UTF-8 `diff_bytes`; the report adds `context_basis` and `context_omitted: ["changes[].diff"]`. Reconstruct the full report from the detailed history record, or split the retained combined author diff at those byte boundaries. Each slice is one valid UTF-8 diff in change order.

Transaction bases cannot compact undo or recovery because those reverse diffs were not reviewed in the forward author report. Preview reverse transitions normally. A stale or different transaction basis refuses before writing. Truncated author diffs do not receive a transaction basis. Without `--context-basis`, `--no-diff` keeps the same metadata and uses `diffs_omitted: true` for compatibility.

Saving an identical plan again against the same revision reuses the existing planned transaction. The repeated report has `saved: false` and `reused_transaction: true`, so a retried agent call cannot create duplicate journal entries.

Project, plan and transaction bases are separate namespaces. Check configuration bases, Git status revisions, raw patch record bases and worktree proposal bases keep their existing meanings and cannot substitute for them.

## Patch artifacts

`fr history patch TX --output FILE` writes a new patch atomically and returns its SHA-256, byte count and transaction metadata without serializing the patch into agent context. The command refuses an existing output path. Keep the artifact alongside the compact report; ordinary stdout and JSON exports remain available when patch text is needed inline.

## Evidence boundary

Compaction changes serialization only. Limits, omitted result counts, uncertainty, source verification, parser validation, check declarations and transaction conflict checks retain their existing contracts. The full reviewed response and every compact response belong in an audit record when a workflow relies on reconstruction.
