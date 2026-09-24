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

For compiler investigations, review `checks --toolchain` and retain full execution output with
`--output-bytes 65536`. Declare relevant environment keys and external identity files in each check.
The toolchain identity also covers workspace Rust manifests, lockfiles, toolchain files and `.cargo` configuration.
Use the actual compiler path or explicitly declare the compiler selected by a launcher.

`fr_ir.compiler_evidence.CompilerEvidence.inspect(client, retained, check="compiler")` reads rustc JSON;
select `format="cargo-json"` for Cargo. It executes no checks. Read capture, disclosure and command
outcome separately. Complete diagnostics can describe a failed compilation. Follow exact source actions
only when needed. Keep expanded, external and invalid spans unresolved. Attach observations through
`page.attach(plan, client, step)`; failing checks remain failures. The caller vouches for retained output.
