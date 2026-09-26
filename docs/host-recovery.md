# Journaled host recovery

Native source and file transactions stage their replacements before publishing any target.
Each publication checks the target snapshot and the staged snapshot against the reviewed change.
Snapshots distinguish absent files, empty text, regular-file permissions and literal symlink targets.
A changed staging file causes refusal and rollback of earlier writes.

Recovery accepts only the transaction's before or after snapshots. It restores after snapshots in
reverse order and skips paths already at their starting state. Before clearing the journal, it
rechecks every affected path and synchronizes its existing ancestor directories. This includes
paths that recovery did not need to write. A conflict or synchronization failure retains pending
recovery when the journal has not yet published its final checkpoint.

## Error reports

A failed transition adds `error.history` to the existing CLI JSON error object. The Python runtime
retains that object in `FrRuntimeError.report`; Rust callers can downcast to `history::HistoryFailure`.
The top-level error kind and cause chain remain available.

```json
{
  "transaction": 1,
  "action": "apply",
  "phase": "installation",
  "outcome": "recovery-incomplete",
  "journal_pending": true,
  "recovery_error": "the attempted rollback failure"
}
```

`action` is `apply`, `undo`, `redo` or `recover`. Phases and outcomes have these meanings:

| Phase | Outcome | Meaning |
|---|---|---|
| `preparation` | `unchanged` | The pending checkpoint failed; source installation did not start. |
| `installation` | `rolled-back` | Installation failed and recovery confirmed the starting state. |
| `installation` | `recovery-incomplete` | Installation and its recovery attempt both failed. |
| `recovery` | `recovery-incomplete` | Explicit recovery could not confirm completion. |
| `finalization` | `finalization-uncertain` | Source installation finished, but the final journal save failed. |

`journal_pending` comes from a fresh journal read after failure. It is `true` for a pending
transaction, `false` for a readable journal without one, or `null` when readback fails.
It describes observed journal contents, not confirmed persistence after a synchronization error.
`recovery_error` is nullable and retains an automatic rollback error separately from the original cause.

Inspect `fr history` before retrying an uncertain finalization. Run `fr history recover ID --write`
when that journal reports a pending transaction. A save can fail after publishing its checkpoint;
blindly retrying the original command could therefore repeat a completed change.
Preflight refusals, lock acquisition and initial plan recording retain the ordinary error contract.

## Evidence and limits

The [pinned task](../tests/agent-eval/host-recovery/task.json) records the baseline and admission boundary.
The [retained evaluator](../tools/host-recovery-acceptance.py) runs the host fault matrix and model comparison.
Each observed operation boundary receives a handled error and a separate child-process exit.
Cases include staging creation, writes, modes, links and sync; publication and deletion; journal
creation, serialization, replacement and sync; snapshot checks; and recovery synchronization.
Every case reads the journal afresh, resumes pending work and compares filesystem snapshots and stacks.
Additional cases alter staged bytes, modes and symlinks, interrupt rollback, or edit a restored file.

The hooks exist only in test builds. Production commands have no fault-injection environment switch.
The matrix keeps its fixtures within temporary workspaces. It does not exercise disk exhaustion,
partial kernel writes, lock acquisition failure, device loss or a machine power cut.
Atomic rename, filesystem sync, snapshot reads, advisory locks and the host process remain trusted.
A noncooperating writer can still change a path between a check and a subsequent filesystem operation.
Process exit can leave staging files or newly created directories; recovery owns recorded targets,
not a general workspace cleanup operation. The native `edit::commit` library writer and browser
memory history have separate contracts.

The [Lean model](../kernels/FrKernels/HostRecovery.lean) proves required publication matches,
conflict refusal, exact abstract restoration and idempotence. Source anchors bind the two Rust
predicates; 224 executable results compare Rust and Lean. These results establish finite model
agreement and model theorems. They do not prove correspondence for the host filesystem adapter.
