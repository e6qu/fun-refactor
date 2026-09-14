# Progressive disclosure

`fr project disclose HANDLE` lets an agent inspect a declaration without loading its source or its
complete semantic IR. The first response contains identity, a combined commitment and two holes.
One hole commits to the source-free semantic model; the other commits to the exact declaration
source. Every hole carries the exact argument array that reveals it.

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
deletion omits it. See [disclosure-bound semantic editing](disclosed-editing.md#structural-capabilities).

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

SHA-256 collision resistance, the Rust SHA implementation, JSON serialization, parser correctness
and the source-free IR reader remain trusted. Lean proves the numeric response ceilings, admitted
offset boundary and complete-frontier replacement laws. Strict source anchors and 1,521 shared
Rust/Lean cases connect those models to the implementation. The scalar editing extension adds 144
admission cases. Structural editing adds 576 admission cases and expands the task-target comparison
to 1,980 cases. CLI tests cover exact actions, stale
identities, UTF-8 paging and exact source reconstruction. The retained
[evaluation](../tests/agent-eval/progressive-disclosure.json) uses a separate Python Merkle oracle
against a generated generic project; it is deterministic agent-style evidence, not a live-agent or
cryptographic-proof claim.
