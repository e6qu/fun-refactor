# Select and execute project checks

When `.fr/checks.json` exists, inspect its declared commands and coverage before running them:

```sh
fr checks
fr checks --run unit --basis '<CHECK_BASIS>' --output-bytes 2048
```

Choose names from the listing for the task. `unit` is the example's name; projects can declare different names.
Use the returned configuration `basis`, not a project revision or Git basis.
A changed configuration requires another inspection. Listing does not execute project code.
Execution runs the declared argv in its project-relative directory with the inherited environment and a direct-child timeout.
It has no command sandbox; commands can write files outside source history. Use the existing task authorization.

Read `passed`, every selected result and `not_run`. Coverage labels are project claims.
`passed: null` means no command ran. A nonzero exit or failed result means validation failed, even if parsing succeeded.
Output includes omitted-byte counts. Increase `--output-bytes` up to 65536 when the reported failure needs more detail.
Source snapshots are not locked or verified by this command. Relevant later edits require another check run.

If declarations are absent, use the project's documented compiler/test commands and report their scope.
Do not treat absent checks, syntax acceptance or a successful unrelated check as behavioral verification.
