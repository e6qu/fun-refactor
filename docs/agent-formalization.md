# Agent formalization workbench

`fr spec` turns admitted declarations or structural snapshots into source-free, content-addressed
formalization plans and a checked Lean workspace. Each step returns structured data, exact actions
and selected proof context. The agent writes the properties and proof tactics.

## Supported kernel

Typed, synchronous pure functions can formalize from Rust, Go, Java static methods, Python,
TypeScript, TSX, Zig and Lean. Bodies admit immutable bindings, complete conditionals, returns,
Boolean operations, selected integer arithmetic, strings, tuples and homogeneous lists.
Candidate discovery retains refusals for untyped JavaScript/JSX, Bash, async handlers and other
excluded declarations. Calls, effects, implicit receivers, mutation, defaults, floating-point
coercion, partial source access and division policies require separate models.

The direct `fr-pure-kernel-1` evaluator additionally represents records, options, results, field
access and indexing. It uses nearest-first indexed bindings and checked signed-64 arithmetic.
Division truncates toward zero; remainder follows the dividend sign. Overflow, division by zero,
missing bindings, type errors and missing fields/indexes produce explicit failures.
Fuel ranges from 1 to 256, with 64 environment values, 4,096 input nodes and depth 64.
Request and response ceilings limit serialized bytes separately.

Generated source models use mathematical Lean `Int` or `Nat` arithmetic. Numeric source widths,
overflow and language coercion remain separate obligations. Evaluation and translation alone
supply no implementation equivalence proof.

## Structural targets

For HTML, CSS, SCSS, Sass, HCL, JSON, YAML, Helm, XML and Markdown, use
`PATH::__fr_structure__`. The model checks retained byte spans, name containment and ordered
immediate parent links. Each fact kind retains at most 32 rows; gap diagnostics retain 16.
The IR records omitted counts, clipped names, confidence and its provenance policy.
Parser completeness, omitted facts, rendering, configuration behavior and embedded execution remain unproved.
Markdown containing Mermaid and HTML carrying Tailwind classes use this structural boundary.
A file selector with a `formalize` guide goal selects this same workflow.

Choose `retained-facts-wellformed` to create the structural validity obligation. Choose `ir-model`
to relate the reviewed evaluator to the generated model. Both require agent-written tactics.

## Kernel/model correspondence

`ir-model` accepts at most eight Boolean inputs and a Boolean output, including structural snapshots.
Its theorem states that Lean's reviewed kernel evaluator at fuel 256 returns the generated model's value.
Named term definitions keep the theorem context small for large admitted snapshots.
Goal identities include the selected model module outside generated proof bodies and the reviewed library.
The package includes the exact reviewed semantic library. Scaffolding elaborates before history writes;
proof checking needs no manual dependency build. Regeneration preserves the written proof regions.
Strict checks refuse a changed semantic library. Typed targets anchor declarations; structural targets
anchor the complete source file.

`spec evidence` reports `kernel_correspondence` rows with separate source, IR, term, model and
semantic-library identities. A row reaches `checked_by_lean` only when the checked package retains
the current generated model, named term definition and exact correspondence proposition.
`source_implementation_proved` remains false. Parser extraction and source semantics still require evidence.
General Lean laws cover binding resolution, shadowing, lifted indices, literal substitution,
evaluation determinism, short-circuit behavior and arithmetic bounds. A conditional multiplication/addition
law and a concrete grouping fixture cover operator-tree evaluation; parser lowering remains separate.
Execution tests compare finite cases with executable Lean and installed source toolchains.
They do not prove general source implementation equivalence.

## Direct evaluation and Python mirror

```python
from fr_ir.formal_kernel import KernelRequest, KernelTerm, KernelValue
from fr_ir.runtime import FrClient

term = KernelTerm.let(
    KernelTerm.literal(KernelValue("int", 3)),
    KernelTerm.binary("add", KernelTerm.bound(0), KernelTerm.bound(1)),
)
result = FrClient(".").kernel(KernelRequest(term, (KernelValue("int", 7),)))
assert result.passed and result.value == KernelValue("int", 10)
```

`fr spec kernel --from REQUEST --report-bytes 65536` accepts a regular JSON file or stdin (`-`).
It reads no project source and writes no history. An admitted evaluation failure returns
`passed: false` and a tagged failure; malformed requests or response ceilings fail the command.
The SDK checks exact fields, native value bounds, policies, request identity and result identity.
`FormalKernel.evaluation` mirrors language-neutral `SourceBinding` and `KernelEvidence` metadata.
The legacy `FormalBinding.rust_type` wire field remains available through the `source_type` alias.

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
from fr_ir.ir import PropertyProposition as Prop, PropertyTask, PropertyTerm as Term

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
relation rules. The Rust test compares 574 finite admission inputs with the Lean executable.
The resource-policy comparison adds 135 shared Rust/Python/Lean inputs.
The pure evaluator corpus adds 913 Rust/Lean executions, including arithmetic edges and structured values. Integration tests cover candidate exclusions,
plan addressing, plan tampering and drift, source-free output, proof preservation, bounded goal
disclosure, agent-authored multi-input properties, failed and corrected proof attempts, exact proof
replacement, undo and redo. Python independently recomputes plan, property-task, proof-task and
receipt addresses and rejects mutation.

Lean checks the proposition over the generated Lean definition. Source anchors check declaration
identity, signature maps check the declared type surface, and executable cases can test selected
Rust/model inputs. Parser correctness, semantic-IR extraction, source-to-Lean lowering, SHA-256,
Lean's implementation, source compilers and filesystem operations remain trusted or tested
components. General implementation equivalence remains an explicit obligation until a verified
generation or correspondence proof covers it.

## Retaining verification across revisions

`spec verify` and `spec evidence` now check every selected module after the package build.
An unimported module cannot inherit a successful build's theorem status.
Use `spec retain PACKAGE` for a review of bounded, dependency-free packages generated by `spec init`.
Execute with `--run --basis DIGEST` to retain a clean build and direct module results with their inputs.
[Investigation proof requirements](agent-investigations.md#retained-model-proofs) bind named theorems to resumable steps.
Retained reports distinguish Lean model proofs from source implementation correspondence.
