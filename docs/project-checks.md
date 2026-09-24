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
Supply the full configuration digest or at least 32 leading hex characters. Missing, short or stale digests, unknown names and duplicate selections refuse before execution.
The digest identifies configuration bytes, including whitespace. It does not identify executable contents.
Execution separately identifies the supported-source tree and rejects a change to that revision or to the configuration during the selected commands.

```sh
fr checks --run unit --basis <BASIS> --record-for <TRANSACTION>
```

The `fr-checks-1` JSON report includes each command's status, exit code, timing and bounded stdout/stderr.
`--quiet-success` omits successful stream text while preserving byte counts and every outcome field.
Failed checks still return diagnostics within `--output-bytes`; timeouts and excessive output remain failures.
This opt-in changes presentation only. Defaults, execution, coverage declarations and configuration digests remain unchanged.
It lists checks that did not run. A listing has `passed: null`; execution passes only when every selected command passes.
Failed commands, spawn failures, timeouts and excessive captured output produce a failing report and exit status 1.
Malformed configuration and selection errors produce the ordinary CLI error object with a nonzero exit status.
Each failure emits one JSON document.

After reviewing the listing, `--no-declarations` omits its repeated `checks` array and each result's `argv`, `cwd` and `covers`.
The report marks `declarations_omitted: true` and retains names, root, configuration basis, unselected names and every execution outcome and diagnostic.
Join result names to the listing with the same basis to recover command metadata, including declared timeouts and coverage.
The flag requires `--run`; stale or missing bases still refuse before any command starts.
It combines with `--quiet-success`. Default reports and listings keep their declarations.

Coverage descriptions come from project declarations. They do not establish test coverage, behavior preservation or formal verification.

## Source-bound transaction receipts

An execution report captures `source_revision` before the first selected command and compares it with the revision after every command.
The revision hashes each supported source file's path, Git-portable owner executable state and
content. It excludes `.fr-history`, `.git`, `target`, `node_modules` and `.lake` trees.
The report sets `source_snapshot_checked: true`; `source_snapshot_stable` and `configuration_stable` must both remain true for `passed: true`.
The configuration receives a fresh confined read and digest after execution.

`--record-for <TRANSACTION>` requires an applied source-history transaction.
Before execution it checks the transaction status, affected-file snapshots and current source revision.
When the transaction carries `required_checks`, its exact configuration digest and ordered check names must match the requested execution.
This refusal happens before a project command starts.
After every command passes without drift, it repeats those checks while holding the history lock.
It then appends one evidence row to the transaction.
The `frce1:` receipt hashes the configuration basis, common source revision and selected check names in declaration order.
Repeating the same evidence is idempotent.
`fr history show <TRANSACTION>` displays recorded rows, and undo or redo retains them as historical evidence for the named revision.
Corrupt receipt data makes the journal invalid instead of silently weakening the claim.

This is a boundary-snapshot receipt rather than a filesystem snapshot held during execution.
A command can mutate and restore a source file between observations.
Files in unsupported languages, check executable contents, dependencies, services, environment variables and external state do not enter the source revision.
The receipt therefore establishes which declared commands passed at one stable observed source boundary; it does not prove their declared coverage or general behavior preservation.

## Execution boundaries

Running checks executes project code with the user's inherited environment and permissions.
The digest review is a stale-configuration guard, not a command sandbox.
Commands may write files; these writes do not enter `fr` history. Use commands appropriate to the authorized task.
Arguments pass directly to the executable without shell expansion. An explicitly declared shell still interprets its own arguments.
Standard input is closed. Executable lookup uses the inherited PATH; relative executable paths use the declared working directory.

Working directories must exist inside the canonical project root and cannot traverse symlinks.
Configuration paths cannot traverse symlinks either. Concurrent filesystem replacement remains outside these checks.
On Unix, every command starts in a separate process group. The runner terminates remaining group
members after the direct command exits and on timeout or excessive output, then checks source
stability. A descendant can escape by entering another process group or starting a new session.
Other platforms clean up only the direct child. Interrupting `fr` itself can also leave descendants
running. Use an external process supervisor when a project needs stronger resource isolation.

Output capture uses temporary files to avoid pipe deadlocks and unbounded in-memory buffering.
The runner polls every 20 ms and stops a running child above 16 MiB on either stream.
This is a soft disk limit; a fast writer can exceed it between polls.
The output budget retains up to 65,536 raw bytes per stream, with exact omitted-byte counts for the observed capture.
UTF-8 replacement decoding and JSON escaping can expand the displayed text beyond that raw-byte budget.
Descendants can keep changing a capture until process-group cleanup completes, or longer when they
escape that group or run on a platform without group cleanup.

Configuration accepts at most 65,536 bytes and 32 checks, with timeouts from 1 through 3,600 seconds.
Unknown fields refuse. Names are unique ASCII letters, digits, underscores or hyphens, at most 64 bytes.
Each command has at most 128 arguments of 4,096 bytes each and 32 coverage descriptions of 512 bytes each.

CLI regressions cover preview, stale configuration, selection, failure reports, output truncation, timeouts, literal arguments and path refusals.
The execution layer has test evidence; no Lean proof covers process behavior or project-check semantics.

## Controlled report measurement

The retained workspace agent traces repeat the reviewed declarations in four check execution reports per trial.
Applying declaration omission to those frozen payloads gives this deterministic projection:

| Retained fr trial | Execution reports | Original payload tokens | Projected tokens |
|---|---:|---:|---:|
| regex repetition 1 | 4 | 2,720 | 1,804 |
| regex repetition 2 | 4 | 2,720 | 1,804 |

