# Prepare one structural task

Use one `project task` call when an edit needs several project views, exact targets and declared
delivery checks. The manifest runs ordinary read-only project queries on one revision. Later query
arguments can reference earlier string results as described in [Batch](batch.md). Each target then
names a full handle directly or references a returned query report.

```json
{
  "schema": "fr-project-task-1",
  "requests": [
    {"id": "target", "arguments": ["find", "render", "--signature", "--source", "--bytes", "2048"]},
    {"id": "callers", "arguments": ["calls", {"request": "target", "pointer": "/rows/0/0"}, "--direction", "incoming"]}
  ],
  "targets": [
    {"id": "render-body", "handle": {"request": "target", "pointer": "/rows/0/0"}, "op": "replace-body"}
  ],
  "checks": ["unit"],
  "delivery": {"exercise-reversal": true, "patch": "artifacts/change.patch"}
}
```

Run `fr project task --from '<TASK_MANIFEST>'`. Read every query’s gaps and omissions.
`target-supported` covers the language and declaration kind, not fragment syntax.

`edit-body-scalar` carries `"scalar":{"operation":"set-int","from":"1","to":"2"}` and no
fragment. Preview validates its unique role match and generated intent.

`edit-body-disclosed` carries `"disclosed":{"edit":"frde1:<DIGEST>","to":"7"}` and the full
handle returned with that capability. It needs no fragment. Task change rebinds it to the current
typed body and refuses stale input.

`edit-body-disclosed-ir` carries
`"disclosed_ir":{"edit":"frdi1:<DIGEST>","value":<TYPED_IR_NODE>}`. Omit `value` only for
delete. The ID fixes category or statement position; do not add a path or index. Python
`DisclosedIrEditRequest` builds it from typed IR.

For the multi-command route, write fragments outside recognized source and replace each
`<FRAGMENT:ID>` in `author_manifest_template`. Follow `next`: preview and save the author batch
under `plan_context_basis`, fill the workflow transaction placeholders, then preview and write it
under `workflow_basis`.

Do not pass an unfilled template. Preserve full bases until write. Without `.fr/checks.json`, omit
`checks` and `delivery` and validate independently. If a referenced report was omitted, increase
`--report-bytes` or narrow the query.

When concrete fragments and declared checks exist, use `task-change` to remove the template joins.
Targets carry either a file path in `from` or complete UTF-8 text in `fragment`. With full retained
handles, set `requests` to `[]`; stale handles refuse.
Add exact author `postconditions` and use schema `fr-task-change-1`:

```json
{
  "schema": "fr-task-change-1",
  "requests": [],
  "targets": [{"id": "render-body", "handle": "frp1:<REVISION>:<ID>",
               "op": "replace-body", "fragment": "{ value.to_uppercase() }"}],
  "postconditions": {"files-changed": 1, "edits": 1, "changed-operations": 1,
                     "paths-changed": ["src/lib.rs"]},
  "checks": ["unit"],
  "delivery": {"check-original": true, "compact-success": true,
               "exercise-reversal": true, "patch": "artifacts/change.patch",
               "check-output-bytes": 2048}
}
```

Preview with `fr task-change --from '<TASK_CHANGE_MANIFEST>'`. Retain the complete diff and
`task_change_basis`. Execute with `fr task-change --from '<TASK_CHANGE_MANIFEST>' --write --basis
'<TASK_CHANGE_BASIS>'`. The write recomputes all inputs, records one check-bound transaction,
checks the original, and runs the reversal and patch lifecycle. Compact success keeps outcomes,
source stability and its receipt; failures keep bounded diagnostics. Changed inputs refuse before
mutation. A failed stage withholds the patch and reports its current state.

When Python is available, use [Runtime](runtime.md) to retain reports and the reviewed basis as local
objects instead of copying complete JSON through context.
