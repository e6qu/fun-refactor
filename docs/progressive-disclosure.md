# Progressive disclosure

`fr project disclose HANDLE` lets an agent inspect a declaration without loading its source or its
complete semantic IR. The first response contains identity, a combined commitment and two holes.
One hole commits to the source-free semantic model; the other commits to the exact declaration
source. Every hole carries the exact argument array that reveals it.

Use `--view evidence --depth N` to commit a source-free project context instead. Its four stable
domains are `code_map`, `call_traces`, `impact` and `sources_and_sinks`. The code map carries the
selected declaration's ancestors and bounded descendants. Call traces carry both directions,
cycles, confidence and depth omissions. Impact retains reference kind and confidence. Sources and
sinks are local value origins and uses with explicit unresolved, confidence, function and depth
boundaries; they are not a whole-program security-taint claim. `evidence_catalog` always names all
four domains under the compact ceiling. Follow an available shortcut or reveal the tree root to
obtain exact actions for the other branches.

Use `--view project` with a full directory, file or declaration handle for the cross-stack model.
Its four stable domains are `technologies`, `applications`, `styles` and
`documents_and_diagrams`. A declaration selects its containing file. The initial
`project_catalog` names every domain and the bounded shortcuts reveal individual content-addressed
branches. The project view has no whole-source frontier; its facts retain narrow source handles for
cases where the structured evidence or an explicit gap is insufficient.

The initial `semantic_shortcuts` are a bounded abridged outline of named or kinded nodes in the
committed model. Each shortcut has its semantic address, high-level labels, opaque hole, exact
action and `editable_scalars` and `editable_ir` descendant counts. The counts let an agent skip
subtrees that cannot produce its requested scalar or structural capability. The semantic root remains available for complete ordered
discovery; the shortcut budget reports how to obtain additional nodes.

Semantic reveals expose one JSON level. Small scalar children are inline. Composite and large
children remain independently addressed holes, so an agent can follow only the body, statement or
expression relevant to its task. A response that cannot fit all children returns a continuation
bound to the same view, hole, profile, limit and next offset. Source remains a separate explicit
domain. Revealing it returns UTF-8-safe fragments and a next hole until the declaration is complete.

An authorable scalar carries a `fr-disclosed-edit-1` descriptor when revealed. Its exact preview
template needs only a replacement value. Equal values at different typed locations receive distinct
IDs, and the author command rebinds an ID to the current revision, handle, semantic body, scalar and
role locator. Large strings expose only a commitment with their serialized size. See
[disclosure-bound editing](disclosed-editing.md) for identity, refusal, composition and lifecycle
details.

Authorable typed nodes and statement-list positions carry `fr-disclosed-ir-edit-1` descriptors in
`ir_edits`. A descriptor exposes the operation, placement, accepted category, current commitment,
schema action and exact preview template. It withholds the internal path and index because the
opaque capability already commits to both. Replacement and insertion accept one typed IR node;
deletion omits it. Compact reveals omit these larger descriptors while retaining `editable_ir`
counts; request the expanded profile before following a structural shortcut. See
[disclosure-bound semantic editing](disclosed-editing.md#structural-capabilities).

The complete response is bounded by `--token-limit`. `used_upper_bound` counts the compact JSON's
UTF-8 bytes plus the trailing newline printed by `fr`. For byte-fallback tokenizers, token count
cannot exceed that byte count. The bound is intentionally conservative and does not claim exact
accounting for a particular model vocabulary. Compact mode accepts 1,024 through 4,096; expanded
mode accepts up to 16,384 and is explicit in the response and every continuation.

Inside `project batch`, `used_upper_bound` retains the corresponding standalone disclosure size.
The batch profile and `--report-bytes` separately bound its combined nested reports.

## Commitment format

The `fr-semantic-merkle-1` tree uses SHA-256 over compact UTF-8 JSON arrays. Object keys are sorted
lexicographically; array order is preserved. The tagged preimages are:

```text
[schema,"null"]
[schema,"bool",value]
[schema,"number",canonical_number_text]
[schema,"string",value]
[schema,"array",[child_digest,...]]
[schema,"object",[[key,child_digest],...]]
[schema,"source",[byte,...]]
```

The combined root hashes
`[schema,"view",revision,target,semantic_root,source_root]`. `view_basis` additionally binds the
disclosure schema, semantic basis, profile and token limit. Semantic hole IDs bind that view basis, their RFC 6901 pointer
and subtree digest. Source holes bind the view, source root and byte offset. Cursor hashes also bind
the profile and token limit. A verifier can recompute a scalar or subtree digest immediately; it can
recompute a parent after collecting all of that parent's child pages.

## Content-addressed objects

Every tree root, revealed node and child row also carries a binary `object_digest`. Equal JSON
subtrees have the same digest across pointers, declarations and project revisions. A client can use
that digest directly as an object-storage key and fetch only the missing branches. The digest's
domain is `fr-merkle-object-1`; proof envelopes use the separate `fr-merkle-inclusion-1` schema.
The Python SDK's `merkle_object_pack` splits a complete JSON value into deduplicated
`fr-merkle-object-1` records;
`restore_merkle_object(root, object_store.get)` follows child digests lazily, fetches each reachable
record at most once and verifies every reconstructed value. A caller can persist, share or evict
individual branches without rewriting the root object. Storage location, retention, authorization
and transport remain the embedding application's policy.

The same generic Python pack/restore API accepts a complete cross-stack project value. It has no
schema-specific conversion layer: the Python object tree must preserve the Rust JSON shape and root
digest exactly.

The binary object tree hashes scalar values directly. Array leaves bind their numeric position and
child digest. Object leaves bind their sorted position, key and child digest. Adjacent leaves hash
as tagged pairs; an unpaired final leaf is promoted. The container digest binds its kind, length and
binary root. This keeps inclusion paths logarithmic even for wide code maps or trace lists.

Normal agent responses return object roots and digests without proof paths. Add `--proofs` for a
cache audit, evaluator or protocol test. Each revealed node then carries an
`fr-merkle-inclusion-1` path to `commitment.object_root`. A proof that cannot fit the selected
response ceiling refuses; it is never silently dropped. The Python SDK verifies either a fetched
object value or an advertised object commitment against such a path.

SHA-256 collision resistance, the Rust SHA implementation, JSON serialization, object-store
durability and authorization, parser correctness and the source-free IR reader remain trusted. Lean
proves the numeric response ceilings, admitted
offset and proof-step boundaries, evidence depth policy and complete-frontier replacement laws.
Strict source anchors and 2,418 shared Rust/Lean cases connect those models to the implementation.
These proofs serve the implementation test suite; agents do not need to ingest them. The scalar
editing extension adds 144 admission cases. Structural editing adds 576 admission cases and expands
the task-target comparison to 1,980 cases. CLI tests cover exact actions, stale identities, UTF-8
paging and exact source reconstruction. The retained
[semantic evaluation](../tests/agent-eval/progressive-disclosure.json) and
[project-evidence evaluation](../tests/agent-eval/progressive-evidence.json) use separate Python
Merkle oracles against generated generic projects. The latter reconstructs the complete evidence
tree, verifies 134 optional paths, rejects tampering and stale actions, and reports its response
ceilings. Its 136-call exhaustive traversal is a protocol audit; an agent normally stops after the
task-relevant branches. These are deterministic implementation tests, not live-agent or
cryptographic-proof claims.
