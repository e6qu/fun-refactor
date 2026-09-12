# Make a semantic intent change

Use this route for an exact scalar edit inside one supported function or method. It avoids raw
source, serialization-only pointer segments and complete replacement nodes.

When the operation, current value and requested value are known, use the direct plan route first:

```text
fr project semantic . --declaration NAME --body --locator-op set-int --locator-from 1 --intent-to 2 --nodes 128 --minimal
```

This searches below the selected directory, refuses duplicate declaration names, selects exactly one
matching scalar and returns a complete `edit_plan.intent`. It omits the body and patterns when
`--locators` is absent. Retain `selection.handle`, `body_identity.basis`, the intent and identities.
Preview and write without constructing a payload:

```text
fr author edit-body-scalar HANDLE --operation set-int --from 1 --to 2
fr author edit-body-scalar HANDLE --operation set-int --from 1 --to 2 --write --plan-basis PLAN_CONTEXT_BASIS
```

For an ambiguous scalar or several ordered edits, query the locator index and build an explicit
manifest:

```text
fr project semantic PATH --declaration NAME --body --locators --locators-only --locator-op set-int --locator-from 1 --nodes 128 --minimal
```

An empty complete row set means no supported scalar matches both filters. Broaden only the uncertain
filter. Without `--locators-only`, the report also returns the semantic body for structural context.

Each row supplies `target`, operation and `from`. Copy them into `fr-semantic-intent-1`, add a
different `to`, and use the reported body basis:

```json
{
  "schema": "fr-semantic-intent-1",
  "base": "frsb1:<CANONICAL_BODY_SHA256>",
  "operations": [{"op":"set-int","target":[<COPIED_STEPS>],"from":"1","to":"2"}]
}
```

Inspect `fr author semantic-schema intent` for all roles, operations and scalar rules. Integer and
float values use portable unsigned decimal strings. Negative values are represented by the IR's
unary node. Name, field and keyword values are ASCII identifiers. Boolean values are JSON Booleans. Operations are
ordered and each locator resolves against the previous operation's result.

The Python mirror follows the same contracts. `ScalarRequest("set-int", "1", "2")` emits the task
manifest fields. `SemanticIntent` and `Intent.SetInt` construct the explicit form.

Direct scalar authoring is the default for one unique edit. Use the SDK for reusable manifests, a
pointer delta for node or statement-list changes, and a complete body when most statements change.

With a body file, run `fr author apply-semantic-intent --body BODY --intent INTENT --canonical
--compiled`. It checks direct interpretation against the compiled `fr-semantic-change-1` result.
For a project change, preview `fr author edit-body-intent HANDLE --from INTENT`, review the complete
diff and writer fidelity, then use the normal task-change or saved-history lifecycle. Author batches,
project tasks and task changes use `edit-body-intent`. The shorter manifest operation is
`edit-body-scalar` with `scalar: {"operation":"set-int","from":"1","to":"2"}`.

A missing or ambiguous role, category or kind mismatch, stale `from`, invalid scalar, no-op, stale
base or invalid intermediate body refuses before source and history mutation. This is a structural
scalar edit. It does not rename bindings or prove program behavior.
