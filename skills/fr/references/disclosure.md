# Progressive disclosure

Start with the full handle returned by `project explore` or `project find`:

```sh
fr project disclose <HANDLE> --token-limit 4096
```

The first response contains semantic and source holes without either payload. Prefer a relevant
`semantic_shortcuts` action whose `editable_scalars` count is nonzero when changing a scalar, then
the semantic root hole. Run `reveal.arguments` exactly; do not reconstruct the handle, cursor,
profile or limit.
Each reveal exposes one IR level. Small scalars are inline and composite children remain holes with
their own exact actions. Follow only the child whose `summary`, key and semantic address match the
task. If `continuation` is present, run its exact arguments to finish that child page.

An editable scalar includes `edit` with an opaque ID and exact `preview_template.arguments`.
Replace only `<NEW_VALUE>` and run that array after `fr`:

```sh
fr author edit-body-disclosed <HANDLE> --edit <EDIT_ID> --to <NEW_VALUE>
fr author edit-body-disclosed <HANDLE> --edit <EDIT_ID> --to <NEW_VALUE> --write --plan-basis <PLAN_CONTEXT_BASIS>
```

Equal values can have different IDs. Copy the ID associated with the intended semantic address.
Preview and inspect the complete diff before the bound write. A large scalar appears as
`from_commitment`; finish its page chain to obtain the edit without repeating its value. Stale,
unknown, malformed and no-op capabilities refuse before mutation. The same request is
`{"edit":"frde1:...","to":"..."}` under a batch/task `disclosed` field.

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
