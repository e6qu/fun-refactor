# Change a function name or arguments with its callers

When you add an argument to `subtotal`, the callers may need changes too. Review the proposed edits and the discovered callers together.
This guide covers supported rename and argument changes. Run the relevant checks before delivering the patch.

Use this route for a refactoring when behavior should stay the same.
A new required argument can deliberately change the interface; state the required caller behavior and checks before applying it.
The [focused milestone](../PLAN.md#m2-make-multi-file-refactorings-and-targeted-changes-dependable) names the next concrete multi-file cases.

A signature is a function's name and parameter list. A caller invokes that function with arguments.
A change scope lists the callers and tests found for the requested change.
For `subtotal(price)`, adding a required `tax` slot means updating calls that previously supplied only `price`.
Inspect missing relationships before promising every caller is updated; after applying the change, run the behavior test.

For a local source question, select the original file explicitly:

```sh
fr --json -C src/project/structural_change.rs project find plan --source --bytes 65536
```

The Python SDK accepts the same selection with `FrClient(path_to_source_file, ...)`.
This reads that file. Select the project directory when discovering callers across files.
`--in` filters a directory query's results; it does not narrow the files indexed for that query.

## Commands, results and limits

`refactor` author operations reuse the native rename and signature planners through revision-bound
handles. Task changes require a fresh [consumer scope](change-scopes.md), select every declared
candidate check, and reject any resulting file outside that scope. Preview shows the complete
multi-file diff. Existing history, checked outcomes, reversal and patch delivery apply unchanged.

```python
from pathlib import Path

from fr_ir.change_scope import ChangeScope
from fr_ir.ir import TaskChange, TaskDelivery
from fr_ir.runtime import FrClient
from fr_ir.structural_change import RefactorRequest

client = FrClient(Path("/project"), executable=Path("/path/to/fr"),
                  timeout=20, max_output_bytes=1048576)
handle = client.project("find", "subtotal").definition_target().handle
scope = ChangeScope.inspect(client, [handle], depth=4, nodes=128,
                            references=512, max_bytes=65536)
change = scope.bind(TaskChange([], [
    RefactorRequest.rename("sum_values", documentation="preserve").target("rename", handle),
    RefactorRequest.remove_parameter(2).target("remove-legacy", handle),
], {"files-changed": 2}, ["behavior"], TaskDelivery(patch="artifacts/migration.patch"),
    acceptance_checks=["behavior"]))
review = client.review(change)
# Inspect the diff, scope and checks before execution.
result = client.execute(review)
```

Replace the two absolute paths with the project and executable you intend to use.
The example assumes a Rust `subtotal` with an unused third parameter, literal
arguments at its callers, and a project-declared `behavior` check for both files.
The client limits each command to 20 seconds and 1 MiB of output. Discovery follows
at most four caller steps, 128 declarations and 512 references, returning at most
65,536 bytes. Inspect an unfinished scope before attempting an edit.

The fragment for a `refactor` target is JSON. It accepts one of these shapes:

- `{"operation":"rename","name":"sum_values","documentation":"preserve"}`
- `{"operation":"remove-parameter","index":2}`
- `{"operation":"move-parameter","from":0,"to":1}`

Positions are zero-based and below 64. Movement requires distinct positions. Unknown fields refuse.
The SDK's `RefactorRequest` validates the transport; native preflight checks source and semantics.
Author batches accept the same request through `from`. Low-level authoring supplies a reviewed
mutation; scope-bound task delivery adds the required discovery and check selections.

## Which changes this route can handle

Renames support Rust, Python and TypeScript functions declared outside classes or type implementation blocks.
They require an explicit documentation policy: `preserve` or `update`.
`update` includes matching comments, Markdown and documentation strings; ordinary executable strings remain unresolved.
For example, a docstring showing `subtotal(2, 3)` can follow the new name, while `getattr(module, "subtotal")` still refuses.
Text mentions and their file check associations must be included in the reviewed change scope.
Other document formats can use a separate [exact text transaction](file-transactions.md#exact-text-replacements)
with their own check, then contribute to the complete delivered patch.
Signature changes remain limited to Rust. Renames retain the existing collision and capture checks and refuse
any planner warning. Weak, unresolved, dispatch and textual candidates need explicit resolution.
Signature changes retain unused-parameter and function-value refusals from the native planner.
Every parameter must have an explicit primitive integer, Boolean or character type.
When removing a parameter, only its discarded argument must be a literal: an integer, Boolean or character written directly in source.
Negative integer literals also qualify. The arguments that remain can contain variables or expressions; the edit preserves their order and bytes.
Moving parameters still requires every argument to be a literal because movement can change evaluation order.

For example, removing an unused third parameter changes `subtotal(amount, fee, 0)` to `subtotal(amount, fee)`.
For `subtotal(record_price(), fee, 0)`, the `record_price()` call still runs before evaluating `fee`.
Removing the third argument from `subtotal(2, 3, record_purchase())` could remove an action the user needs.
That call refuses. Inspect the caller and prepare a separate reviewed change for its behavior.
Direct calls inside assertion macros, such as `assert_eq!(subtotal(amount, fee, 0), expected)`, use the same rule.
This reads the written call arguments; it does not analyze arbitrary macro expansion.

An import alias gives a local name to a function from another module.
For TypeScript, renaming exported `subtotal` to `sumValues` changes
`import { subtotal as displayTotal }` to `import { sumValues as displayTotal }`.
Calls through `displayTotal` keep that name. An unrelated function called `subtotal` also keeps its name.
Relative JavaScript and JSX imports use their source files when resolving these names.

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
call sites in two files. The scripted evaluator compares a prescribed ordinary edit with a
new-process native delivery. An independent Python oracle compiles the program and checks all
public outputs before and after. Receiver replay checks exact apply, reversal and reapplication.
The [retained manifest](../tests/agent-eval/results/2026-09-28-structural-change/manifest.json) binds
these artifacts. No model service runs and no token-saving or live-agent claim is made.

The Rust/Lean target admission comparison includes the new operation and an unknown-operation
refusal. Its theorem concerns the language/target predicate. Parser correctness, reference resolution,
source behavior and the full native refactor implementation remain outside that proof.
