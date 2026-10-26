# Deliver one verified transaction

A transaction saves an edit. A workflow applies it, runs declared checks and delivers its patch. A failed check can leave the source edited.

Use this route after reviewing and saving one complete change plan. First list `fr checks`.
A basis identifies the reviewed state; copy it unchanged. Keep the checks' basis, the transaction ID
and the plan's `transaction_context_basis`.

Save this JSON as extensionless `.fr/deliver`. Adding a `.json` file changes the reviewed source,
even when Git ignores it, and invalidates the plan.
The input `schema` is the integer `1`; `fr-workflow-1` identifies output reports.

```json
{
  "schema": 1,
  "transaction": 7,
  "transaction-context-basis": "frtb2:<FROM_SAVED_PLAN>",
  "checks": {
    "basis": "<FROM_FR_CHECKS>",
    "names": ["unit", "lint"]
  },
  "exercise-reversal": false,
  "patch": {"output": "change.patch"},
  "check-output-bytes": 2048
}
```

```sh
fr workflow --from .fr/deliver
fr workflow --from .fr/deliver --write --basis '<WORKFLOW_BASIS>'
```

Copy the first command's `workflow_basis` (`frwb1:...`) into `--basis`.
The author's `plan_context_basis` (`frpb1:...`) identifies a different preview.
Keep the JSON field names and replace every placeholder.
A successful workflow writes a Git text patch to `patch.output`; its formatting can differ from Git's own output.
If the request specifies `git diff --binary HEAD`, write that output once after the workflow.
Review the delivered patch and finish. The named checks already ran.
`check-output-bytes` bounds retained failure streams.
`exercise-reversal` adds undo and redo checks when needed.
Set `exercise-reversal` to true only when the same checks also accept the original code.
For a repair or feature, false is appropriate: undo restores the original mistake or missing feature, which new tests should reject.
Use the [history commands](history.md) to review undo and redo separately.

Review the preview's stage list, full resolved bases, check coverage, patch digest and destination.
Copy its `workflow_basis` into the write. This omits that unchanged preflight envelope.
A failed stage stops later work and leaves their status `pending`. Inspect `transaction_status` and
preserve check diagnostics. A pending transaction still requires the separate recovery route.

For an unchanged applied transaction, `resume-applied: true` starts with fresh checks and requires
a reviewed workflow basis. Read `docs/workflow-recovery.md` for admission and delivery limits.

This workflow checks only project-declared commands. Independent behavior oracles and receiver
checks remain separate evidence when the task requires them. Use the manual Checks, History and Git
routes when no check configuration exists or when each transition needs external validation.
