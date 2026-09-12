# Make a semantic intent change

Use this source-free route for an exact scalar edit inside a supported function or method.

When the declaration name, operation, current value and requested value are known, preview the edit
directly from the workspace path:

```text
fr author edit-body-scalar . --declaration NAME --operation set-int --from 1 --to 2
fr author edit-body-scalar . --declaration NAME --operation set-int --from 1 --to 2 --write --plan-basis PLAN_CONTEXT_BASIS
```

This resolves one declaration below `.`, selects one scalar and includes the generated intent in
the preview. It needs no project query or payload. Duplicate names and zero or multiple scalar
matches refuse. Pass the preview's `plan_context_basis` to the write command.

Use a standalone source-free plan query when review must happen before author preview:

```text
fr project semantic . --declaration NAME --body --locator-op set-int --locator-from 1 --intent-to 2 --nodes 128 --minimal
```

It returns `edit_plan.intent`, a revision-bound handle and the body and intent identities. Body and
patterns stay omitted unless requested.

For an ambiguous scalar or several ordered edits, query the locator index and build an explicit
manifest:

```text
fr project semantic PATH --declaration NAME --body --locators --locators-only --locator-op set-int --locator-from 1 --nodes 128 --minimal
```

An empty complete row set means no scalar matches both filters. Broaden only the uncertain filter.

Each row supplies `target`, operation and `from`. Copy them into `fr-semantic-intent-1`, add a
different `to`, and use the reported body basis:

```json
{
  "schema": "fr-semantic-intent-1",
  "base": "frsb1:<CANONICAL_BODY_SHA256>",
  "operations": [{"op":"set-int","target":[<COPIED_STEPS>],"from":"1","to":"2"}]
}
```

Inspect `fr author semantic-schema intent` for all roles, operations and scalar rules. Integers and
floats use unsigned decimal strings. The IR represents negatives with a unary node. Names, fields
and keywords are ASCII identifiers. Booleans are JSON values. Operations are ordered.

The Python mirror follows the same contracts. `ScalarRequest("set-int", "1", "2")` emits the task
manifest fields. `SemanticIntent` and `Intent.SetInt` construct the explicit form.

Use the SDK for reusable manifests, a pointer delta for node or statement-list changes, and a
complete body when most statements change.

With a body file, run `fr author apply-semantic-intent --body BODY --intent INTENT --canonical
--compiled`. It checks direct interpretation against the compiled `fr-semantic-change-1` result.
For a project change, preview `fr author edit-body-intent HANDLE --from INTENT`, review the complete
diff and writer fidelity, then use the normal task-change or saved-history lifecycle. Author batches,
project tasks and task changes use `edit-body-intent`. The shorter manifest operation is
`edit-body-scalar` with `scalar: {"operation":"set-int","from":"1","to":"2"}`.

A missing or ambiguous role, category or kind mismatch, stale `from`, invalid scalar, no-op, stale
base or invalid intermediate body refuses before source and history mutation. This is a structural
scalar edit. It does not rename bindings or prove program behavior.
