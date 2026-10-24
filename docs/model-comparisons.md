# Compare a small access rule before and after a change

Suppose you edit a Boolean access rule and want to check whether it allows the same inputs as before. Save the old supported model, then compare the new one.
The result concerns those models and their stated assumptions. It does not establish that every part of the application behaves the same.

A Boolean value is `true` or `false`. A model describes the supported Boolean calculation as data for a proof.
Equivalent means both models return the same answer for every input covered by the comparison.
Refinement here means every input accepted by the new model was also accepted by the old one.
For an access rule, replacing “staff or owner” with “staff and owner” is stricter but changes some answers.
The Python snippets assume an initialized proof package, a `FrClient` named `client` and an object store named `store`.

## What the comparison checks

Capture a supported source function before changing it. After the change, request equivalence or
refinement between its retained model and a newly captured model. Rust and Python functions need
explicit Boolean parameters and a Boolean result. The route admits at most eight parameters per
function and the existing pure formalization subset. Calls, effects and integer arithmetic refuse.

The first snippet continues an existing session: `client` is a `FrClient` for your project and `store` is its external object store.
`ModelSnapshot` saves a supported calculation before editing. `ModelComparison` prepares the claim to check against the later function.
The tuple `(1, 0)` maps the new function's first argument to the old second argument, and its second to the old first.
Initialize the proof package with `fr spec init` before using this sequence; it is an integration fragment, not a standalone program.

```python
from fr_ir.refinement import ModelSnapshot, ModelComparison

before = ModelSnapshot.capture(client, "subject.rs::allowed")
snapshot_root = before.persist(store)
# Deliver and check the source change through the normal reviewed workflow.
before = ModelSnapshot.restore(store, snapshot_root)
comparison = ModelComparison("Preserved", before, "subject.rs::permits", (1, 0))
review = comparison.review(client)
# Inspect both generated models, the argument map and the complete file diff.
comparison.execute(client, review)
```

Initialize the package with `fr spec init` first. `spec snapshot TARGET` returns the native snapshot.
`spec compare --from REQUEST --package specs` previews a comparison. Writing or saving requires its
unchanged `--basis`. Existing comparison files refuse replacement; use a new name for a new claim.
Names start with a capital ASCII letter and contain at most 64 letters or digits.

The argument map supplies each new parameter with an old parameter index. `(1, 0)` swaps the first
two arguments. Omitting an old argument supports unused-parameter removal. Repeated indices express
an explicit restriction to equal arguments. They do not establish equivalence for independent new
inputs. Quantification covers every Boolean assignment to the old parameters.

`equivalent` requires equal model outputs. `refines` requires that a true new output implies a true
old output. This direction permits stricter acceptance rules. It does not express arbitrary refinement
of effects, errors, state or termination.

Illustration: changing `staff or owner` to `staff and owner` makes the model's access rule stricter.
The new rule can satisfy `refines`, because every newly allowed input was allowed before.
It fails `equivalent`, because staff alone no longer grants access. Choose the relation that matches
your requirement, then check the remaining connection between the models and the actual source.

## Bounded source and virtual workspaces

UTF-8 encodes text as bytes. For example, `é` occupies two bytes, so byte limits can differ from character counts.

Capture accepts at most 64 KiB of source before parsing. It derives the model from that retained
text in an isolated in-memory workspace, then checks that the caller's source is unchanged.
Requests are limited to 256 KiB and retained manifests to 512 KiB. Limits count UTF-8 bytes.
Oversized virtual files are rejected before copying; native reads stop after the limit plus one
byte. Other formalization routes retain their existing limits.

Native library callers can use `vfs::with_handle` for scoped virtual workspaces. Model capture,
comparison previews, package initialization plans and snapshot validation work without a physical
workspace directory. Validation replays retained source in memory and restores the caller's
workspace on success, error or unwinding. Scopes run synchronously on the calling thread; writes
to the supplied handle persist. Native path checks still refuse symlink traversal.
Lean checking and proof execution continue to require a native toolchain and filesystem package.

The [virtual workspace task](../tests/agent-eval/virtual-model-workspaces/task.json) pins the
boundaries and regression cases. The shared VFS byte and text readers enforce the same size limits
on disk and in memory.

## Proofs and resumption

Each comparison creates a Lean module and an adjacent `.refinement.json` manifest. The manifest
retains both source texts and generated models. Native validation regenerates each model from its
retained source, then compares the new snapshot with current workspace source. Changes to generated
definitions, the relation or argument map refuse. A missing manifest also refuses.

The module starts with visible proof debt. Use the existing `spec proof-task`, `spec proof-check` and
`spec prove` commands to author and check the `preserves` theorem. Finite Boolean claims can use
agent-authored `decide` tactics. A false claim fails Lean checking. Generated model definitions alone
disable the unused-variable linter, allowing historical unused parameters. Proof warnings remain fatal.

```python
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_proofs import ProofReport, run_proofs

plan = TaskPlan("Check model preservation", ("same outputs",), (
    TaskStep.proved("models", "Are the model outputs equal?",
        proofs=(comparison.requirement(),), satisfies=("same outputs",)),
))
run = run_proofs(plan, client, "models", ProofReport.review(client), store)
assert run.passed and run.resumed.complete
```

Retained reports expose `evidence.model_comparisons`, including snapshot digests, relation, argument
map and checked status. Source, package, baseline, proof and checker changes invalidate dependent
steps. Restored plans revalidate their inputs in a fresh process. A stored proof never authorizes a
source write. Saved complete source contents conservatively invalidate after unrelated edits in that file.

## Scope and evidence

These theorems concern generated Boolean models. Parsing, extraction and the connection between source and its model
remain trusted or unproved. Historical snapshots retain content identity; they do not authenticate a
repository revision. Numeric overflow, runtime types, external consumers and general translation
semantics remain outside this contract. `source_implementation_proved` stays false.

The [pinned task](../tests/agent-eval/refinement/task.json) covers Rust rename and parameter removal,
Rust/Python equivalence and a stricter Boolean rule. Its independent oracle compiles Rust and checks
all eight input assignments against Python expectations. The scripted evaluator compares
prescribed text edits with reviewed native delivery, retains false-claim refusal, and replays both
patches with exact reversal. Fresh receiver proof checks regenerate the models from retained source.
The [evaluator](../tools/refinement-acceptance.py) records these outcomes for tested cases without a live-agent or
token-saving claim.

The [retained artifacts](../tests/agent-eval/results/2026-09-28-virtual-model-comparisons/manifest.json)
and [SDK regressions](../sdk/python/tests/test_refinement.py) cover the complete lifecycle,
zero and eight parameters, source and model tampering, stale reviews, proof failures, size limits,
saved-plan reversal and current formal-model validation.
