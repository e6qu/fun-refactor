# Plan a structural change

Check the relevant operation in the installed binary. A supported language cell still permits input-specific refusals.
For a rename, `<TARGET>` is a unique name or the file and `position` from `project show`, such as `app.py:1:5`.
A rename does not accept a project handle.

```sh
fr --json capabilities --capability rename
fr --json rename '<TARGET>' welcome
fr --json rename '<TARGET>' welcome --save-plan
fr history show '<TX>'
fr history patch '<TX>' --check
fr history apply '<TX>'
fr history apply '<TX>' --write --no-diff
```

Inspect the diff and warnings before saving or applying. `<TX>` is the returned `transaction`, scoped to this workspace.
Saving leaves source unchanged; applying that ID checks its recorded file snapshots and project source digest.
A plain rerun with `--write` computes a new plan. Apply the saved ID when the reviewed edits must remain identical.
After a stale-plan refusal, inspect the changed basis and create a new plan; do not edit journal digests to force acceptance.

Check the applied report, then run the project's relevant compiler or tests.
Use [declared checks](checks.md) when the project supplies `.fr/checks.json`.
Syntax validation rejects new parser errors; it does not prove imports resolve, tests pass, or behavior stays equivalent.
Refresh handles only before another source query or edit needs one. Keep the transaction ID for [history](history.md) and [patch export](git.md).

For related operations, load [recipes](recipes.md). For implementation changes, load [authoring](author.md).
For whole-entry changes, use `fr file delete`, `fr file executable`, or `fr file symlink`; each previews or records through the same history workflow.

For a revision-bound Next.js or FastAPI route feature, use `fr migrate feature` after `fr project features`.
FastAPI dependency edits require an explicit owning `pyproject.toml` and one exact requirement string for every missing generated import.
Use repeated `--check NAME` options to bind project-owned test commands to the transaction.
Review the connected files, save the plan, apply its transaction and attach the required evidence before cutover.
