# Development continuity

The first real-agent acceptance milestone is complete. The active follow-up reduces its observed context overhead and repeats the paired experiment.
The user authorized local commits. Publishing and pushing remain outside this request.

## Current work

M4g adds declared project checks through `.fr/checks.json` and `fr checks`, committed as `893b9db`.
The original nine focused CLI scenarios pass. The portable skill has 33 executable examples and passes its validator.
The full native/WASM gate passed, including 311/311 capability coverage and 29 Lean build jobs.
Its log is `/tmp/fr-m4g-full-check.log`.

M4h evaluates two code tasks on the unmodified strsim 0.11.1 release.
The archive checksum matches Cargo.lock and its metadata records upstream commit 76c5a900e6e12cfc605eee5ab6e36300384c8682.
Tasks cover Unicode Sørensen–Dice correctness and a normalized OSA API addition.
Each task pairs a fresh agent using fr with a fresh agent using ordinary file tools.
Both arms share declared checks, patch application, exact undo/redo and independent behavioral oracles.
All four trials pass, with zero recorded tool failures and zero human task corrections.
The fr trials use 18,628 and 17,370 retrieved-context tokens; ordinary-file trials use 9,469 and 10,393.
These first results establish functionality while showing context overhead on a small project.

The initial pilot under this repository's target directory failed Cargo workspace discovery.
Those interrupted pilots remain outside scored results. The harness now refuses preparation beneath a Cargo project and preflights upstream tests.
Completed live trial directories remain under `/private/tmp/fr-agent-acceptance-2026-09-07`.
The durable evidence is `tests/agent-eval/results/2026-09-07`, including prompts, transcripts, patches, scores and pilot failures.
Use the repository copy for replay and token audits; temporary projects are disposable after evidence retention.
The frozen binary is `target/agent-eval-bin/fr`.

## Next steps

M4i adds `project find`, opt-in quiet-success check output and more selective skill references.
The full native/WASM gate passes, including 127 project CLI scenarios, ten check scenarios and 33 skill examples.
Its log is `/tmp/fr-m4i-full-check.log`. Strict verification passes with twenty-two fresh anchors and signature maps and zero obligations.
The report is `/tmp/fr-m4i-spec-verify.json`; all 29 Lean build jobs pass.
Four fresh trials pass with the frozen `target/agent-eval-bin/fr-m4i` binary and unchanged task oracles.
Sessions are under `/private/tmp/fr-context-followup-2026-09-07`. Retain and audit their evidence after committing the implementation.
Report improvements against the original evidence separately from the new ordinary-file comparison.

Ten harness regressions, four-patch behavioral replay and exact token auditing pass.
The final full gate passed; its log is `/tmp/fr-m4h-full-check.log`.
The acceptance commit contains the harness, evidence, extra output-limit regression and refreshed documentation.
Further product work should target the remaining inspection and transaction report overhead using measured traces.
Broader project evaluation, M5 automated Lean adoption and M6 feature migrations remain open in PLAN.md.
Do not claim general context savings from a small experiment or count hidden model reasoning as measured data.

## Local validation

Use the repository's cached dependencies with `CARGO_HOME="$PWD/target/cargo-home"` and `CARGO_NET_OFFLINE=true`.
`tools/check.sh` runs the native/WASM gate; `fr spec verify kernels --json` checks strict source correspondence and Lean builds.
The separate deep audit remains optional unless a change warrants it.
Prose budgets remain 272 long sentences and 9,335 Rust comment lines, with other tracked categories at zero.

Evaluation scoring uses `target/agent-eval-venv/bin/python`, tiktoken 0.12.0 and a checksum-pinned o200k_base vocabulary.
The reference encoding measures instrumented text; it does not establish the serving model's billed context use.
