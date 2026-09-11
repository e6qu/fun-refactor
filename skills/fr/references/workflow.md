# Deliver one verified transaction

Use this route after reviewing and saving one complete change plan. First list `fr checks`. Retain
its basis, the transaction ID and the plan's `transaction_context_basis`. Write an extensionless
control manifest outside recognized source so creating it does not stale the plan.

```sh
fr workflow --from '<WORKFLOW_MANIFEST>'
fr workflow --from '<WORKFLOW_MANIFEST>' --write
```

The schema-one manifest names `transaction`, `transaction-context-basis`, and `checks` with `basis`
and `names`. Optional `exercise-reversal` checks apply, undo and redo states in order. Optional
`patch.output` creates a new relative Git patch only after all stages pass. `check-output-bytes`
bounds retained failure streams.

Review the preview's stage list, full resolved bases, check coverage, patch digest and destination.
A failed stage stops later work and leaves their status `pending`. Inspect `transaction_status` and
preserve check diagnostics. A pending transaction still requires the separate recovery route.

This workflow checks only project-declared commands. Independent behavior oracles and receiver
checks remain separate evidence when the task requires them. Use the manual Checks, History and Git
routes when no check configuration exists or when each transition needs external validation.
