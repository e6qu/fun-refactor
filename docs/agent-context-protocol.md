# Avoid sending an agent the same report twice

Suppose an agent has already read a function report and asks for its callers next. It can keep the earlier report and request only the new information.
The identifiers below let the client check which earlier facts it may reuse. A changed file requires a fresh check.

Context is the text available to an agent for its next decision.
A report can repeat project details the agent already read. A basis is a returned fingerprint connecting later output to that earlier report.
For a second caller query, retain the complete first report before requesting an abbreviated reply.
The client must reconstruct and verify omitted fields; storing the earlier report and checking it still costs work.

## Commands, results and limits

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

The client reconstructs the response by copying the three named fields from its saved full report
and removing `context_omitted`. Keep `context_basis` as the reference to the saved report; copy it unchanged.
`fr` checks that the current project still matches it before omitting those fields.
Changes to source, manifests, scan policy or tool version require a fresh full report.
Authoring checks this reference before saving or applying a transaction.

For example, the agent can avoid reading the same project coverage twice while asking for callers.
If another agent edits a helper between those calls, the compact request refuses. Read a fresh
full report, then continue from its new reference.

For client implementers, the basis is SHA-256 over the JSON encoding of
`["fr-context-1", revision, handle_prefix, coverage]`.

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
fr history redo '<TX>' --write --no-diff --context-basis '<TRANSACTION_CONTEXT_BASIS>'
```

The same retained basis can compact a later redo.

UTF-8 encodes text as bytes. For example, `é` occupies two bytes, so byte limits can differ from character counts.

The compact report keeps the transaction, action, outcome, paths, existence and modes. Each change replaces `diff` with its UTF-8 `diff_bytes`; the report adds `context_basis` and `context_omitted: ["changes[].diff"]`. Reconstruct the full report from the detailed history record, or split the retained combined author diff at those byte boundaries. Each slice is one valid UTF-8 diff in change order.

Transaction bases cannot compact undo or recovery because those reverse diffs were not reviewed in the forward author report. Preview reverse transitions normally. A stale or different transaction basis refuses before writing. Truncated author diffs do not receive a transaction basis. Without `--context-basis`, `--no-diff` keeps the same metadata and uses `diffs_omitted: true`.

Saving an identical plan again against the same revision reuses the existing planned transaction. The repeated report has `saved: false` and `reused_transaction: true`, so a retried agent call cannot create duplicate journal entries.

Use each reference only for the action that returned it. A project report, an edit preview and
a saved transaction cover different contents. For example, `context_basis` from a caller query
cannot replace `plan_context_basis` when saving an edit. Check and Git references have their own roles too.

## Reviewed task-change context

`fr task-change` replaces the task-to-author and author-to-workflow joins with one `frtc1:` basis.
The basis covers the complete task report, author report, exact source payloads, selected checks and
delivery choices. Preview emits every covered field and writes nothing. Write accepts only the
unchanged complete basis, then returns a compact report with `reviewed_context_omitted`.

Reconstruct the execution context by retaining the preview and overlaying the write report. The
write keeps the transaction and complete workflow outcomes. Project, author, workflow and check
bases remain visible inside their owning evidence, but the caller does not copy them between
manifests. Any changed source, fragment, task manifest, check configuration or patch destination
refuses the reviewed write.

## Patch artifacts

`fr history patch TX --output FILE` writes a new patch atomically and returns its SHA-256, byte count and transaction metadata without serializing the patch into agent context. The command refuses an existing output path. Keep the artifact alongside the compact report; ordinary stdout and JSON exports remain available when patch text is needed inline.

## Evidence boundary

The baseline is the original recorded reply with repeated fields.
The text-size comparison substitutes abbreviated replies and current instructions into that same old sequence of calls.
Reconstructing a reply requires retaining and checking the earlier full report; those CPU and storage costs are not measured here.
This is a projection of an existing transcript, not a fresh agent completing the task.

Compaction changes serialization only. Limits, omitted result counts, uncertainty, source verification, parser validation, check declarations and transaction conflict checks retain their existing contracts. The full reviewed response and every compact response belong in an audit record when a workflow relies on reconstruction.

The [reading-size measurement](../tests/agent-eval/context-protocol-v3.json) replays an old transcript
with current instructions. For example, shortening a guide can reduce the text that the same
recorded calls would return. Refresh that measurement when the guides change. The documentation
review does this on GitHub and checks it against the unchanged original trials.
This measures text size; it does not establish a faster repair, lower billed cost or a new agent outcome.
