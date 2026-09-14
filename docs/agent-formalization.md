# Agent formalization workbench

`fr spec` can turn a deliberately small Rust function into a source-free, content-addressed
formalization plan and a checked Lean proof workspace. The workflow is designed for agents: each
step returns structured data, exact next commands and only the proof context selected by digest.

## Supported kernel

Candidate generation currently admits top-level, synchronous Rust functions with explicit
parameter and return types. Their bodies may contain pure expressions, immutable `let` bindings,
complete `if` expressions and returns. Literals, names, tuples, lists, Boolean operators,
comparisons, addition, subtraction and multiplication map to Lean. Calls, effects, mutation,
unsafe code, partial indexing, division, remainder and implicit types refuse with a specific
boundary instead of producing a speculative model.

The generated model is deterministic for this subset. That establishes how `fr` translated its
semantic IR; it does not by itself prove equivalence to the compiled Rust function. The plan keeps
source identity, signature correspondence, model generation and implementation/model
correspondence as separate evidence fields.

## Agent workflow

Initialize the checked package once:

```sh
fr --json spec init --write
```

Discover candidates without reading their source text:

```sh
fr --json spec candidates src --limit 32
```

Each row states eligibility, the reason, a declaration hash, property shapes that fit the
signature, and an exact `spec plan` action. Create a plan with only built-in properties you intend
to prove:

```sh
fr --json spec plan src/lib.rs::keep \
  --property identity --property idempotent > formal-plan.json
```

`fr-formal-plan-1` contains typed bindings, normalized semantic IR, the generated Lean definition,
property propositions, assumptions, remaining obligations and exact actions. Its `object_digest`
is the Merkle address of every semantic field; presentation actions are excluded from the address.
The Python `FormalPlan` class reads, validates, round-trips and stores this exact shape.

For a project-specific property, ask for the current model signature and proposition grammar:

```sh
fr --json spec property-task src/lib.rs::both --token-limit 4096 > property-task.json
```

`fr-property-task-1` contains no source body, theorem or proof. It binds the source declaration,
generated model name, input/output types, allowed term and proposition nodes, 8-parameter,
64-node and 16-level ceilings, empty templates and exact actions to one Merkle digest. The agent
authors `fr-formal-property-1` with that digest. Terms can name declared property parameters, call
the model, carry typed integer and Boolean literals, use admitted unary/binary operations and form
typed conditionals. Propositions provide equality, inequality, numeric ordering, Boolean truth,
negation, conjunction, disjunction and implication.

The zero-dependency Python mirror keeps the authored shape adjacent to the protocol:

```python
from fr_ir import PropertyProposition as Prop, PropertyTask, PropertyTerm as Term

task = PropertyTask.from_data(property_task_json)
x, y = Term.variable("x"), Term.variable("y")
property_ = task.property(
    "commutative",
    [{"name": "x", "lean_type": "Bool"},
     {"name": "y", "lean_type": "Bool"}],
    Prop.equals(Term.model(x, y), Term.model(y, x)),
)
property_.write("property.json")
```

Create the plan with the unchanged property tree:

```sh
fr --json spec plan src/lib.rs::both --property-from property.json > formal-plan.json
```

The agent owns the property name, parameters, proposition and every later proof tactic. `fr`
rejects stale task identities, unsafe or duplicate names, types outside the disclosed signature,
unknown variables, model arity errors, type errors and oversized trees. The rendered theorem and
authored tree are both committed by the plan digest.

Scaffolding reparses the plan with unknown-field rejection, regenerates it from the current source
and requires exact equality before preparing a write:

```sh
fr --json spec scaffold --from formal-plan.json --write
```

A changed declaration, type mapping, model, proposition, authored property tree, assumption,
obligation or digest makes the plan stale. Regeneration only changes marked generated regions.
Before a scaffold write, the pinned Lean toolchain elaborates the exact generated module. Named
proof regions retain their bytes, and the existing handwritten extension region remains outside
generated content.

List compact proof commitments, then reveal one exact goal:

```sh
fr --json spec goals specs --token-limit 4096
fr --json spec goals specs --goal DIGEST --token-limit 4096
```

The catalog carries stable content addresses and reveal vectors. A selected detail discloses the
theorem header, source and signature anchors, proof-region name, proof command and verify command.
The serialized report stays within the requested byte ceiling. Clients can retain goal details by
digest in the same object-storage pattern as project disclosure.

Create a proof task without reading or reproducing the model file:

```sh
fr --json spec proof-task \
  specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity --token-limit 4096
```

`fr-proof-task-1` binds the selected goal, input contract, checker and exact actions to one content
address. Its direct and calculation templates contain only positions for agent-written tactics.
They do not contain a proof. The agent writes the proof file, then asks Lean to check it without a
workspace mutation:

```sh
printf 'rfl\n' > proof.lean
fr --json spec proof-check \
  specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity --from proof.lean
fr --json spec prove specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity \
  --from proof.lean --write
fr --json spec verify specs
```

`proof-check` returns `fr-proof-attempt-1`. A successful receipt binds the current goal, normalized
inserted tactics and pinned Lean checker. A failed attempt returns structured diagnostics within the selected
byte ceiling and no apply action. `spec prove` repeats that Lean check before it prepares history.
It rejects a leading `by`, proof placeholders, debt markers and ambiguous proof regions. The
transaction supports `history undo`, `history redo`, plan review and Git patch export. `spec verify`
remains the authority for strict anchors and a warnings-as-errors Lake build.

## Verification boundary

`FrKernels.FormalPlan` proves the Boolean admission policies and abstract property operator and
relation rules. The Rust test compares all 574 finite inputs with the Lean executable. Integration tests cover candidate exclusions,
plan addressing, plan tampering and drift, source-free output, proof preservation, bounded goal
disclosure, agent-authored multi-input properties, failed and corrected proof attempts, exact proof
replacement, undo and redo. Python independently recomputes plan, property-task, proof-task and
receipt addresses and rejects mutation.

Lean checks the proposition over the generated Lean definition. Source anchors check declaration
identity, signature maps check the declared type surface, and executable cases can test selected
Rust/model inputs. Parser correctness, semantic-IR extraction, Rust-to-Lean lowering, SHA-256,
Lean's implementation, the Rust compiler and filesystem operations remain trusted or tested
components. General implementation equivalence remains an explicit obligation until a verified
generation or correspondence proof covers it.
