# Semantic project model

`fr project semantic TARGET` returns cross-language program structure without returning source text.
`TARGET` can be an indexed directory or file path, or a revision-bound declaration handle. A
directory requires `--declaration NAME` and must contain exactly one eligible declaration with that
name. The public contract is `fr-semantic-model-1`.

```sh
fr project semantic src/lib.rs
fr project semantic src/lib.rs --declaration calculate
fr project semantic . --declaration calculate --body --locator-op set-int \
  --locator-from 1 --intent-to 7 --nodes 256 --minimal
fr project semantic '<HANDLE>' --body --nodes 256 --minimal
```

The default report retains declarations, parameters, types and containment, and omits executable
bodies. `--body` requests a complete body. `--nodes` limits tagged IR nodes from 1 through 4,096.
`--declaration NAME` selects one unique eligible declaration below a file or directory path and
returns its full handle. Zero or multiple matches refuse; ambiguous names require a handle from the
structural project queries.
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

The first fresh Luna/low pair produced exact canonical values through both routes. Direct JSON used
fewer commands and 65.7% fewer input tokens after reported cache hits; the SDK producer used 51.5%
fewer bytes. Treat direct JSON as the measured default for a small one-off body. Prefer the SDK when
early category checks or reuse matters, and measure broader tasks before claiming a context saving.
See `docs/agent-ir-sdk-evaluation.md` for the conditions and limits.

## Checked semantic deltas

A complete bounded body report includes `body_identity` with schema `fr-semantic-body-1`, an
`frsb1:` basis, statement and node counts, and `source_free: true`. Source-bearing or oversized
bodies report why this identity is unavailable.

`fr-semantic-change-1` binds up to 64 ordered operations to that basis. A replacement names a
bounded RFC 6901 path, its `type`, `statement`, `expression` or `template` category, and one typed
value. Statement insertion names a list path, index and statement. Statement deletion names the
selected statement path. The catalog is available through `fr author semantic-schema change`.

Add `--pointers` to a single-declaration body query when a delta needs exact addresses. The bounded
`fr-semantic-body-pointers-1` list carries the same body basis and gives each authorable node's path,
category and kind. Paths that exceed the change contract's limit do not appear.

The pure `fr author apply-semantic-change --body BODY --change CHANGE --canonical` command checks a
delta without scanning a project. It refuses malformed or escaped paths, missing targets, category
crossings, unsupported or source-bearing nodes, invalid indices, stale bases, no-ops and any invalid
intermediate body. Each operation resolves against the previous canonical result.

The Python SDK mirrors the same operation hierarchy:

```python
from fr_ir import Change, Expr, SemanticChange, Stmt

change = SemanticChange(body_basis, [
    Change.Replace("/body/0/value", Expr.Name("replacement")),
    Change.InsertStatement("/body", 1, Stmt.Return(Expr.Int(0))),
])
change.write("change.json")
```

`fr author edit-body-semantic HANDLE --from CHANGE` reconstructs the current function body and
checks the basis. It applies the delta through the semantic writer and exact body splice. Author batches,
project tasks and reviewed task changes accept `edit-body-semantic`. Their normal preview, drift,
checks, reversal and patch rules apply.

The [controlled comparison](semantic-delta-evaluation.md) measures one small edit against complete
body replacement. It reports the smaller delta input, larger receipt output and identical checked
lifecycle result.

The operation selects the existing function model, replaces its IR body and renders one function
through the writer for the target language. It currently supports the same Rust, Go, Java,
TypeScript and TSX targets as source body replacement. Rendering must carry no source verbatim and
produce exactly one outer function. The existing authoring path then validates the complete body,
splices only its body span and reparses the unchanged destination context.

The report retains semantic input and rendered-body hashes, node count and writer fidelity.
Preview, saved plans, batch steps, `task-change`, checks, undo, redo and patch delivery use the
existing review bases and drift checks. Use `replace-body-semantic` or `edit-body-semantic` as the
operation name in author, project-task and task-change manifests.

The Lean project model proves whole-model budget admission. The Lean author model proves that
admission requires the exact schema, a supported target, source-free input and bounded size. Shared
executions cover selected numeric boundaries, every Boolean admission state and the expanded task
target matrix. The anchored catalog model proves uniqueness within each finite category, refusal of
both unsupported kinds and category separation. All 155 category/index cases agree with Rust.
These proofs do not cover parser correctness, JSON deserialization, Python execution, SHA-256 or
writer semantics.

The semantic-change model adds source-anchored admission, result-bound, pointer-bound and statement
index predicates. Lean proves that accepted changes have matching bases, 1 through 64 operations,
at most 512 top-level statements and 4,096 nodes. It also proves insertion and deletion count laws.
Executable comparisons cover all Boolean states and selected numeric boundaries. These results do
not prove RFC 6901 parsing, Serde, SHA-256, tree replacement or writer correctness.

