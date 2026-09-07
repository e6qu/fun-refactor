# Declared project checks

`fr checks` lists commands from `.fr/checks.json` without executing them.
Each declaration names the command, working directory, timeout and intended coverage.
This makes check selection explicit for an agent without inferring commands from filenames or package scripts.

```json
{
  "schema": 1,
  "checks": [
    {
      "name": "unit",
      "argv": ["cargo", "test", "--offline", "--lib"],
      "cwd": ".",
      "timeout_seconds": 120,
      "covers": ["Rust library unit tests"]
    }
  ]
}
```

Inspect the returned `basis` and declarations, then select check names:

```sh
fr checks
fr checks --run unit --basis <BASIS> --output-bytes 4096
```

`--run unit,integration` selects several names. Execution follows declaration order and continues after a failed check.
Missing or stale configuration digests, unknown names and duplicate selections refuse before execution.
The digest identifies configuration bytes, including whitespace. It does not identify the source tree or executable contents.
Run the selected checks after applying the reviewed change. Rerun them after any subsequent relevant source change.

The `fr-checks-1` JSON report includes each command's status, exit code, timing and bounded stdout/stderr.
It lists checks that did not run. A listing has `passed: null`; execution passes only when every selected command passes.
Failed commands, spawn failures, timeouts and excessive captured output produce a failing report and exit status 1.
Malformed configuration and selection errors produce the ordinary CLI error object with a nonzero exit status.
Each failure emits one JSON document.

Coverage descriptions come from project declarations. They do not establish test coverage, behavior preservation or formal verification.
The report states `source_snapshot_checked: false`. Concurrent source changes can invalidate the evidence.

## Execution boundaries

Running checks executes project code with the user's inherited environment and permissions.
The digest review is a stale-configuration guard, not a command sandbox or a source transaction.
Commands may write files; these writes do not enter `fr` history. Use commands appropriate to the authorized task.
Arguments pass directly to the executable without shell expansion. An explicitly declared shell still interprets its own arguments.
Standard input is closed. Executable lookup uses the inherited PATH; relative executable paths use the declared working directory.

Working directories must exist inside the canonical project root and cannot traverse symlinks.
Configuration paths cannot traverse symlinks either. Concurrent filesystem replacement remains outside these checks.
Timeouts terminate and reap the direct child. Descendants can outlive it, including on interruption of `fr` itself.
Use an external process supervisor when a project needs descendant cleanup or stronger resource isolation.

Output capture uses temporary files to avoid pipe deadlocks and unbounded in-memory buffering.
The runner polls every 20 ms and stops a running child above 16 MiB on either stream.
This is a soft disk limit; a fast writer can exceed it between polls.
The output budget retains up to 65,536 raw bytes per stream, with exact omitted-byte counts for the observed capture.
UTF-8 replacement decoding and JSON escaping can expand the displayed text beyond that raw-byte budget.
Descendant writers can continue changing a capture after the direct child exits.

Configuration accepts at most 65,536 bytes and 32 checks, with timeouts from 1 through 3,600 seconds.
Unknown fields refuse. Names are unique ASCII letters, digits, underscores or hyphens, at most 64 bytes.
Each command has at most 128 arguments of 4,096 bytes each and 32 coverage descriptions of 512 bytes each.

CLI regressions cover preview, stale configuration, selection, failure reports, output truncation, timeouts, literal arguments and path refusals.
The execution layer has test evidence; no Lean proof covers process behavior or project-check semantics.
