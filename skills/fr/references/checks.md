# Run declared checks

Review `.fr/checks.json` through the listing, then run task-relevant names with its basis:

```sh
fr checks
fr checks --run unit --basis '<CHECK_BASIS>' --quiet-success --no-declarations --output-bytes 2048
fr checks --run unit --basis '<CHECK_BASIS>' --record-for '<TX>' --quiet-success
```

`unit` is an example name. Listing executes nothing. A run executes declared argv in its declared directory with inherited environment and a direct-child timeout. The full basis or a prefix of at least 32 hex characters is accepted.

Read `passed`, results, `not_run`, `source_snapshot_stable`, and `configuration_stable`. `passed: null` means nothing ran. Quiet success removes streams but retains byte counts; failures keep bounded diagnostics. `--no-declarations` relies on the retained matching listing. Increase `--output-bytes` only for needed failure detail.

Source or check-configuration drift fails the run. `--record-for` preflights an applied transaction and attaches a passing source-bound receipt. Required checks must match their recorded basis and ordered names exactly.

Checks do not lock files while commands run, and unsupported files lie outside the source revision. Syntax acceptance, coverage labels, and unrelated checks do not establish behavioral correctness.
