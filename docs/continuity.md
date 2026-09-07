# Development continuity

M4k is complete in `6d0928b`. M4l has four prepared regex workspace sessions and awaits autonomous trials.
The user authorized local commits. Publishing and pushing remain outside this request.

## Current state

M4g adds declared project checks through `.fr/checks.json` and `fr checks`, committed as `893b9db`.
M4h establishes real-agent acceptance on the pinned strsim 0.11.1 release, committed as `6015c90`.
It pairs fresh fr and ordinary-file agents on Unicode Sørensen–Dice correctness and a normalized OSA API addition.
All four initial trials pass, including independent oracles, declared checks, clean patch receivers and exact undo/redo.
The first fr trials use 18,628 and 17,370 retrieved-context tokens; file trials use 9,469 and 10,393.

M4i implementation commit `0498c6b` adds `project find`, opt-in `checks --quiet-success` and more selective skill references.
Lookup returns paged handles and optional headers, with exact or literal substring matching and selected subtree scope.
It matches full names before clipping and binds cursors to revision and query options. Index construction cost remains unchanged.
Quiet-success checks retain failed-command diagnostics and explicit omitted-byte counts.
The skill separates authoring from recipes and patch delivery from Git administration, with nine optional references and 33 executable examples.

All four fresh follow-up trials pass with zero recorded tool failures and zero human task corrections.
The fr trials use 12,287 and 13,109 tokens, reductions of 34.0% and 24.5% against the first fr trials.
Fresh file trials use 6,087 and 8,397 tokens, so fr still consumes more context on these small tasks.
Both arms received a prompt clarification to keep handles revision-bound when used, replacing unconditional refresh instructions.
Tasks, independent oracles and mandatory workflow stages stayed the same. Multiple changes prevent exact attribution to a single feature.
See the [follow-up report](agent-context-followup.md) for comparisons, provenance and limits.

## Durable evidence

The original bundle is `tests/agent-eval/results/2026-09-07`; the follow-up is `tests/agent-eval/results/2026-09-07-context`.
Each retains prompts, transcripts, scores, patches and its original portable skill snapshot, with manifest checksums.
The original bundle includes interrupted Cargo-workspace pilots, excluded from scored results.
The follow-up has no pilots. Preparation refuses Cargo ancestors and preflights upstream tests.
Never rewrite the original evidence when changing the tool, skill or harness.
Recording now preserves scored failures too; behavioral replay still fails for unsuccessful trials.

Temporary sessions remain under `/private/tmp/fr-agent-acceptance-2026-09-07` and `/private/tmp/fr-context-followup-2026-09-07`.
Frozen binaries are `target/agent-eval-bin/fr` and `target/agent-eval-bin/fr-m4i`.
Use repository evidence for replay and token audits; temporary projects are disposable after retention.
Replay runs recorded patches and oracles, without rerunning agents.

## Validation

The M4i full native/WASM gate passes, including 127 project CLI scenarios, ten check scenarios and all 33 skill examples.
Its log is `/tmp/fr-m4i-full-check.log`. Capability coverage remains 311/311.
Strict verification passes with twenty-two fresh anchors and signature maps, zero obligations and all 29 Lean build jobs.
The report is `/tmp/fr-m4i-spec-verify.json`. No new claim of full parser or process verification follows.
Follow-up token auditing passes; its report is `/tmp/fr-m4i-token-audit.json`.
The final acceptance regression covers eight recorded patches and ten harness regressions, alongside documentation checks.
Its log is `/tmp/fr-m4i-final-check.log`. Only replay coverage, evidence and documentation changed after the full implementation gate.

Use cached dependencies with `CARGO_HOME="$PWD/target/cargo-home"` and `CARGO_NET_OFFLINE=true`.
`tools/check.sh` runs the native/WASM gate; `fr spec verify kernels --json` checks strict source correspondence and Lean builds.
The separate deep audit remains optional unless a change warrants it.
Prose budgets remain 272 long sentences and 9,335 Rust comment lines, with other tracked categories at zero.
Scoring uses `target/agent-eval-venv/bin/python`, tiktoken 0.12.0 and a checksum-pinned o200k_base vocabulary.
Reference tokens measure instrumented text, excluding system context, hidden reasoning and billed usage.

