# Run project checks

Inspect `.fr/checks.json` declarations before execution:

```sh
fr checks
fr checks --run unit --basis '<CHECK_BASIS>' --quiet-success --no-declarations --output-bytes 2048
```

Choose task-relevant names from the listing; `unit` is only the example. Use its configuration `basis`. Listing runs no project code. Execution runs declared argv in its project-relative directory, with inherited environment, no command sandbox, and a direct-child timeout.

Read `passed`, each selected result, and `not_run`. Coverage labels are project claims. `passed: null` means nothing ran. Quiet success omits stream text but keeps byte counts; failures retain bounded diagnostics. `--no-declarations` relies on the matching reviewed listing. Raise `--output-bytes` up to 65536 only for needed failure detail.

Checks do not lock or verify source snapshots; rerun them after relevant edits. If declarations are absent, use documented project commands. Syntax acceptance or an unrelated check does not prove behavior.