## Semantic intents

`fr-semantic-intent-1` expresses shape-preserving scalar edits through semantic roles. A locator
uses roles such as `statement`, `initializer`, `condition`, `callee`, `left`, `right`, `operand`,
`element` and `template-part`. Optional category, kind, index and label fields act as exact
witnesses. List roles never choose an implicit first item.

Use a filtered locator-only query when the operation and current scalar are known:

```sh
fr project semantic src/lib.rs --declaration calculate --body --locators \
  --locators-only --locator-op set-int --locator-from 1 --nodes 128 --minimal
```

`fr-semantic-body-locators-1` rows contain a copyable `target`, category, kind, supported operation
and exact `from` scalar. Omitting `--locators-only` also returns the body when more structural context
is needed. Omitting one or both filters broadens discovery. The index covers scalar nodes reachable
through the published role vocabulary. Switch arms, variant arms, catch records, map pairs and
record literal fields still require a typed pointer delta or complete body.

The eleven operations set integer, float, string and Boolean literals, name expressions, field and
keyword names, binary and unary operators, template text and comments. Decimal strings are portable
unsigned spellings. This keeps scalar output valid across the supported writers; negative values use
the IR unary node. Identifier edits use the common ASCII identifier subset.

When the operation and both scalar values are known, add `--intent-to VALUE` to the filtered
semantic query. It resolves exactly one locator and returns `fr-semantic-edit-plan-1` with the full
generated intent, input and result body identities, and refinement evidence. When `--locators` is
omitted, this form also omits the semantic model and pattern rows. It therefore discovers the file,
handle, body basis and intent in one source-free request:

```sh
fr project semantic . --declaration calculate --body \
  --locator-op set-int --locator-from 1 --intent-to 7 --nodes 128 --minimal
```

Preview the same request directly against the returned handle, then repeat with the unchanged plan
basis and `--write`:

```sh
fr author edit-body-scalar '<HANDLE>' --operation set-int --from 1 --to 7

fr author edit-body-scalar '<HANDLE>' --operation set-int --from 1 --to 7 \
  --write --plan-basis '<PLAN_CONTEXT_BASIS>'
```

The author report retains the generated intent and compiled-change identity. Author batches,
project tasks and task changes use `edit-body-scalar` with a `scalar` object containing
`operation`, `from` and `to`. Use the explicit intent route when several ordered edits are needed or
when a scalar is ambiguous. `fr author plan-semantic-intent --body BODY` exposes the same planner
without scanning a project.

Progressive disclosure removes that remaining ambiguity without exposing source or asking the agent
to construct a locator. A revealed scalar includes an opaque `frde1:` capability and an exact
preview template:

```sh
fr author edit-body-disclosed '<HANDLE>' --edit 'frde1:<DIGEST>' --to 7
```

The capability binds the revision, declaration, body identity, scalar pointer, operation, current
value and typed role locator. The author recomputes all of them, then generates and checks the same
semantic intent used by this section. Author batches, project tasks and task changes carry
`disclosed: {edit,to}`. Large scalar values stay committed rather than repeated in capability and
author receipts. See [disclosure-bound editing](disclosed-editing.md).

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

`fr author apply-semantic-intent --body BODY --intent INTENT --canonical --compiled` interprets each
intent directly and compiles an ordered `fr-semantic-change-1`. It runs the checked delta engine and
requires equal canonical results. `fr author edit-body-intent HANDLE --from INTENT` carries the
result through the existing writer, byte-preserving body splice and history lifecycle. Author
batches, project tasks and reviewed task changes use `edit-body-intent`.

The Python SDK mirrors `Role`, `NodeCategory`, `LocatorStep`, `Intent`, `SemanticIntent`, the
three-field `ScalarRequest` and the two-field `DisclosedEditRequest` used by task manifests. Direct
JSON is the measured choice for a one-off scalar edit. Python provides earlier type and scalar checks
when several operations or reusable producer logic justify it. The deterministic four-route result
is documented in [semantic intent evaluation](semantic-intent-evaluation.md).

`FrKernels.SemanticIntent` anchors admission, locator bounds and the exact operation/category/kind
relation. It proves deterministic exact-singleton abstract locator resolution, scalar category,
kind and child locality, direct/compiler equivalence and ordered composition. Exhaustive executable
comparisons cover the finite admission and target relations. Runtime application also compares the
direct result with the compiled delta result on every accepted request. Serde, role traversal,
SHA-256, parsers, writers, filesystem behavior and Python remain tested or trusted boundaries.

The edit-plan extension additionally models exact-one candidate admission and proves that compiling
the uniquely planned scalar intent produces the same abstract result as direct interpretation.
Rust and Lean compare every Boolean admission state over selected candidate-count boundaries.
