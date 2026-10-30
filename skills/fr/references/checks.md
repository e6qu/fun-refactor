# Run declared checks

A check runs a project command. `.fr/checks.json` stores settings; save reports elsewhere.
For a rounding fix, review the rounding check. Its basis identifies that listing; `<TX>` identifies a saved edit.

Review the listing, then run relevant names with its basis:
```sh
fr checks
fr checks --run unit --basis '<CHECK_BASIS>' --quiet-success --no-declarations --output-bytes 2048
fr checks --run unit --basis '<CHECK_BASIS>' --record-for '<TX>' --quiet-success
```
`unit` is an example. Runs use declared argv, cwd, inherited environment and timeout. Use a full basis or 32+ hex prefix.

Read `passed`, `results`, `not_run`, `source_snapshot_stable` and `configuration_stable`. Null means no execution. Quiet success keeps stream byte counts; failures retain bounded diagnostics. `--no-declarations` needs a matching listing.

Changed source or settings fail the check. `--record-for` checks an applied edit and records its source inputs.
Required checks bind settings and ordered names. Commands run without a file lock.
Parsing and unrelated checks do not establish the requested behavior.

[Compiler diagnostics and identities](compiler.md).
