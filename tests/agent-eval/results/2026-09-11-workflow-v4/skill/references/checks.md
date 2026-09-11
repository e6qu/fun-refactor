# Run declared checks

Review the listing, then run task-relevant names with its basis:
```sh
fr checks
fr checks --run unit --basis '<CHECK_BASIS>' --quiet-success --no-declarations --output-bytes 2048
fr checks --run unit --basis '<CHECK_BASIS>' --record-for '<TX>' --quiet-success
```
`unit` is an example. Listing executes nothing. Runs use declared argv, directory, inherited environment, and timeout. A full basis or 32-character prefix works.

Read `passed`, results, `not_run`, `source_snapshot_stable`, and `configuration_stable`; null means nothing ran. Quiet success retains stream byte counts, while failures keep bounded diagnostics. `--no-declarations` requires the retained matching listing.

Source or configuration drift fails. `--record-for` preflights an applied transaction and attaches a source-bound receipt; required checks bind exact configuration and ordered names. Commands run without a file lock. Unsupported files, syntax acceptance, coverage labels, or unrelated checks do not establish behavior.
