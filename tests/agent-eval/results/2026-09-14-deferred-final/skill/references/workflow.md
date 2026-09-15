# Deliver one verified transaction

Use this route after reviewing and saving one complete change plan. First list `fr checks`. Retain
its basis, the transaction ID and the plan's `transaction_context_basis`. Write an extensionless
control manifest outside recognized source so creating it does not stale the plan. The input
`schema` is the integer `1`; `fr-workflow-1` identifies output reports and is not valid here.

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
fr workflow --from '<WORKFLOW_MANIFEST>'
fr workflow --from '<WORKFLOW_MANIFEST>' --write --basis '<WORKFLOW_BASIS>'
```

The manifest names `transaction`, `transaction-context-basis`, and `checks` with `basis`
and `names`. Replace every placeholder and preserve the exact hyphenated field names. Optional
`exercise-reversal` checks apply, undo and redo states in order. Optional
`patch.output` creates a new relative Git patch only after all stages pass. `check-output-bytes`
bounds retained failure streams.

Review the preview's stage list, full resolved bases, check coverage, patch digest and destination.
Copy its `workflow_basis` into the write. This omits that unchanged preflight envelope.
A failed stage stops later work and leaves their status `pending`. Inspect `transaction_status` and
preserve check diagnostics. A pending transaction still requires the separate recovery route.

This workflow checks only project-declared commands. Independent behavior oracles and receiver
checks remain separate evidence when the task requires them. Use the manual Checks, History and Git
routes when no check configuration exists or when each transition needs external validation.
