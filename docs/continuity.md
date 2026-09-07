# Development continuity

M4j is complete: history writes can omit repeated diff text after preview. Broader agent efficiency remains open.
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

## Latest completed increment

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
The M4j commit includes implementation, skill examples, controlled measurements, regression coverage and refreshed local docs.

## Next steps

Evaluate larger projects and tasks spanning package boundaries with repeated paired trials using the current options.
The skill now distinguishes full names from fragments; fresh agents must establish whether that guidance avoids empty-result round trips.
Use those traces to select further inspection and report changes before claiming general context savings or timing improvements.
Keep skills selective and executable against the distributed binary.
Further authoring operations, M5 automated Lean adoption and M6 framework migrations remain open in PLAN.md.
