# Development continuity

The active request is to complete the first real-agent acceptance milestone, test it, commit coherent increments and refresh local documentation.
The user authorized local commits. Publishing and pushing remain outside this request.

## Current work

M4g adds declared project checks through `.fr/checks.json` and `fr checks`.
Eight focused CLI scenarios pass. The portable skill has 33 executable examples and passes its validator.
The full native/WASM gate passed, including 311/311 capability coverage and 29 Lean build jobs.
Its log is `/tmp/fr-m4g-full-check.log`.

M4h evaluates two code tasks on the unmodified strsim 0.11.1 release.
The archive checksum matches Cargo.lock and its metadata records upstream commit 76c5a900e6e12cfc605eee5ab6e36300384c8682.
Tasks cover Unicode Sørensen–Dice correctness and a normalized OSA API addition.
Each task pairs a fresh agent using fr with a fresh agent using ordinary file tools.
Both arms share declared checks, patch application, exact undo/redo and independent behavioral oracles.

The initial pilot under this repository's target directory failed Cargo workspace discovery.
Those interrupted pilots remain outside scored results. The harness now refuses preparation beneath a Cargo project and preflights upstream tests.
Active trials live under `/private/tmp/fr-agent-acceptance-2026-09-07`.
Their prompts, events and snapshots must remain untouched while agents run.
The frozen binary is `target/agent-eval-bin/fr`.

## Next steps

Commit the validated declared-check feature with its documentation.
Complete all four fresh-agent trials, score their recorded payloads and verify independent oracles.
Test the evaluation harness and retain reproducible results, patches, provenance and measurement limits.
Refresh PLAN.md, the skill handoff and project-context evaluation with the observed results.
Do not claim general context savings from a small experiment or count hidden model reasoning as measured data.

## Local validation

Use the repository's cached dependencies with `CARGO_HOME="$PWD/target/cargo-home"` and `CARGO_NET_OFFLINE=true`.
`tools/check.sh` runs the native/WASM gate; `fr spec verify kernels --json` checks strict source correspondence and Lean builds.
The separate deep audit remains optional unless a change warrants it.
Prose budgets remain 272 long sentences and 9,335 Rust comment lines, with other tracked categories at zero.

Evaluation scoring uses `target/agent-eval-venv/bin/python`, tiktoken 0.12.0 and a checksum-pinned o200k_base vocabulary.
The reference encoding measures instrumented text; it does not establish the serving model's billed context use.
