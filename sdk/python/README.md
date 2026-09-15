# fun-refactor agent SDK for Python

This zero-dependency package constructs source-free `fr-semantic-body-1` payloads. Its four
namespaces follow the Rust IR hierarchy and retain distinct node types at runtime.

`FrClient` also keeps project reports, progressive disclosure and reviewed task-change sessions as
Python objects. It invokes the local `fr` binary without a shell and exposes selected report values
without printing the surrounding JSON:

```python
from fr_ir.context import DirectoryObjectStore
from fr_ir.runtime import FrClient

client = FrClient(".")
found = client.project("find", "render", "--signature")
handle = found.at("/rows/0/0")
session = client.context(
    handle, view="evidence",
    store=DirectoryObjectStore("/tmp/fr-objects"),
)
code_map = session.materialize_section("code_map")
packet = session.packet({"code_map": "/model/code_map"}, max_bytes=4096)
```

Keep a directory object store outside the analyzed project so cache creation does not invalidate
its revision. See [the runtime contract](../../docs/agent-runtime-sdk.md) and
[context workspace](../../docs/agent-context-workspace.md) for bounded calls, selected packets,
storage adapters and the reviewed session API.

Declare a high-level evidence goal instead of manually sequencing section traversal:

```python
from fr_ir.intent import AgentIntent

prepared = client.prepare(AgentIntent(handle, "trace"))
call_traces = prepared.at("/selected/call_traces")
```

`understand`, `trace`, `change`, `migrate` and `prove` expand to fixed evidence sections. Use
`IntentNeed` for named projections or a pointer inside a section. The runtime keeps intermediate
reports local, follows sibling branches through the retained action graph and admits only a packet
within the declared call and byte ceilings.

The package root is deliberately empty. Import IR constructors from `fr_ir.ir`, the subprocess
client from `fr_ir.runtime`, progressive storage from `fr_ir.context`, and high-level requests from
`fr_ir.intent`. The package has no `__main__.py` and publishes no mutable `__all__` registry.

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
