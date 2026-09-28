# Checked task outcomes

Compilation and reversible delivery can succeed while a requested behavior fails. The
[unknown-target trials](evaluations.md#unknown-target-investigations) retained that outcome.
Task changes now accept a separate list of post-change checks for the requested behavior.

```python
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget

change = TaskChange(
    [], [TaskTarget("repair", handle, "replace-body", fragment=body)],
    {"files-changed": 1, "paths-changed": ["src/lib.rs"]},
    ["compile"], TaskDelivery(patch="artifacts/change.patch"),
    acceptance_checks=["behavior"],
)
review = client.review(change)
# Inspect the complete review and declared check commands.
result = client.execute(review)
```

Both lists select existing names from `.fr/checks.json`. Ordinary `checks` run at the requested
original, applied, restored and reapplied lifecycle stages. `acceptance_checks` run after the applied
checks and again after redo. They never run on the original or restored state. This permits a bug
reproducer to fail before the repair while still checking exact reversal.

Preview binds the acceptance selection, configuration and toolchain identity into the task-change
basis. An unknown name or changed review refuses before source mutation. Execution rechecks the
acceptance toolchain before each run and retains source, configuration and toolchain stability.
The regular workflow manifest uses an optional `acceptance-checks` object containing `basis`, `names`
and `toolchain-digest`. Existing manifests without this field keep their previous stages and behavior.

A failed stage stops the workflow and withholds its patch. Source remains at the reported history
state, usually applied, so the agent can diagnose the failure or explicitly undo it. The workflow
does not automatically undo a failed outcome or retry a nondeterministic check. History operations
remain separate explicit actions; acceptance is a condition of this reviewed workflow, not a global
restriction on every possible manual export.

## Retained plan completion

```python
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_delivery import run_delivery

plan = TaskPlan("Repair the public result", ("behavior passes",), (
    TaskStep.checked("outcome", "Does the delivered result meet the requirement?",
        checks=("behavior",), satisfies=("behavior passes",)),
))
store = DirectoryObjectStore("/tmp/task-evidence")
run = run_delivery(plan, client, "outcome", review, store)
assert run.passed and run.resumed.complete
```

The helper requires exactly the reviewed acceptance checks and all four checked dependency classes.
It stores the started plan before execution, then retains the full delivery receipt before attaching
evidence. A passing delivery explicitly resets only the selected outcome step to its post-change
inputs, starts it, and attaches the final acceptance report through native check validation.
Other steps keep their own dependencies and evidence. A changed diagnosis or prerequisite remains
stale; the helper cannot make that prerequisite true. Steps requiring model proofs use the separate
proof attachment API.

`DeliveryReceipt.restore(store, root)` validates the retained manifest identity, ordered stage
coverage and outcomes. `TaskPlan.restore(store, run.plan_root).resume(client)` revalidates completion.
To attach a receipt in another process, explicitly reset and start the selected step, then call
`attach_delivery(plan, client, step, receipt)`. Attachment rechecks current source, configuration,
declared check commands, executable identities and declared environment/file inputs. Editing those
inputs invalidates completed steps and refuses old receipts. Independent observations can survive.

Keep the object store outside the analyzed project. Failed execution and failed attachment retain
different outcomes. A receipt may record successful delivery while a changed prerequisite prevents
plan completion. Interrupted CLI execution has no completed SDK receipt; reopen the stored started
plan and inspect native history before preparing another review. No stored receipt authorizes a write.

Check declarations define coverage. Passing commands establish only their executed cases; declared
toolchain inputs do not cover undeclared subprocess dependencies or imports. Merkle identities bind
trusted local records and detect changed stored bytes. They do not authenticate an untrusted report
producer or prove source implementation equivalence.

## Evidence

The [pinned task](../tests/agent-eval/checked-outcomes/task.json) uses a two-file Rust subtotal repair
and fee requirement. The deterministic evaluator supplies the implementations; this is not a live
agent comparison. Its independent Python oracle compiles the public Rust API and checks 49 outputs.
It retains compiler-only acceptance of a wrong change, rejection under post-change checks, successful
delivery, fresh-process completion, invalidation and independent patch replay with exact reversal.
The [retained manifest](../tests/agent-eval/results/2026-09-28-checked-outcomes/manifest.json) binds its
artifacts and source snapshots. Run `python3 tools/outcome-acceptance.py --audit DIRECTORY` to verify
them and replay the compiled receiver checks without calling a model service.

The [SDK tests](../sdk/python/tests/test_investigation_delivery.py) cover stale checkers, changed
configuration and environment, source-writing checks, failure after redo, malformed receipts and
plan/manifest requirement mismatches. Native tests compile and run independent Unicode cases.
The [Lean model](../kernels/FrKernels/Workflow.lean) admits acceptance only in the applied state;
sixteen shared Rust/Lean cases check that finite stage policy. This does not prove host execution.
