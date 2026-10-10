# Finish delivery after a failed applied check

A checked edit can change source and then fail its applied-state check. The failed report retains
the transaction and stage outcomes, leaves later stages pending, and withholds the patch. Keep
that report. Failure does not mean that source was restored.

Choose recovery from the actual source state:

- If an external check prerequisite failed and the recorded edited source is still correct,
  repair that prerequisite, review an applied-state workflow, and resume delivery.
- If the source needs repair, preview undo and restore the original source before preparing a
  corrected edit. A source repair on top of the applied transaction needs a new reviewed
  transaction; resuming the old one cannot deliver a combined patch.
- If another edit conflicts with the transaction, preserve it and resolve the conflict explicitly.
  Neither resume nor undo grants permission to overwrite it.

## Review and resume an unchanged applied transaction

Reopen the project and run `fr history show TX`. Read the current `context_basis` and
`required_checks`, including its `configuration_basis` and exact ordered `checks` list.
Preserve the original delivery and acceptance-check policy. An ordinary history record without
required checks does not supply that policy; use the original reviewed request.

Write an extensionless manifest outside recognized source, such as `.fr-resume`:

```json
{
  "schema": 1,
  "transaction": 7,
  "transaction-context-basis": "<CURRENT_CONTEXT_BASIS>",
  "resume-applied": true,
  "checks": {
    "basis": "<REQUIRED_CONFIGURATION_BASIS>",
    "names": ["behavior"]
  },
  "exercise-reversal": true,
  "patch": {"output": "artifacts/change.patch"},
  "check-output-bytes": 512
}
```

```sh
fr workflow --from .fr-resume
fr workflow --from .fr-resume --write --basis '<REVIEWED_WORKFLOW_BASIS>'
```

Inspect the preview before the write. Resume requires that explicit workflow basis and the latest
applied, replayable transaction. Preview validates recorded source, file kinds, modes and current
history, then binds the current source revision, transaction context, checks, patch and manifest.
Changed source or declarations between preview and execution refuse before checks or writes.

The first stage is `check-applied`; there is no initial `apply`. With reversal enabled, the stages
are `check-applied`, `undo`, `check-restored`, `redo`, `check-applied`, and `deliver-patch`.
Declared acceptance checks run after each applied check. Each successful applied check records
its receipt against the transaction and exact source revision. Patch delivery occurs only after
all requested stages pass. Independent receiver replay and behavior checks remain separate work.

`check-original` is invalid in resume mode: the transaction is already applied. Use
`exercise-reversal` to check restored source. A repeated check failure again stops later stages
and withholds delivery. An existing patch destination refuses; resume does not overwrite it.

The new workflow rechecks current conditions. It does not reuse a failed result or claim that
an earlier process completed. It does not recover a pending filesystem write; use the separate
history recovery commands for an interrupted transaction.

## What the comparison measures

The hosted comparison uses four prescribed Python/Rust body edits, both recovery routes, and
success, renewed check failure, later source edits and changed check declarations. The failed
prerequisite lives outside project source; fixing it does not change the check declaration or
edited source. The caller authors every replacement body.

Retained programs include reopening, review, execution and receipt selection. Reports include
the initial failed attempt, recovery traffic, exact source, patches, check receipts and fresh
receiver replay. Successful receiver patches also face an independent behavior check and a
conflicting receiver checkout. These are workflow regressions, with no live model or general
token-efficiency claim. Source repair and combined delivery across multiple transactions remain
outside this comparison.

## Retained results

[Run 38043857792](https://github.com/e6qu/fun-refactor/actions/runs/38043857792) passed the native
workflow tests, SDK type checks and all 32 comparison cells. Eight successful deliveries passed
receiver application, behavior and conflict controls. The other 24 cells retained renewed check
failures and stale source/check refusals without delivery. Twelve offline corruption tests audit
the [retained report](../tests/agent-eval/results/2026-10-10-recovery-delivery/recovery-delivery-context.json).

| Edit shape | Resume caller bytes | Undo/retry caller bytes | Resume total protocol bytes | Undo/retry total protocol bytes |
| --- | ---: | ---: | ---: | ---: |
| One Python body | 1,203 | 1,565 | 26,270 | 39,048 |
| One Rust body | 1,203 | 1,564 | 26,008 | 38,441 |
| Two Rust bodies | 1,203 | 1,654 | 29,680 | 45,516 |
| Two Python files, same function name | 1,203 | 1,670 | 30,868 | 47,389 |

Caller bytes count the complete recovery program, transaction input and canonical submission.
Protocol bytes count every serialized request and response, including the initial failed attempt.
Resume uses four recovery calls and seven total calls; undo/retry uses six and nine. Both paths
deliver identical patch bytes for each fixture. These counts do not measure model tokens or
general agent efficiency.

The same run refreshed the earlier 40-cell body and 38-cell route comparisons. Repository
[run 38043863822](https://github.com/e6qu/fun-refactor/actions/runs/38043863822) refreshed the two
existing Python tasks, including 365 behavior cases, interruption and patch replay. These remain
known scripted tasks. The roadmap stays at 7/18 demonstrated items and 0/4 completed milestones.

The CLI change also required fresh [host-interruption evidence](https://github.com/e6qu/fun-refactor/actions/runs/38044199608).
That run passed 430 handled-failure boundaries, 430 process-exit boundaries and 224 model cases.
These checks concern interrupted writes, independently of the failed-check resume path above.