Each projection removes 916 tokens, or 33.7% of check-execution output, with all other fields held fixed.
These counts exclude the listing, prompts, skill reads, generated requests and other task outputs.
They use the original instrumented JSON wrapper and pinned tiktoken 0.12.0/o200k_base tokenizer.
No fresh agent ran with this option; the twelve autonomous trial scores remain unchanged.

The [retained comparison](../tests/agent-eval/checks-context.json) also runs the real CLI against a pristine pinned regex workspace.
After warming its checks, it compares quiet-success reports with and without declaration omission.
Joining results to the reviewed listing reconstructs the full report, apart from execution timings and varying successful stream lengths.
Both declared checks pass, and tracked source and index bytes stay unchanged.
The live stdout uses CLI formatting; its token counts differ from the instrumented transcript projection.

Reproduce the comparison after preparing the pinned workspace described by the [evaluation evidence](evaluations.md):

```sh
python3 tools/checks-context.py --fr target/debug/fr
target/agent-eval-venv/bin/python tools/checks-context.py --fr target/debug/fr --tokens
```

CLI regressions separately check failed exits, spawn errors, truncated invalid UTF-8, quiet-success composition and exact declaration reconstruction.
They also check that omitted declarations cannot bypass configuration review or cause an unselected command to execute.

## Toolchain and Rust build identities

`checks --toolchain` adds `fr-check-toolchain-2` evidence. It binds each executable's resolved path
and bytes, plus workspace Rust build inputs. The build inputs include `Cargo.toml`, `Cargo.lock`,
`rust-toolchain`, `rust-toolchain.toml`, `.cargo/config` and `.cargo/config.toml`.
Adding a previously absent build configuration changes this identity. Discovery excludes `.git`,
`.fr-history`, `target`, `node_modules`, `.lake` and directory symlinks. It accepts at most 1,024
build files, 4 MiB per file and 64 MiB in total. Unsupported build-input types refuse evidence;
a directory at a configuration filename can change Cargo behavior.

Declarations can also name environment keys and external identity files:

```json
{
  "name": "compiler",
  "argv": ["/absolute/toolchain/bin/rustc", "--error-format=json", "--crate-type=lib", "subject.rs"],
  "cwd": ".",
  "timeout_seconds": 30,
  "covers": ["Rust library compilation under the declared flags"],
  "environment": ["RUSTC_BOOTSTRAP"],
  "identity_files": ["/absolute/toolchain/lib/librustc_driver.dylib"]
}
```

Use the actual installed paths. A launcher such as rustup does not identify the compiler it selects.
Declare the selected compiler, driver libraries and other relevant external inputs when a check uses a launcher.
For Cargo, declare relevant keys such as `RUSTC`, `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS`.
External Cargo configuration and undeclared dependencies remain outside the automatic workspace scope.

Each check admits 32 distinct environment names and 16 distinct identity files. Environment records
contain names and value digests; they distinguish unset from empty without returning values.
Identity files can use absolute paths or confined paths relative to the check directory. Records bind
resolved paths and streaming content hashes, with a 256 MiB limit per file.

Toolchain evidence compares inputs before and after execution. Drift prevents passing evidence.
The checked-plan SDK also compares the reviewed toolchain before execution. These identities observe
boundaries; they do not detect a mutate-and-restore operation between observations.
Existing transaction receipts keep their original source/configuration contract.

## Retained compiler diagnostics

After a reviewed check runs, retain its full JSON report outside the source tree. Keep declarations
and request enough output for the diagnostic protocol. Rustc writes JSON diagnostics to stderr;
Cargo's `--message-format=json` writes protocol events to stdout.

```sh
fr checks --toolchain
fr checks --toolchain --run compiler --basis '<BASIS>' --output-bytes 65536
fr --json project compiler-evidence --from checks.json --digest '<REPORT-SHA256>' --check compiler --format rustc-json --limit 8
```

The report digest is SHA-256 over compact, sorted-key JSON with UTF-8 strings. The Python SDK computes it.
The adapter executes no compiler commands. It validates retained execution against current source,
configuration, executable, build and declared external identities. It requires full declarations;
`--no-declarations` output alone cannot establish those inputs.

Diagnostic facts bind their rule, confidence, parent, input identity and exact admitted source spans.
The adapter omits compiler-rendered source, suggestions and source-line text. Explicit source actions
use revision-bound file handles. Macro expansions, external files, unsupported paths and invalid byte
boundaries retain mapping gaps. Rust parser acceptance and compiler rejection remain separate observations.
A `syntax-accepted-compiler-error` record preserves that difference without asserting a parser defect.

`capture.complete` covers the retained diagnostic protocol. `disclosure_complete` covers the selected
page range. `complete` also requires a finished command. A failed compilation can have complete
diagnostic evidence; it remains a failed check. Missing Cargo completion events, unknown events,
malformed JSON, clipped streams, exhausted detail budgets and contradictory outcomes prevent completeness.
Each page admits 1–64 diagnostic rows and a 4 KiB–1 MiB response budget. Parsing admits 1,024 diagnostics,
eight child levels, 16 spans per diagnostic and 512 message characters. Truncation retains continuations
or explicit cutoffs. Missing diagnostics never establish runtime safety or source correctness.

The caller's digest binds retained output; it is not an execution attestation. Synthetic protocol tests
exercise hostile shapes separately from real rustc/Cargo acceptance. Five Lean theorems establish the
coverage conjunction, and native code agrees on all 16 Boolean cases. These prove no compiler or process semantics.
