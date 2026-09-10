# Run project checks

Inspect `.fr/checks.json` declarations before execution:

```sh
fr checks
fr checks --run unit --basis '<CHECK_BASIS>' --quiet-success --no-declarations --output-bytes 2048
fr checks --run unit --basis '<CHECK_BASIS>' --record-for '<TX>' --quiet-success
```

Choose task-relevant names from the listing; `unit` is only the example. Use its configuration `basis`. The full digest or a prefix of at least 32 hex characters is valid. Listing runs no project code. Execution runs declared argv in its project-relative directory, with inherited environment, no command sandbox, and a direct-child timeout.

Read `passed`, each selected result, `source_snapshot_stable`, `configuration_stable`, and `not_run`. Coverage labels are project claims. `passed: null` means nothing ran. Quiet success omits stream text but keeps byte counts; failures retain bounded diagnostics. `--no-declarations` relies on the matching reviewed listing. Raise `--output-bytes` up to 65536 only for needed failure detail.

Execution compares the supported-source revision before the first check and after every selected check. Source or declaration drift fails the report. `--record-for` preflights one applied transaction, then attaches a passing `frce1:` receipt to its durable journal after rechecking its affected files and source revision. The receipt binds the configuration basis, source revision and selected names. It remains historical after undo or later source changes.

Checks do not lock sources while project code runs. Commands can mutate and restore a file between boundary snapshots, and unsupported files are outside the revision. If declarations are absent, use documented project commands. Syntax acceptance or an unrelated check does not prove behavior.
