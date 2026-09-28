# Reviewed structural changes

`refactor` author operations reuse the native rename and signature planners through revision-bound
handles. Task changes require a fresh [consumer scope](change-scopes.md), select every declared
candidate check, and reject any resulting file outside that scope. Preview shows the complete
multi-file diff. Existing history, checked outcomes, reversal and patch delivery apply unchanged.

```python
from fr_ir.change_scope import ChangeScope
from fr_ir.ir import TaskChange, TaskDelivery
from fr_ir.structural_change import RefactorRequest

handle = client.project("find", "subtotal").definition_target().handle
scope = ChangeScope.inspect(client, [handle])
change = scope.bind(TaskChange([], [
    RefactorRequest.rename("sum_values").target("rename", handle),
    RefactorRequest.remove_parameter(2).target("remove-legacy", handle),
], {"files-changed": 2}, ["behavior"], TaskDelivery(patch="artifacts/migration.patch"),
    acceptance_checks=["behavior"]))
review = client.review(change)
# Inspect the diff, scope and checks before execution.
result = client.execute(review)
```

The fragment for a `refactor` target is JSON. It accepts one of these shapes:

- `{"operation":"rename","name":"sum_values"}`
- `{"operation":"remove-parameter","index":2}`
- `{"operation":"move-parameter","from":0,"to":1}`

Positions are zero-based and below 64. Movement requires distinct positions. Unknown fields refuse.
The SDK's `RefactorRequest` validates the transport; native preflight checks source and semantics.
Author batches accept the same request through `from`. Low-level authoring supplies a reviewed
mutation; scope-bound task delivery adds the required discovery and check selections.

## Admitted subset

Targets are Rust free functions. Renames retain the existing collision and capture checks and refuse
any planner warning. Weak, unresolved, dispatch and textual candidates need explicit resolution.
Signature changes retain unused-parameter and function-value refusals from the native planner.
Every parameter must have an explicit primitive integer, Boolean or character type. Every indexed
call argument must be an integer, Boolean or character literal, including negative integer literals.
Calls with variables, computations, blocks, macros or effects remain outside this signature route.

These restrictions prevent this route from removing an effectful argument or changing the order of
side effects. They do not prove general source equivalence. Compiler checks still establish typing;
behavior checks establish only their executed cases. External consumers and runtime dispatch remain
outside the indexed scope. Removing a public API parameter can require external migration.

Task resolution and author batches both admit at most 32 targets.
All batch operations use the original snapshot. Disjoint rename and parameter edits can share one
transaction. Overlapping selections refuse in either order; dependent migrations need a new review
against the changed source. A clipped diff cannot authorize execution. Adding consumers, changing
check associations or changing source invalidates the retained scope and any old review.

Persist scopes and task plans with the existing SDK object store. Fresh processes must revalidate
inputs before delivery. Retained receipts bind ordered check results to the delivered snapshot;
they do not authorize another write or automatically renew pre-change discovery.

## Evidence

The pinned [task](../tests/agent-eval/structural-change/task.json) migrates a Rust function and 49
call sites in two files. The deterministic evaluator compares a prescribed ordinary edit with a
fresh-process native delivery. An independent Python oracle compiles the program and checks all
public outputs before and after. Receiver replay checks exact apply, reversal and reapplication.
The [retained manifest](../tests/agent-eval/results/2026-09-28-structural-change/manifest.json) binds
these artifacts. No model service runs and no token-saving or live-agent claim is made.

The Rust/Lean target admission comparison includes the new operation and an unknown-operation
refusal. Its theorem concerns the language/target predicate. Parser correctness, reference resolution,
source behavior and the full native refactor implementation remain outside that proof.
