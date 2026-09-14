# fun-refactor semantic IR for Python

This zero-dependency package constructs source-free `fr-semantic-body-1` payloads. Its four
namespaces follow the Rust IR hierarchy and retain distinct node types at runtime.

`FormalPlan.from_json(...)` also mirrors `fr-formal-plan-1`, validates every nested field and
independently recomputes its Merkle content address. Agents can inspect and store a plan without
handling source text, then pass the unchanged JSON to `fr spec scaffold --from`.

```python
from fr_ir import BinaryOp, Expr, SemanticBody, Stmt

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
from fr_ir import Change, Expr, SemanticChange

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
from fr_ir import Intent, LocatorStep, NodeCategory, Role, SemanticIntent

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
from fr_ir import DisclosedEditRequest

request = DisclosedEditRequest("frde1:<64 lowercase hex digits>", "7")
operation = {"op": "edit-body-disclosed", "handle": handle,
             "disclosed": request.to_data()}
```

The SDK checks the wire shape. Rust rebinds the opaque identity to the current revision,
declaration, body, scalar value and typed role locator before it plans an edit.

Structural disclosure capabilities use the same IR constructors as complete bodies and deltas.
Pass a typed node for replacement or insertion; omit it for a deletion capability:

```python
from fr_ir import DisclosedIrEditRequest, Expr

replacement = DisclosedIrEditRequest(
    "frdi1:<64 lowercase hex digits>", Expr.Int(7)
)
deletion = DisclosedIrEditRequest("frdi1:<64 lowercase hex digits>")
operation = {"op": "edit-body-disclosed-ir", "handle": handle,
             "disclosed_ir": replacement.to_data()}
```

The opaque identity determines the operation, position, and accepted node category. Rust checks
that the optional value shape agrees with that capability and with the current Merkle commitment.

Disclosure trees can be stored by content address:

```python
from fr_ir import merkle_object_digest, merkle_object_pack, restore_merkle_object

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
