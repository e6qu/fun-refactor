# Native OpenCode trials, October 2, 2026

These are development integration tests on the three previously reviewed explanation tasks.
They do not establish general efficiency, independent-task performance or verified provider cost.
See [the protocol and results](../../../../../docs/opencode-native-tools.md).

The installed tools were OpenCode 1.18.34 and `fr` 0.35.0. Each plan binds the exact `fr` binary
hash and evaluator sources. No local compiler build ran. Runner snapshots preserve the code used
for each group. Provider credentials are absent from these artifacts.

## Frozen groups

- `preflight`: four cache-expiration attempts. All four passed. `report.json` uses the fresh-directory
  replay fix, after the first report call exposed an unpack error. Scores did not change.
- `repositories`: eight key-rotation and retry-callback attempts, frozen after the preflight fixes.
  The original report has two passes, three citation failures, two auditor failures and one timeout.
- `access-probe`: two later controls on a handwritten function. The prompt requests `fr find/show`
  when available. Both passed; the `fr` control made both CLI calls. These controls are separate
  from the twelve repository attempts and cannot establish voluntary tool choice.

Each group includes its plan, original report, corrected review, raw attempt files and runner
snapshot. `summary.json` joins the groups without changing their original records. Source snapshots
live in the plans. Disposable unpacked workspaces and caches do not belong to the retained evidence.

The source-hash tool description changed only for the later access probe. Earlier plans preserve
schema version 1. The new version says to use an empty string for a first read.

## Auditor correction

Both Kimi retry attempts received source-hash errors, recovered, and submitted correct answers
with the required source. OpenCode represents those tool errors in `state.error`. The first
auditor looked only at `state.output`, so it rejected both transcripts before grading.

The corrected auditor matches the exact host error to the native error result. `review.json`
rechecks the retained transcripts without new model calls. Both Kimi retry answers pass.
Their original records still say failed. Across both repository groups, the original total is
six passes; the reviewed total is eight of twelve. The three citation failures and timeout remain.

The three key-rotation failures have correct factual values. Their signing-key citations omit the
call from signing into key derivation, which the frozen rubric requires. The review does not relax
that criterion. The GLM retry `fr` attempt stopped at 120 seconds and has no completed session export.

Five of six `fr`-enabled repository attempts recorded no `fr` call. Kimi's retry attempt used one
`fr map`, then ordinary reads. The separate access control confirms native `fr find/show` work.
These results motivate testing public guidance and task selection; they do not establish token savings.

## Replay

Use `tools/native-rehearsal.py report PLAN ATTEMPTS` for the original outcomes and `review` for the
separate corrected assessment. Both run offline. On the developer workstation, wrap them in
`python3 /Users/zardoz/.codex/tools/fr-local-guard.py`. CI replays all three groups and checks the
recovered errors, preserved failures and exact source from the `fr` access control.
