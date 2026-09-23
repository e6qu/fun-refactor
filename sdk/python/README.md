# fun-refactor agent SDK for Python

`fr_ir.flow` provides typed control-flow graphs and opt-in `FlowCache` persistence through the
existing Merkle stores. Input changes recompute; unrelated edits renew evidence handles without
renewing mutation authority. See the [flow contract](../../docs/agent-investigations.md#verified-result-reuse).

This zero-dependency package constructs source-free `fr-semantic-body-1` payloads. Its four
namespaces follow the Rust IR hierarchy and retain distinct node types at runtime.

Install the wheel from the same GitHub release as the native binary. Check the exact package,
binary and wire-schema agreement before starting a long agent session:

```python
from fr_ir.runtime import FrClient

client = FrClient(".", executable="fr")
print(client.compatibility().version)
```

`FrClient` also keeps project reports, progressive disclosure and reviewed task-change sessions as
Python objects. It invokes the local `fr` binary without a shell and exposes selected report values
without printing the surrounding JSON:

```python
from fr_ir.context import DirectoryObjectStore
from fr_ir.runtime import FrClient

client = FrClient(".")
found = client.project("find", "render", "--signature")
target = found.definition_target()
session = client.context(
    target.handle, view="evidence",
    store=DirectoryObjectStore("/tmp/fr-objects"),
)
code_map = session.materialize_section("code_map")
packet = session.packet({"code_map": "/model/code_map"}, max_bytes=4096)
```

Keep a directory object store outside the analyzed project so cache creation does not invalidate
its revision. See [the runtime contract](../../docs/agent-runtime-sdk.md) and
[context workspace](../../docs/agent-context-workspace.md) for bounded calls, selected packets,
storage adapters and the reviewed session API.

For a remote content-addressed endpoint that accepts `GET` and idempotent `PUT` at
`<base>/<sha256>`, use the zero-dependency adapter. The application supplies authorization headers;
the SDK bounds responses, refuses redirects, verifies every content address during restoration and
reads each stored record back through `store_merkle_value`.

```python
from fr_ir.http_store import HttpObjectStore

store = HttpObjectStore(
    "https://objects.example/v1/fr",
    headers={"Authorization": "Bearer session-token"},
)
session = client.context(handle, view="evidence", store=store)
```

Start with a structured goal when the agent has not chosen a command or protocol:

```python
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector

guide = client.guide(AgentGoal("understand", selector=GoalSelector(name="render")))
evidence = client.follow_guide(guide.actions()[0])
definition = guide.target.location
if definition is not None:
    print(definition.name.range.start.line, definition.name.range.start.col)
    session = client.context(guide.target.handle)
    print(session.source_text(definition.definition))
```

`AgentGoal` mirrors `fr-agent-goal-1`, including operation, constraints, checks, proof expectations,
context limits and delivery. `AgentGuide` verifies the goal identity and Merkle report; following an
action first revalidates its basis and then checks the exact response contract and byte ceiling.
Exact declaration targets expose typed `DefinitionLocation`, `TextLocation`, `TextRange`,
`TextPosition` and `ByteSpan` values from `fr_ir.runtime`. Byte spans and 1-based Unicode
line/column ranges remain bound to the report revision; `location.text(source)` verifies both
coordinate forms and slices matching UTF-8 source without treating byte offsets as Python character indexes.
`ContextSession.source_text(location)` follows only exact source actions returned by that bound
session, validates every fragment against its source commitment and stops once the requested typed
location is covered. `SourceFragment.location`, `span` and `source_span` remain file-relative, so
`fragment.text_at(location)` accepts a covered AST location directly. Its `offset` remains relative
to the committed declaration for exact protocol paging. The fragment and session verify line ranges
and byte spans against revealed text and the target definition. AST locations stay file-relative for capability authoring.
`ByteSpan` also serves as the optional range of `CapabilityOperation`, so an AST name or definition
span can flow into a reviewed capability without rebuilding a raw mapping.
Direct `project find`, `select`, `map`, `explore` and `show` reports expose typed definitions as
well. `definition_targets()` checks the report revision, response shape, handle and coordinates before
returning the same `AgentTarget` used by guide, disclosure and intent workflows.
`definition_target()` additionally requires exactly one definition and refuses absent or ambiguous
lookups.
`complete_guide` follows read and preview actions without writing. Every writable route instead
uses one `GuideReview`: author the route's typed `TaggedIntentAction`, pass it with the retained
guide to `client.review_guide`, inspect the native review, then pass that unchanged review to
`client.execute_guide`.
For a complete semantic scalar goal, `guide.semantic_scalar_action()` returns the exact typed task
operation already committed by the guide, avoiding a second agent-authored manifest.
See [the language-aware route contract](../../docs/agent-workflow-guide.md) for authored fields,
specialized workflows and the measured freshness cost.

Declare a high-level evidence projection after selecting its exact target:

```python
from fr_ir.intent import AgentIntent

compiled = client.compile(AgentIntent(handle, "trace"))
call_traces = compiled.at("/selected/call_traces")
```

`understand`, `trace`, `change`, `migrate` and `prove` expand to fixed evidence sections. Use
`IntentNeed` names a projection or a pointer inside a purpose-approved section. Native compilation
uses one project snapshot and no progressive subprocess calls. Pass a `MemoryObjectStore` or
`DirectoryObjectStore` to verify and retain each selected subtree. `client.prepare` keeps the
progressive action traversal available for protocol testing and parity checks.

Use tagged actions for intent-bound planning and delivery:

```python
from fr_ir.intent_actions import TaggedIntentAction, TaskChangeOperation

compiled = client.compile(AgentIntent(
    handle, "change", packet_limit=65536,
    action=TaggedIntentAction(TaskChangeOperation(change)),
))
# Inspect compiled.at("/action/review") before executing.
result = client.execute_intent(compiled)
```

`TaskChangeOperation` permits the existing bounded requests and multiple targets. Other operation
mirrors are `AuthorBatchOperation`, `RecipeOperation`, `CapabilityOperation`,
`FrameworkMigrationOperation`, `ApplicationMigrationOperation`, `ProjectQueryOperation`,
`SurfaceEditOperation`,
`PropertyTaskOperation`, `FormalPlanOperation`, `ProofTaskOperation` and
`ProofSubmissionOperation`, all in `fr_ir.intent_actions`. Their fields match the public tagged IR.
Formal plans write scaffolds only when a package, checks and delivery are supplied. Proof properties
and tactics remain agent-authored; checked model theorems retain explicit implementation obligations.

`client.review_guide(guide, TaggedIntentAction(operation))` binds an authored operation to its
retained goal and returns the common immutable review. Native compilation checks that goal and
basis in the same snapshot as evidence.
Every `fraa2:` review commits selected evidence, secondary targets, exact changes, check
configuration, proof expectation and delivery. The SDK independently verifies those identities and
stores all selected Merkle roots when given an object store. `execute_guide` refreshes the guide and
executes only that unchanged `fraa2:` review. Execution preserves checks, requested undo/redo and
patch delivery. Read-only plans cannot execute. The lower-level `compile_guided_intent` remains
available for inspection-only integrations. The legacy `IntentAction` and `fraa1:`
wire format remain available for one direct task target without project requests.

The package root is deliberately empty. Import IR constructors from `fr_ir.ir`, the subprocess
client from `fr_ir.runtime`, progressive storage from `fr_ir.context`, and high-level requests from
`fr_ir.intent`, with workflow goals in `fr_ir.guide`. The package has no `__main__.py` and publishes
no mutable `__all__` registry.

For language-neutral HTTP authoring, `fr_ir.application` provides `RouteBundle`,
`HttpRoute`, `HttpInput`, `Literal`, `Path`, `Input`, `Object` and `Array`. Required query values and
JSON-body fields use explicit string, integer or Boolean scalar contracts. Its exclusive IR-file
write feeds
the reviewed `migrate application` planner. Application hierarchies also support
`FrClient.context(handle, view="application")` and checked Merkle subtree storage.
`StaticComponent`, `StaticElement` and `StaticText` mirror the bounded React/Next.js
intrinsic JSX subset.
See [the application IR contract](../../docs/application-ir.md) for exact boundaries.

Install the current test extra and run the suite with pytest:

```sh
python3 -m pip install -e './sdk/python[test]'
python3 -m pytest -q sdk/python/tests
```

`FormalPlan.from_json(...)` also mirrors `fr-formal-plan-1`, validates every nested field and
independently recomputes its Merkle content address. Agents can inspect and store a plan without
handling source text, then pass the unchanged JSON to `fr spec scaffold --from`.
`ProofTask.from_json(...)` validates a bounded `fr-proof-task-1` context and its Merkle address.
`ProofAttempt.from_json(...)` verifies that an accepted attempt's receipt binds the goal, normalized
agent-written tactics and pinned checker. Neither class generates proof tactics.

`PropertyTask.from_data(...)` validates `fr-property-task-1` and independently recomputes its Merkle
address. `PropertyTerm` and `PropertyProposition` mirror the Rust proposition IR. The task's
`property(...)` method checks names, disclosed types, model arity and the 64-node/16-level ceilings,
then returns an `AgentProperty` with the exact `fr-formal-property-1` wire shape:

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

The property tree contains a theorem proposition and no tactics. Rust revalidates it against the
current model task before planning, and the proof remains agent-authored.

```python
from fr_ir.ir import BinaryOp, Expr, SemanticBody, Stmt

change = SemanticBody([
    Stmt.Return(
        Expr.Binary(BinaryOp.MUL, Expr.Name("value"), Expr.Int(2))
    )
])
change.write("change.json")
```

Inspect only the needed contract variant, validate the emitted value, and preview it against a
revision-bound function handle:

```sh
fr author semantic-schema statement --kind return
fr author validate-semantic --from change.json --canonical
fr author replace-body-semantic HANDLE --from change.json
```

`Type`, `Stmt`, `Expr` and `TemplatePart` reject values from another node category. The SDK exposes
no constructor for either `unsupported` variant and detects cycles during serialization. Rust
performs the final strict field, schema, size and source-free checks.

Build a smaller change when the agent already has a semantic body. The SDK computes the canonical
body identity and infers the replacement category from the typed node:

```python
from fr_ir.ir import Change, Expr, SemanticChange

delta = SemanticChange(change, [
    Change.Replace("/body/0/value/value/right", Expr.Int(3)),
])
delta.write("delta.json")
```

Apply it without scanning a project. Each operation addresses the result of the preceding operation:

```sh
fr author semantic-schema change
fr author apply-semantic-change --body change.json --change delta.json --canonical
```

For scalar edits, role locators avoid serialization-only `value` segments and complete replacement
nodes. The object hierarchy mirrors `fr-semantic-intent-1` directly:

```python
from fr_ir.ir import Intent, LocatorStep, NodeCategory, Role, SemanticIntent

intent = SemanticIntent(change, [
    Intent.SetInt([
        LocatorStep(Role.STATEMENT, index=0, category=NodeCategory.STATEMENT, kind="return"),
        LocatorStep(Role.RESULT, category=NodeCategory.EXPRESSION, kind="binary"),
        LocatorStep(Role.RIGHT, category=NodeCategory.EXPRESSION, kind="int"),
    ], "2", "3")
])
intent.write("intent.json")
```

`LocatorStep` accepts a role plus optional index, category, kind and label witnesses. Rust remains
the authority for resolution against a body and compiles accepted intents through the checked delta
engine. Python rejects malformed portable scalars, invalid roles, empty locators and no-op edits
before serialization.

When `project disclose` returns an opaque scalar edit capability, keep that identity instead of
reconstructing its locator. `DisclosedEditRequest` mirrors the two fields accepted by author batch,
project-task and task-change manifests:

```python
from fr_ir.ir import DisclosedEditRequest

request = DisclosedEditRequest("frde1:<64 lowercase hex digits>", "7")
operation = {"op": "edit-body-disclosed", "handle": handle,
             "disclosed": request.to_data()}
```

The SDK checks the wire shape. Rust rebinds the opaque identity to the current revision,
declaration, body, scalar value and typed role locator before it plans an edit.

Structural disclosure capabilities use the same IR constructors as complete bodies and deltas.
Pass a typed node for replacement or insertion; omit it for a deletion capability:

```python
from fr_ir.ir import DisclosedIrEditRequest, Expr

replacement = DisclosedIrEditRequest(
    "frdi1:<64 lowercase hex digits>", Expr.Int(7)
)
deletion = DisclosedIrEditRequest("frdi1:<64 lowercase hex digits>")
operation = {"op": "edit-body-disclosed-ir", "handle": handle,
             "disclosed_ir": replacement.to_data()}
```

The opaque identity determines the operation, position, and accepted node category. Rust checks
that the optional value shape agrees with that capability and with the current Merkle commitment.

Build one complete reviewed change session without separate fragment files. These classes mirror
the `fr-task-change-1` wire shape and validate request order, target inputs, postconditions, checks,
paths and byte ceilings before writing JSON:

```python
from fr_ir.ir import ProjectReference, ProjectRequest, TaskChange, TaskDelivery, TaskTarget

request = ProjectRequest("target", ["find", "render", "--signature"])
change = TaskChange(
    [request],
    [TaskTarget("body", ProjectReference("target", "/rows/0/0"),
                "replace-body", fragment="{ value.to_uppercase() }")],
    {"files-changed": 1, "edits": 1, "changed-operations": 1,
     "paths-changed": ["src/lib.rs"]},
    ["unit"],
    TaskDelivery(patch="artifacts/change.patch"),
)
change.write("task-change.json")
```

`TaskDelivery` defaults to original-state checks, compact successful evidence and reversal. Rust
revalidates the complete manifest, handles, source, fragments and check declarations under the
reviewed task-change basis.

Disclosure trees can be stored by content address:

```python
from fr_ir.ir import merkle_object_digest, merkle_object_pack, restore_merkle_object

pack = merkle_object_pack(project_evidence)
for digest, record in pack["objects"].items():
    object_store.put(digest, record)

restored = restore_merkle_object(pack["root"], object_store.get)
assert merkle_object_digest(restored) == pack["root"]
```

The callback fetches only objects reachable from that root and each digest at most once, so a client
can retain or evict branches independently. Equal subtrees deduplicate automatically.
`verify_disclosure_commitment` checks an advertised object digest against an opt-in
`project disclose --proofs` path;
`verify_disclosure_proof` hashes a complete fetched value before checking the same path.

The bounded formal evaluator lives in `fr_ir.formal_kernel`. `KernelValue`, `KernelTerm` and
`KernelRequest` mirror the native tagged IR; `FrClient.kernel` returns a validated `KernelResult`.
`KernelEvidence` and `SourceBinding` expose separate IR, term, model and language-neutral signature identities.
`FormalBinding.source_type` aliases the retained legacy wire field.
See [formalization](../../docs/agent-formalization.md) for structural targets and agent-written correspondence proofs.

Typed `Occurrence` values expose exact use sites. `fr_ir.investigation` provides `TaskPlan`,
`TaskStep`, dependencies, evidence records and flow witnesses. Plans persist through the existing
Merkle object store and revalidate dependencies on resume. See the
[investigation contract](../../docs/agent-investigations.md) for supported semantics and boundaries.

`fr_ir.origins.SemanticOrigins` links semantic and authoring pointers to exact, multiple or absent
source origins. `TaskStep.from_guide` retains ready guide arguments and structured input.
`TaskStep.checked` and `fr_ir.investigation_checks.run_checks` bind reviewed check results to source,
configuration and executable identities. Failed checks remain available through verified storage.
