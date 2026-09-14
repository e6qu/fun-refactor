# Disclosure-bound semantic editing

`project disclose` attaches opaque capabilities to authorable parts of a source-free typed body.
An agent can select a repeated scalar, complete node or statement-list position without reading
source, guessing an RFC 6901 path or constructing a role locator.

## Scalar capabilities

An authorable scalar carries a `frde1:` descriptor when revealed. Run its preview template after
replacing only `<NEW_VALUE>`:

```sh
fr author edit-body-disclosed '<FULL_HANDLE>' --edit 'frde1:<DIGEST>' --to 7
```

The eleven scalar operations are the same operations as semantic intents: integer, float, string
and Boolean literals; name, field and keyword names; binary and unary operators; template text;
and comments. `semantic_shortcuts[].editable_scalars` counts capabilities below each abridged node.
A small scalar appears as `from`; large values remain tagged Merkle commitments.

## Structural capabilities

Authorable typed nodes and statement-list positions carry `ir_edits`. Each descriptor names one
operation and placement, the accepted IR category, a commitment to the current node or list, the
relevant schema action and exact preview arguments. It omits the internal path and index. The opaque
`frdi1:` ID already binds those details, so an agent only chooses a disclosed capability and supplies
the smallest replacement node when required.

```sh
# Same-category node replacement or statement insertion:
fr author edit-body-disclosed-ir '<FULL_HANDLE>' \
  --edit 'frdi1:<DIGEST>' --from node.json

# Statement deletion:
fr author edit-body-disclosed-ir '<FULL_HANDLE>' --edit 'frdi1:<DIGEST>'
```

`replace` accepts one source-free node in the descriptor's `accepts` category. `insert-statement`
accepts one statement at its bound `before` or `append` position, including an empty list.
`delete-statement` accepts no value. The existing semantic-change validator remains the execution
authority for node categories, source freedom, pointer and index bounds, result size and strict IR
shape. `semantic_shortcuts[].editable_ir` lets an agent skip irrelevant subtrees.
Structural descriptors are returned only with the explicit expanded profile; compact responses
retain their counts without letting the larger descriptor arrays crowd out scalar inspection.

Capabilities are available for bodies that the Rust, Go, Java, Python, TypeScript or TSX writer can render.
An unsupported reader or writer emits no capability. Preview is read-only. Review the complete diff
and receipt, then use the normal `--write --plan-basis '<PLAN_CONTEXT_BASIS>'` transition. The
resulting transaction supports checked apply, undo, redo and forward or reverse Git patch export.

## Identity and refusal

A scalar ID hashes this compact JSON tuple:

```text
["fr-disclosed-edit-1",revision,full_handle,body_basis,
 scalar_pointer,operation,current_scalar,typed_role_locator]
```

A structural ID hashes:

```text
["fr-disclosed-ir-edit-1",revision,full_handle,body_basis,
 address_pointer,path,index,category,operation,placement,current_merkle]
```

The structural descriptor discloses `current_merkle` while keeping the address, semantic-change path
and index inside the identity. Authoring reconstructs every candidate from the current typed body and
accepts exactly one matching identity with the same current node or statement list. It also requires
the operation's value shape, the bound node category and a real replacement. Source, declaration,
body, node/list, operation, category or position drift therefore refuses before history creation.

For large values, descriptors and receipts use the same tagged canonical JSON tree as progressive
disclosure. `serialized_bytes` measures compact JSON bytes. A commitment does not reveal its value
or prove program behavior.

## Composed manifests and Python

Author batches, project tasks and reviewed task changes carry scalar requests unchanged:

```json
{
  "op": "edit-body-disclosed",
  "handle": "frp1:<FULL_HANDLE>",
  "disclosed": {"edit": "frde1:<DIGEST>", "to": "7"}
}
```

Structural requests use the adjacent shape:

```json
{
  "op": "edit-body-disclosed-ir",
  "handle": "frp1:<FULL_HANDLE>",
  "disclosed_ir": {
    "edit": "frdi1:<DIGEST>",
    "value": {"kind": "int", "value": "7"}
  }
}
```

Omit `value` for deletion. `DisclosedEditRequest` and `DisclosedIrEditRequest` in the zero-dependency
Python SDK emit these shapes. The structural request accepts the same typed `Type`, `Stmt`, `Expr`
and `TemplatePart` nodes as complete bodies and semantic changes, catching raw dictionaries and
other non-IR values before serialization. Batch operations remain atomic and disjoint. Task changes
rebind capabilities to the current body and run declared checks, reversal stages and patch delivery.

## Verification evidence

`FrKernels.DisclosedEdit` proves the scalar admission requirements across 144 shared Rust/Lean
cases. `FrKernels.DisclosedIrEdit` proves exact-candidate, fresh-current, request-shape,
same-category-value and changed-result requirements across 576 cases. The existing semantic-change
kernel proves insertion/deletion position bounds and statement-count laws. `FrKernels.Project`
proves both capability routes share body replacement's supported language and target policy across
the complete 1,980-case matrix.

The retained [scalar evaluation](../tests/agent-eval/disclosed-edit.json) independently recomputes
two identities for equal literals, exercises one exact edit and checks behavior, stale refusal,
patches, undo and redo.

The retained [structural evaluation](../tests/agent-eval/disclosed-ir-edit.json) independently
recomputes all four capability identities and current Merkle roots on a generated generic fixture.
Each capability preview equals the corresponding explicit semantic-change preview. The evaluator
compiles behavior, rejects stale reuse, checks forward and reverse patches, exercises undo and redo,
and confirms same-shaped empty lists at different positions receive distinct IDs. Every disclosure
response stays below its explicit 16,384-byte ceiling.

These proofs and fixtures do not prove SHA-256 collision resistance, JSON serialization, semantic
parsing, writer correctness, compiler behavior or filesystem atomicity. Those components remain
trusted or covered by integration and behavioral tests. The evaluations are deterministic evidence;
they do not claim live-agent quality or exact model-token accounting.
