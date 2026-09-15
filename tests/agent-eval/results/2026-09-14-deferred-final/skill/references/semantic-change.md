# Make a checked semantic change

Use this route for a small typed body edit without resending the complete body. Query one exact
function with `fr project semantic PATH --declaration NAME --body --pointers --nodes N --minimal`.
Choose the smallest `N` that returns the complete model.

Retain `selection.handle` and `body_identity.basis`. A missing identity means the body carries source
or exceeds authoring limits. `body_pointers.fields` names compact row columns. Each row gives one
bounded path, exact category and kind under the same basis.

`fr-semantic-change-1` has `base` and 1 through 64 ordered operations. Each operation sees the
previous result. `replace` needs a pointer, reported category and typed node. `insert-statement`
needs a statement-list path, index and statement. `delete-statement` needs a statement path.

Follow every key in the reported path. An integer on the right of a binary inside a `let` value can
use `/body/0/value/value/value/right`. Inspect wire shapes with
`fr author semantic-schema change` and the exact node section and kind.

The Python SDK mirrors the operation hierarchy:

```python
from fr_ir import Change, Expr, SemanticChange

change = SemanticChange(body_basis, [
    Change.Replace(pointer, Expr.Int(2)),
])
change.write("change.json")
```

Use `SemanticChange(body, operations)` when the SDK already holds a complete `SemanticBody`.
Portable JSON uses this shape:

```json
{
  "schema": "fr-semantic-change-1",
  "base": "frsb1:<CANONICAL_BODY_SHA256>",
  "operations": [{
    "op": "replace",
    "path": "/body/0/value",
    "category": "expression",
    "value": {"kind": "int", "value": "2"}
  }]
}
```

Preview with `fr author edit-body-semantic HANDLE --from change.json`. The preview validates the
delta against the current body. `validate-semantic` accepts complete bodies only. When a complete
body file is available, `fr author apply-semantic-change --body BODY --change CHANGE --canonical`
tests pure composition without scanning a project.

Review the complete diff and writer fidelity, then use the normal saved-plan, history or checked
workflow route. Author batches, project tasks and task changes use `edit-body-semantic`. A stale base,
bad pointer, category crossing, invalid intermediate body or no-op refuses before source or history
mutation. The final writer, reparse and byte-preservation checks match whole-body replacement.
