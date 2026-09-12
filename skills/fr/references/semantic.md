# Read and write semantic structure

Use `fr project semantic PATH` to inspect declarations and types without source. Add `--body` only
for the implementation being changed. Set `--nodes` to the smallest useful complete-model budget.
A known unique target can use `--declaration NAME` and return its full handle in the same call.
Add `--minimal` after the target is exact and workspace-wide coverage is not needed. Its
`report_omitted` lists omitted project-envelope fields.
A null model with `omitted-node-budget` needs the reported node count; never infer omitted children.

Use a full declaration handle when selecting one function or method. Keep `semantic_basis` beside
any nested `BASIS#POINTER` addresses. Source or inventory drift invalidates project handles.
Unsupported source is hashed and counted by default. Reserve `--unsupported-source` for a task that
requires the exact unsupported syntax because it exposes text.

Pattern rows describe IR shapes such as filter-map, optional-branch, variant-match,
failure-propagation and deferred-cleanup. Treat `behavior_proved: false` as a firm claim boundary.

For an implementation change, inspect only the needed contract entries. `fr author semantic-schema`
returns the bounded index. Add a section and `--kind` for one exact JSON shape and its Python
constructor. These calls do not scan a project.

Use the Python SDK when it is available. Its `Type`, `Stmt`, `Expr` and `TemplatePart` namespaces
match the IR hierarchy and reject category mistakes before serialization:

```python
from fr_ir import BinaryOp, Expr, SemanticBody, Stmt

change = SemanticBody([
    Stmt.Return(Expr.Binary(BinaryOp.MUL, Expr.Name("value"), Expr.Int(2)))
])
change.write("change.json")
```

Direct JSON remains a portable route. Write schema `fr-semantic-body-1`; its `body` is a list of
tagged statements from the returned model. Keep `kind` and `value`. Never add `source` or
`unsupported` nodes.

```json
{
  "schema": "fr-semantic-body-1",
  "body": [{
    "kind": "return",
    "value": {
      "kind": "binary",
      "value": {
        "op": "mul",
        "left": {"kind": "name", "value": "value"},
        "right": {"kind": "int", "value": "2"}
      }
    }
  }]
}
```

For smaller basis-bound edits, load [Semantic change](semantic-change.md).

Run `fr author validate-semantic --from FILE --canonical`, then preview
`fr author replace-body-semantic HANDLE --from FILE`. Review the rendered diff and writer
fidelity. Save or apply it through the same plan, history and checked-workflow route as other author
operations. Author batches, project tasks and task changes use operation `replace-body-semantic`.
The route supports Rust, Go, Java, TypeScript and TSX function-body targets. A refusal means the IR
cannot be lowered inside one body under the current contract; inspect the reported boundary.
