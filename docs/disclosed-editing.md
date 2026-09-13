# Disclosure-bound semantic editing

`project disclose` attaches an opaque `frde1:` capability to each authorable scalar when that
scalar is revealed. The capability lets an agent select one repeated value without reading source,
guessing an RFC 6901 path or constructing a role locator.

Run the returned preview template after replacing only `<NEW_VALUE>`:

```sh
fr author edit-body-disclosed '<FULL_HANDLE>' --edit 'frde1:<DIGEST>' --to 7
```

Preview is read-only. Review the complete diff and `disclosed_edit` receipt, then use the normal
`--write --plan-basis '<PLAN_CONTEXT_BASIS>'` transition. The resulting transaction supports
checked apply, undo, redo and forward or reverse Git patch export.

Capabilities are available for source-free semantic bodies that the Rust, Go, Java,
TypeScript or TSX body writer can render. The eleven scalar operations are the same operations as
semantic intents: integer, float, string and Boolean literals; name, field and keyword names;
binary and unary operators; template text; and comments. An unsupported reader or writer emits no
edit capability.

`semantic_shortcuts[].editable_scalars` counts capabilities below each abridged node. Reveal a
relevant shortcut with a nonzero count. A small scalar appears as `from` beside its capability. A
large string remains paged and appears only as a tagged Merkle `from_commitment`; the capability is
attached after the last page. Large old or replacement values remain commitments in the author
receipt as well.

## Identity and refusal

The lowercase SHA-256 part of an edit ID hashes this compact JSON tuple:

```text
["fr-disclosed-edit-1",revision,full_handle,body_basis,
 scalar_pointer,operation,current_scalar,typed_role_locator]
```

The current scalar and its typed locator are both bound. Two equal literals at different positions
therefore receive different IDs. The author command recomputes every candidate from the current
typed body and accepts only one matching capability whose current value still agrees and whose
replacement differs. Malformed, unknown, ambiguous, stale and no-op requests refuse before history
or source mutation. A changed project also invalidates the full declaration handle.

For large values, `from_commitment` and `to_commitment` use the same tagged canonical JSON tree
format as progressive disclosure. `serialized_bytes` measures compact JSON bytes. The commitment
does not reveal the value and does not prove program behavior.

## Composed manifests

Author batches, project tasks and reviewed task changes use the same payload:

```json
{
  "op": "edit-body-disclosed",
  "handle": "frp1:<FULL_HANDLE>",
  "disclosed": {"edit": "frde1:<DIGEST>", "to": "7"}
}
```

Batch operations remain atomic and disjoint. A project task carries this object into its generated
author template. A task change resolves it through the same semantic intent compiler, declared
checks, reversal stages and patch delivery as other authoring operations. The Python SDK's
`DisclosedEditRequest` emits the two-field object and validates its wire shape.

## Verification evidence

`FrKernels.DisclosedEdit` proves that admission implies a full handle, a well-formed capability,
exactly one candidate, a matching current value and a real change. It proves stale-current,
unchanged and malformed-capability refusal. `FrKernels.Project` proves that disclosed editing has
the same finite language and declaration-target policy as body replacement. Rust and Lean agree on
144 admission cases and the complete 1,782-case task-target matrix.

The retained [deterministic evaluation](../tests/agent-eval/disclosed-edit.json) independently
recomputes two identities for equal integer literals, confirms the old scalar route refuses their
ambiguity, makes one exact source-free edit, compiles and runs the Rust fixture, checks stale
refusal, validates forward and reverse patches, and exercises undo and redo. It uses three bounded
disclosure responses totaling 13,577 bytes; the largest is 8,068 bytes under an explicit 16,384-byte
expanded limit. The preview is 3,968 bytes.

These proofs and fixtures do not prove SHA-256 collision resistance, JSON serialization, semantic
parsing, writer correctness, compiler behavior or filesystem atomicity. Those components remain
trusted or covered by integration and behavioral tests.
