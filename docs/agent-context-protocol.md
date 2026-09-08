# Agent context protocol

Project, author and source-history transition reports can omit facts that the caller has already reviewed. The omission is opt-in and bound to the exact retained facts; a normal call always remains self-contained.

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

## History transitions

A full `history apply`, `undo`, `redo` or `recover` preview includes a `frhb1:` context basis bound to the transaction, action and complete change rows. Supply it when performing the reviewed transition:

```sh
fr history undo '<TX>'
fr history undo '<TX>' --write --no-diff --context-basis '<HISTORY_CONTEXT_BASIS>'
```

The completion then keeps `applied` and omits `action`, `changes` and `transaction`. Copy those fields from the matching preview and change only `applied` to reconstruct the ordinary full write report. A basis from another action, transaction or change set refuses before writing. Without `--context-basis`, `--no-diff` keeps paths, existence and modes while omitting only diff strings for compatibility.

Project bases and history bases are separate namespaces. Check configuration bases, Git status revisions, patch record bases and worktree proposal bases keep their existing meanings and cannot substitute for either context basis.

## Evidence boundary

Compaction changes serialization only. Limits, omitted result counts, uncertainty, source verification, parser validation, check declarations and transaction conflict checks retain their existing contracts. The full reviewed response and every compact response belong in an audit record when a workflow relies on reconstruction.
