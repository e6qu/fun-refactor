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

Run `fr project task --from '<TASK_MANIFEST>'`. Read every nested query’s gaps and omissions. A
target’s `target-supported` eligibility only proves that its language and declaration kind can
enter that authoring route. It does not validate fragment bytes or the exact syntax container.

Write each required fragment outside recognized source. Replace every `<FRAGMENT:ID>` in
`author_manifest_template` with the actual path and save the result. Follow `next`: preview the
author batch, save it under the complete `plan_context_basis`, replace the
transaction placeholders in `workflow_manifest_template`, preview the workflow, and write it under
the complete `workflow_basis`.

Do not pass either template directly to its command. Preserve full bases until their reviewed
write. If no `.fr/checks.json` exists, omit `checks` and `delivery`; use the manual author/history
route and independent validation. A task reference to an omitted query report refuses, so increase
`--report-bytes` or narrow that query instead of guessing its handle.

When concrete fragments and declared checks exist, use `task-change` to remove the template joins.
Keep the same requests and targets, add each fragment path as target `from`, add exact author
`postconditions`, and use schema `fr-task-change-1`. Put `check-output-bytes` inside `delivery`.

Preview with `fr task-change --from '<TASK_CHANGE_MANIFEST>'`. Retain the complete diff and
`task_change_basis`. Execute with `fr task-change --from '<TASK_CHANGE_MANIFEST>' --write --basis
'<TASK_CHANGE_BASIS>'`. The second call recomputes all inputs, records one check-bound transaction,
and runs the requested reversal and patch lifecycle. Changed source, fragments, checks or delivery
choices refuse before persistence. A failed stage withholds the patch and reports its current state.
