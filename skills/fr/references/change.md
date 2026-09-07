# Plan a structural change

Check the relevant operation in the installed binary. A supported language cell still permits input-specific refusals.
For a rename, `<TARGET>` is a unique name or the file and `position` from `project show`, such as `app.py:1:5`.
A project handle is not a mutation target.

```sh
fr --json capabilities --capability rename
fr --json rename '<TARGET>' welcome
fr --json rename '<TARGET>' welcome --save-plan
fr history show '<TX>'
fr history patch '<TX>' --check
fr history apply '<TX>'
fr history apply '<TX>' --write
```

Inspect the diff and warnings before saving or applying. `<TX>` is the returned `transaction`, scoped to this workspace.
Saving leaves source unchanged; applying that ID checks its recorded file snapshots and project source digest.
A plain rerun with `--write` computes a new plan. Apply the saved ID when the reviewed edits must remain identical.
After a stale-plan refusal, inspect the changed basis and create a new plan; do not edit journal digests to force acceptance.

Check the applied report, then run the project's relevant compiler or tests.
Syntax validation rejects new parser errors; it does not prove imports resolve, tests pass, or behavior stays equivalent.
Refresh project handles after a write. Keep the transaction ID for [history](history.md) and [patch export](git.md).

## Several related operations

Load the recipe vocabulary only when authoring a recipe:

```sh
fr --json recipe --vocabulary
fr --json recipe rename.recipe --explain
fr --json recipe rename.recipe
```

For the two-file greeting example, `rename.recipe` contains:

```recipe
schema 1
recipe rename-greeting {
  requires symbol "greet" where kind=function in="app.py"
  rename to "welcome" where kind=function name="greet" in="app.py"
  expect matched = 1
  expect changed = 2 files
  expect refusals = 0
}
```

This is an alternative to the single rename above; preview it before that rename changes the source.
Choose expectations from the task's actual scope, rather than copying the example's file count.
Save a recipe with `--save-plan` and apply its returned transaction through history.
All steps see the preceding virtual result, and failed expectations prevent source writes.
A recipe composes existing operations; it cannot authorize an unsupported transformation.

Bounded arbitrary declaration/body authoring remains roadmap work.
If no operation expresses the requested change, use the normal editor on the needed source and run appropriate checks.
Those editor writes do not automatically join an `fr` transaction. Translation produces a draft for supported constructs; inspect unsupported cases and validate the target project.
