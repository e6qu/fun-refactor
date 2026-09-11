# Semantic project model

`fr project semantic TARGET` returns cross-language program structure without returning source text.
`TARGET` can be an indexed file path or a revision-bound declaration handle. The public contract is
`fr-semantic-model-1`.

```sh
fr project semantic src/lib.rs
fr project semantic src/lib.rs --declaration calculate
fr project semantic '<HANDLE>' --body --nodes 256 --minimal
```

The default report retains declarations, parameters, types and containment, and omits executable
bodies. `--body` requests a complete body. `--nodes` limits tagged IR nodes from 1 through 4,096.
`--declaration NAME` selects one unique declaration directly inside a file path and returns its full
handle; ambiguous names require a handle from the structural project queries.
`--minimal` omits the generic project envelope after establishing the revision-bound semantic
identity. `report_omitted` lists those fields. Omit this flag when workspace-wide coverage or a
reusable project context basis matters.
When the model does not fit, `status` is `omitted-node-budget`, `model` is null and
`node_budget.required` gives the needed size. The command never clips a returned subtree.

The default `source_policy` is `source-free`. Unsupported IR nodes retain only byte counts and
SHA-256 identities. `--unsupported-source` is an explicit diagnostic escape hatch and can expose
the original unsupported text. Semantic analysis refuses files above 262,144 source bytes; use
structural project queries to select a smaller unit.

`semantic_basis` binds the project revision, selected handle and returned model. Nested addresses
have the form `SEMANTIC_BASIS#RFC6901_JSON_POINTER`, so an agent can name an exact expression or
statement without copying source. Any source or project-inventory change invalidates declaration
handles before a later query or edit.

`patterns` reports syntax-derived IR shapes. Current rows cover map and filter-map comprehensions,
optional branches and loops, variant matches, failure propagation, deferred cleanup, exception
regions, suspension and higher-order functions. Each row says `behavior_proved: false`; it is a
structural observation rather than a behavioral proof.

## Typed body authoring

Inspect the contract before constructing a body. These commands do not build a project index:

```sh
fr author semantic-schema
fr author semantic-schema statement --kind return
fr author semantic-schema expression --kind binary
```

`fr author replace-body-semantic HANDLE --from FILE` accepts a source-free body with schema
`fr-semantic-body-1`:

```json
{
  "schema": "fr-semantic-body-1",
  "body": [
    {
      "kind": "return",
      "value": {
        "kind": "binary",
        "value": {
          "op": "mul",
          "left": {"kind": "name", "value": "value"},
          "right": {"kind": "int", "value": "2"}
        }
      }
    }
  ]
}
```

The JSON contract uses kebab-case `kind` tags and `value` payloads. Unknown fields refuse.
The input may contain at most 512 top-level statements and 4,096 tagged semantic nodes. Any
`source` field or `unsupported` node refuses, preventing source fragments from crossing this route.
Run `fr author validate-semantic --from FILE --canonical` before selecting a project. It returns
the canonical Rust identity, top-level statement count and complete semantic node count.

The zero-dependency package under `sdk/python` mirrors the Rust namespaces while hiding wire-format
details:

```python
from fr_ir import BinaryOp, Expr, SemanticBody, Stmt

change = SemanticBody([
    Stmt.Return(Expr.Binary(BinaryOp.MUL, Expr.Name("value"), Expr.Int(2)))
])
change.write("change.json")
```

`Type`, `Stmt`, `Expr` and `TemplatePart` produce distinct node classes and reject category
crossings immediately. The package has no `Unsupported` constructor. Its exhaustive fixture covers
every authorable constructor and passes Rust deserialization and canonical serialization.

The operation selects the existing function model, replaces its IR body and renders one function
through the writer for the target language. It currently supports the same Rust, Go, Java,
TypeScript and TSX targets as source body replacement. Rendering must carry no source verbatim and
produce exactly one outer function. The existing authoring path then validates the complete body,
splices only its body span and reparses the unchanged destination context.

The report retains semantic input and rendered-body hashes, node count and writer fidelity.
Preview, saved plans, batch steps, `task-change`, checks, undo, redo and patch delivery use the
existing review bases and drift checks. Use `replace-body-semantic` as the operation name in author,
project-task and task-change manifests.

The Lean project model proves whole-model budget admission. The Lean author model proves that
admission requires the exact schema, a supported target, source-free input and bounded size. Shared
executions cover selected numeric boundaries, every Boolean admission state and the expanded task
target matrix. The anchored catalog model proves uniqueness within each finite category, refusal of
both unsupported kinds and category separation. All 155 category/index cases agree with Rust.
These proofs do not cover parser correctness, JSON deserialization, Python execution, SHA-256 or
writer semantics.
