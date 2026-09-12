# Make a semantic intent change

Use this route for an exact scalar edit inside one supported function or method. It avoids raw
source, serialization-only pointer segments and complete replacement nodes.

Query a filtered locator index when the operation and current value are known:

```text
fr project semantic PATH --declaration NAME --body --locators --locators-only --locator-op set-int --locator-from 1 --nodes 128 --minimal
```

Retain `selection.handle`, `body_identity.basis` and one exact `body_locators.rows` entry. An empty
complete row set means no supported scalar matches both filters. Broaden only the uncertain filter.
Without `--locators-only`, the report also returns the semantic body for structural context.

Each row supplies a copyable `target`, typed category and kind, operation, and exact `from` value.
Put the target into `fr-semantic-intent-1`, provide a different `to` scalar and keep the body basis:

```json
{
  "schema": "fr-semantic-intent-1",
  "base": "frsb1:<CANONICAL_BODY_SHA256>",
  "operations": [{
    "op": "set-int",
    "target": [
      {"role": "statement", "index": 0, "category": "statement", "kind": "let"},
      {"role": "initializer", "category": "expression", "kind": "binary"},
      {"role": "right", "category": "expression", "kind": "int"}
    ],
    "from": "1",
    "to": "2"
  }]
}
```

Inspect `fr author semantic-schema intent` for all roles, operations and scalar rules. Integer and
float values use portable unsigned decimal strings. Negative values are represented by the IR's
unary node. Name, field and keyword values are ASCII identifiers. Boolean values are JSON Booleans. Operations are
ordered and each locator resolves against the previous operation's result.

The Python mirror follows the same contract:

```python
from fr_ir import Intent, LocatorStep, SemanticIntent

target = [LocatorStep(**step) for step in locator_row["target"]]
change = SemanticIntent(body_basis, [Intent.SetInt(target, "1", "2")])
change.write("intent.json")
```

Direct JSON is the measured default for one small edit. The SDK helps when constructing several
typed operations or reusing Python logic. Use a pointer delta for complete-node replacement or
statement insertion/deletion. Use a complete semantic body when most of the body changes.

With a body file, run `fr author apply-semantic-intent --body BODY --intent INTENT --canonical
--compiled`. It checks direct interpretation against the compiled `fr-semantic-change-1` result.
For a project change, preview `fr author edit-body-intent HANDLE --from INTENT`, review the complete
diff and writer fidelity, then use the normal task-change or saved-history lifecycle. Author batches,
project tasks and task changes use `edit-body-intent`.

A missing or ambiguous role, category or kind mismatch, stale `from`, invalid scalar, no-op, stale
base or invalid intermediate body refuses before source and history mutation. This is a structural
scalar edit. It does not rename bindings or prove program behavior.