## History report reduction

M4j adds opt-in `--no-diff` for history apply, undo, redo and recover with `--write`.
Completion reports retain transaction, action, applied status, paths, existence and modes, with an explicit omission flag.
Preview diffs, snapshots and guards remain intact. The skill also distinguishes full names from fragments before lookup.
Eight history CLI scenarios, eighteen history unit scenarios and all 33 skill examples pass.
Controlled replay matches both recorded edits and patches, with 69.8% and 59.1% less completion-report context.
Including undo/redo previews, the reductions are 41.9% and 35.5%; this does not measure fresh agents or total task context.
The retained report is `tests/agent-eval/history-context.json`; `tools/history-context.py` reproduces it with optional pinned token counts.
Strict verification passes in `/tmp/fr-m4j-spec-verify.json`, with twenty-two fresh anchors and signature maps and zero obligations.
The full native/WASM gate passes in `/tmp/fr-m4j-full-check.log`, retaining 311/311 capability coverage and 29 Lean build jobs.
The acceptance regression also replays all eight earlier patches and runs ten harness regressions.
M4j implementation commit `4ae00eb` includes skill examples, controlled measurements, regression coverage and refreshed local docs.
Repeating the measurement after the full gate produced identical reports and counts; the retained report identifies the final rebuilt binary.

## Next steps

Evaluate larger projects and tasks spanning package boundaries with repeated paired trials using the current options.
The skill now distinguishes full names from fragments; fresh agents must establish whether that guidance avoids empty-result round trips.
Use those traces to select further inspection and report changes before claiming general context savings or timing improvements.
Keep skills selective and executable against the distributed binary.
Further authoring operations, M5 automated Lean adoption and M6 framework migrations remain open in PLAN.md.

## Active workspace evaluation

The pinned regex source commit is `2b527599eb9eea0dcc288c704584f242f26a5c61`, with an unmodified archive and a separate pinned Cargo.lock.
The public escape-into task requires following functionality from the regex facade into regex-syntax.
Its `deny(missing_docs)` lint exposed the need for leading outer documentation in inserted Rust functions.
The authoring extension passes 31 scenarios. The full native/WASM gate passes in `/tmp/fr-m4k-full-check.log`.
Strict verification passes in `/tmp/fr-m4k-spec-verify.json`: twenty-two fresh anchors and signature maps, zero obligations and 29 Lean build jobs.
Capability coverage remains 311/311. Sixteen harness regressions pass in `/tmp/fr-m4l-harness-final.log`.
The controlled rehearsal passes all four validation stages, exact history and patch delivery.
Its 3,174-case oracle passes on the project and receiver and rejects three compiled negative controls.
Durable evidence is `tests/agent-eval/regex/rehearsal.json`; source archive, normalized modes and lock details are in the [workspace report](agent-workspace-evaluation.md).
The harness now supports `prepare --project regex --repetitions 2`, producing four independent trial directories.
The default strsim preparation and existing evidence remain compatible.
Final acceptance replay and documentation links pass in `/tmp/fr-m4l-final-check.log`, including all eight earlier patches and the controlled history comparison.
Fresh agents have not run this task; controlled results must not count as autonomous acceptance.

Prepared sessions are under `/private/tmp/fr-regex-agent-eval-2026-09-08`:

- `regex-escape-into-fr-r1`
- `regex-escape-into-files-r1`
- `regex-escape-into-fr-r2`
- `regex-escape-into-files-r2`

All four have matching original project/receiver snapshots, 453 tracked paths, 5,553,380 Rust source bytes and no recorded agent events.
The frozen binary is `target/agent-eval-bin/fr-workspace-eval`, with digest recorded in every session and the rehearsal report.
Do not rebuild or change the binary, skill snapshots, task prompt, harness or oracle during active trials.
The current session requires explicit user authorization to spawn sub-agents; a request for the four evaluation agents is pending.
After authorization, give each prompt to a fresh agent without conversation history and follow the same tool boundary in both arms.
Record actual runtime and interventions with `record --execution-note`; never label a controlled rehearsal as an autonomous result.
Score and retain every repetition, including failures, then report per-trial observations before aggregate comparisons.
