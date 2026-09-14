# Progressive disclosure

Start with the full handle returned by `project explore` or `project find`:

```sh
fr project disclose <HANDLE> --token-limit 4096
```

```sh
fr project disclose <HANDLE> --view evidence --depth 3 --token-limit 4096
```

This source-free view catalogs `code_map`, `call_traces`, `impact` and `sources_and_sinks`. Follow
an exact shortcut or root action. `object_digest` is a stable storage key; the Python SDK packs and
lazily restores objects. Use `--proofs` only for cache or protocol tests.

The first response contains semantic and source holes without either payload. Prefer a relevant
`semantic_shortcuts` action whose `editable_scalars` count is nonzero for a scalar change or whose
`editable_ir` count is nonzero for a typed-node or statement-list change. Run `reveal.arguments`
exactly; do not reconstruct the handle, cursor, profile or limit.
Each reveal exposes one IR level. Follow the child whose summary, key and semantic address match the
task. Run exact continuation arguments to finish a child page.

An editable scalar includes `edit` with an opaque ID and exact `preview_template.arguments`.
Replace only `<NEW_VALUE>` and run that array after `fr`:

```sh
fr author edit-body-disclosed <HANDLE> --edit <EDIT_ID> --to <NEW_VALUE>
fr author edit-body-disclosed <HANDLE> --edit <EDIT_ID> --to <NEW_VALUE> --write --plan-basis <PLAN_CONTEXT_BASIS>
```

Equal values can have different IDs; use the one at the intended semantic address. Inspect the
complete preview before the bound write. A large scalar appears as
`from_commitment`; finish its page chain to obtain the edit without repeating its value. Stale,
unknown, malformed and no-op capabilities refuse before mutation. The same request is
`{"edit":"frde1:...","to":"..."}` under a batch/task `disclosed` field.

Use `--profile expanded --token-limit 16384` before following `editable_ir`; compact reveals omit
structural descriptors so scalar inspection stays within its smaller envelope. An `ir_edits` entry
selects one structural operation without exposing its hidden path or index.
Use `accepts` to construct the smallest typed node with the Python SDK or the category named by its
`schema_action`, write that node as one JSON value, and run `preview_template.arguments` exactly:

```sh
fr author edit-body-disclosed-ir <HANDLE> --edit <IR_EDIT_ID> --from ../node.json
fr author edit-body-disclosed-ir <HANDLE> --edit <DELETE_EDIT_ID>
```

Replacement accepts the bound category; insert and append accept a statement; delete takes no
value. Batch and task manifests carry `disclosed_ir: {edit,value?}`. Reveal a fresh capability after
any source change.

Use the `exact-source` hole only when semantic structure cannot support the edit or review. Source
pages place their continuation hole in `frontier`; follow its exact action until the frontier is
empty. Every response states a conservative token upper bound based on serialized UTF-8 bytes.
`compact` accepts at most 4096. Use `--profile expanded --token-limit N` explicitly, up to 16384,
when the compact envelope or a single child cannot fit.

Keep `view_basis` and `commitment.root` with derived decisions. A changed project invalidates the
target and every hole. A changed profile, limit or cursor invalidates a continuation. Do not treat an
unrevealed hole as absence, and do not treat a Merkle digest as behavioral proof.

Exact actions may be copied into a profiled batch. Batch argument arrays accept the leading
`project` emitted by the action:

```json
{
  "schema": "fr-project-batch-1",
  "requests": [
    {"id": "names", "arguments": ["explore", "greet"]},
    {"id": "frontier", "arguments": ["disclose", {"request": "names", "pointer": "/rows/0/handle"}, "--token-limit", "4096"]},
    {"id": "root", "arguments": ["disclose", {"request": "names", "pointer": "/rows/0/handle"}, "--reveal", {"request": "frontier", "pointer": "/frontier/0/id"}, "--token-limit", "4096"]}
  ]
}
```
