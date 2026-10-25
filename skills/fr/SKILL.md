---
name: fr
description: Use fr to refactor code, check targeted edits, or write and check Lean proofs for supported code.
---
# Work with fr

Work at the project root, or pass `-C PATH`. Use this route for a small code change.
A handle selects code at one source revision. Copy it unchanged from a query result.

```sh
fr --json project find '<NAME>' --signature --source --bytes 4096
fr checks
```

Read the source, gaps and omissions. Narrow an ambiguous name with `--in FILE`.
Follow `next_offset` for clipped source. For insertion, query with `--in DESTINATION_FILE` and use that result's `root` file handle.
The check listing names project commands and supplies their `basis`; listing does not run them.

Choose the operation that matches the request. A function body holds its executable statements.
A parameter is an input in its declaration; an argument is a value supplied by a caller.

| Change | Operation | Contents of an extensionless fragment file |
| --- | --- | --- |
| Replace a function body | `replace-body` | The new body; include braces for Rust or TypeScript, omit the function's outer indentation for Python statements |
| Rename a function | `refactor` | `{"operation":"rename","name":"<NEW_NAME>","documentation":"preserve"}` |
| Remove an unused Rust parameter | `refactor` | `{"operation":"remove-parameter","index":0}`; positions start at zero |
| Add a Rust function | `insert-declaration` | One complete function, including its documentation comment; target the destination file or container |

This rename route selects Rust, Python and TypeScript functions outside classes or type implementation blocks.
It updates resolved uses and preserves explicit import aliases.
Parameter removal updates declared direct callers. The discarded argument must be a literal, such as `false` or `0`.
Expressions left in place retain their evaluation order. Refuse an effectful discarded argument rather than silently dropping its work.
External consumers remain unknown. Read warnings and review every affected file before applying.

Store fragments and manifests in `.fr/`, outside recognized source. A manifest groups disjoint edits from one revision:
```json
{"operations":[{"op":"<OPERATION>","handle":"<HANDLE>","from":".fr/fragment"}]}
```
Add another operation for each necessary body or declaration. Replace placeholders with actual values; inspect the complete diff.
```sh
fr --json author batch --from .fr/change
fr --json author batch --from .fr/change --save-plan --plan-basis '<PLAN_CONTEXT_BASIS>'
```
The first command previews. Saving records one transaction, a group of proposed edits, without changing source.
Retain its `transaction` number and `transaction_context_basis`.

Use the saved transaction with the check listing's basis and explicit check names:
```json
{
  "schema":1,
  "transaction":7,
  "transaction-context-basis":"<TRANSACTION_CONTEXT_BASIS>",
  "checks":{"basis":"<CHECK_BASIS>","names":["<CHECK_NAME>"]},
  "exercise-reversal":false,
  "patch":{"output":"change.patch"},
  "check-output-bytes":2048
}
```
Replace `7` with your transaction number. Save as `.fr/workflow`, then review and execute:
```sh
fr --json workflow --from .fr/workflow
fr --json workflow --from .fr/workflow --write --basis '<WORKFLOW_BASIS>'
```
Require `passed: true`. This applies the edit, checks it and exports its patch.
Set `exercise-reversal` to `true` when the same checks also accept the original code; it then checks undo and redo too.
For a repair or new feature, keep it `false`: the new behavior test may correctly fail after undo restores the old code.
Use `fr history undo '<TX>'` and `fr history redo '<TX>'` to preview reversals; add `--write` only after reviewing each preview.
A failed check can leave source edited: retain its diagnostics and inspect `transaction_status`.
A competing source edit invalidates the old review; prepare a fresh preview.
When no checks exist, run explicit compiler or behavior checks and follow [manual delivery](references/git.md).

For detailed authoring rules, read [Author](references/author.md); for delivery and recovery, read [Workflow](references/workflow.md).
Use [Explore](references/explore.md) for unfamiliar code and the [Lean proof loop](references/lean.md) for a fixed property.
The [first-change lesson](references/first-change.md) introduces these steps with a complete greeting example.
