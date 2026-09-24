# Run declared checks

Review the listing, then run relevant names with its basis:
```sh
fr checks
fr checks --run unit --basis '<CHECK_BASIS>' --quiet-success --no-declarations --output-bytes 2048
fr checks --run unit --basis '<CHECK_BASIS>' --record-for '<TX>' --quiet-success
```
`unit` is an example. Runs use declared argv, cwd, inherited environment and timeout. Use a full basis or 32+ hex prefix.

Read `passed`, `results`, `not_run`, `source_snapshot_stable` and `configuration_stable`. Null means no execution. Quiet success keeps stream byte counts; failures retain bounded diagnostics. `--no-declarations` needs a matching listing.

Source or configuration drift fails. `--record-for` preflights an applied transaction and records source-bound evidence. Required checks bind configuration and ordered names. Commands run without a file lock. Unrelated checks, coverage labels and syntax acceptance do not establish behavior.

For compiler diagnostics and identities, read [Compiler](compiler.md).
