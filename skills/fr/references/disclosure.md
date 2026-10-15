# Read one report section at a time

For a retry fix, inspect the selected function before asking for the whole project.
A handle selects code returned by a query. A hole names an unopened report section.
A continuation gives the exact command for its next page. Disclosure means opening those sections as needed.

```sh
fr project disclose '<HANDLE>' --token-limit 4096
fr project disclose '<HANDLE>' --view evidence --depth 3 --token-limit 4096
```

The evidence view contains `code_map`, `call_traces`, `impact` and `sources_and_sinks`.
Follow a returned shortcut or root action, then the child relevant to the question.
Use `fr intent --from INTENT.json` when the needed sections are already known; see [intents](intents.md).

## Follow returned commands

Run returned `reveal.arguments` unchanged. Do not invent a handle, cursor, profile or limit.
Finish continuations before treating a section as complete. A changed project requires a fresh handle.
Keep `view_basis` and `commitment.root` with findings derived from the report.
A Merkle digest fingerprints a report tree; it does not prove program behavior.

`semantic_shortcuts` can lead to structured code. `editable_scalars` counts editable simple values;
`editable_ir` counts editable statements or other structured nodes.
For a retry integer, select its returned edit ID rather than another equal integer.

```sh
fr author edit-body-disclosed '<HANDLE>' --edit '<EDIT_ID>' --to 7
```

Review the complete diff, then copy its plan basis into the write:

```sh
fr author edit-body-disclosed '<HANDLE>' --edit '<EDIT_ID>' --to 7 --write --plan-basis '<PLAN_CONTEXT_BASIS>'
```

Large values use `from_commitment`; finish their pages to obtain the edit reference.
Changed, unknown, malformed or no-op edits refuse before writing.

## Edit a statement or structured value

Use `--profile expanded --token-limit 16384` to obtain `ir_edits`; compact replies omit those descriptors.
IR is the structured code record. Its categories distinguish statements, expressions and types.
Read `accepts` and `schema_action` before writing a replacement node as JSON.

```sh
fr author edit-body-disclosed-ir '<HANDLE>' --edit '<IR_EDIT_ID>' --from ../node.json
fr author edit-body-disclosed-ir '<HANDLE>' --edit '<DELETE_EDIT_ID>'
```

Replacement needs the accepted category; insertion needs a statement; deletion supplies no new value.
Batch and task inputs use `disclosed_ir: {edit,value?}`; simple-value edits use `disclosed: {edit,to}`.
Obtain a new descriptor after source changes. See [semantic edits](semantic-change.md).

Read exact source when structure is insufficient. Follow `frontier` actions until that section completes.
Page `offset` counts bytes within the declaration; `location` describes the complete file.
Compact reports accept at most 4,096 token units; expanded reports accept at most 16,384.
Those conservative output bounds do not measure billed model tokens.
An unopened section cannot establish absence. Use `--proofs` for protocol verification when needed.
