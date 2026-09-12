# fun-refactor semantic IR for Python

This zero-dependency package constructs source-free `fr-semantic-body-1` payloads. Its four
namespaces follow the Rust IR hierarchy and retain distinct node types at runtime.

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
fr author semantic-schema expression --kind binary
fr author validate-semantic --from change.json --canonical
fr author replace-body-semantic HANDLE --from change.json
```

`Type`, `Stmt`, `Expr` and `TemplatePart` reject values from another node category. The SDK exposes
no constructor for either `unsupported` variant and detects cycles during serialization. Rust
performs the final strict field, schema, size and source-free checks.
