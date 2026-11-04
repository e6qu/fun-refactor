# Continue the focused agent work

[PLAN.md](../PLAN.md#next-work) owns the active milestones.
Complete each milestone in one PR, including implementation, checks, required live attempts,
results and tracking. Keep preparation on that milestone's branch until the whole milestone is ready.
The three jobs are refactoring, targeted code changes and Lean proof writing.
[BUGS.md](../BUGS.md) owns actionable defects.
The [retained-results report](roadmap-status.md) audits earlier claims; its A–D counts are not completion counts for the new roadmap.

## Current work

PR [#597](https://github.com/e6qu/fun-refactor/pull/597) is merged. Its final
[CI run](https://github.com/e6qu/fun-refactor/actions/runs/41727356977) passed all
92 jobs, and all seven auxiliary workflows passed on the same commit.
M1 and M2 have their complete comparisons. M3 remains open: GLM and Kimi still
lack accepted page-count proofs within the original 150 seconds.

The [supplied-proof delivery results](../tests/agent-eval/proof-algorithms/local-results/M3-delivery/README.md)
retain all twelve attempts: eleven accepted patches, five complete successes,
six missing-accounting failures and one timeout without a patch. Both proofs
were supplied. This establishes neither better proof discovery nor a finish-card
advantage. Keep every original outcome and leave list reversal reserved.

OpenCode2 2.0.32 omitted final usage after session completion. Both no-model
reproductions pass with the pinned official 2.2.5 repair. The runner and native
admission reject missing or duplicate usage. A new comparison must have fresh
complete admission; the old receipts cannot authorize more calls.

Continue on `improve/typed-division-guidance` for the next part of M3.
The immediate task is a repeatable no-model diagnosis of the complete page-count
claim. The original source, generated model, invalid-input branch, signed-64
bounds and eleven-at-five example stay fixed. Start from the accepted manual
reference, change one proof step per control, and check with the current fr build.
An accepted reference is checked again after the rejection controls.

The new `page-count-diagnosis` job in the algorithm workflow owns this check.
Its report must distinguish wrong argument types, the wrong assumption,
truncating versus Euclidean division, missing conversion, missing arithmetic
relationships and unpacking an alternative as a pair. Its result is a tool
control, never a new agent attempt or a replacement for an old failure.

Before another comparison, record which controls actually fail and their first
useful Lean diagnostics. Then propose one instruction or tool change with a
prediction fixed before calls. Do not launch the old unrun division-fact plan,
repeat a closed slot, lengthen the deadline or claim the proof-writing milestone
complete from these controls. Keep the remaining M3 work in its milestone PR.

## What the merged proof work established

`spec proof-state` reads Lean's actual goals at a draft cursor. A solved branch is
not a checked complete proof. Require `proof-check` to pass before saving.
`spec prove` distinguishes a new `written` result from `already-present`, which
needs no repeat save. `history patch 4 7 --output change.patch` exports saved edits
4 and 7 together, using their recorded contents in that order. The shared
[Lean skill](../skills/fr/references/lean.md) explains the check and save loop.

The [twenty-session Euclid review](../tests/agent-eval/proof-algorithms/local-results/review-context.md)
retains eight independently accepted delivered patches and twelve complete-attempt
timeouts. It establishes no general graph advantage or list-reversal transfer
candidate. A ninth diff was written outside the project and remains a failed
attempt. The reader repair accepts the valid multiline proof form; GLM's last
retained draft still has Lean errors. Keep list reversal reserved and do not
reopen closed slots or add automatic backtracking without supporting observations.

Both valid reference proofs pass and all twelve broken algorithm controls are
rejected. Repaired fr accepts all eight delivered patches in a separate replay.
The original [fact grades](https://github.com/e6qu/fun-refactor/actions/runs/41427685363)
and [presentation grades](https://github.com/e6qu/fun-refactor/actions/runs/41427693183)
remain unchanged. Earlier interrupted batches, all original M3 outcomes and the
unrun division-fact plan retain their stated status. Raw local records stay private;
only reviewed summaries and submitted patches may be uploaded.

Both complete fixed-corpus audits preserve their original symbol and reference
facts. All nineteen collection groups affected by PR #593 have current retained
records. The metadata comparison preserves its earlier measurement: one saved
edit now uses 954 response bytes rather than 689, an extra 265 bytes. This is
response size, not program execution speed or evidence of agent improvement.

## Keep the other milestones concrete

[M2](../PLAN.md#m2-make-multi-file-refactorings-and-targeted-changes-dependable) names Rust parameter removal, the regex escaped-length feature, a Python invoice repair and a TypeScript imported-alias rename.
Each needs preserved-source checks, behavior checks, stale-preview refusal, undo, redo and receiver-checkout delivery.

[M3](../PLAN.md#m3-help-the-agents-write-repair-and-maintain-lean-proofs) names the Boolean language cases, page counting and an unsigned edit-range check.
The model writes tactics for the fixed property and uses Lean diagnostics to repair them.
Keep model proofs separate from checked claims about the source implementation.
Updating a source fingerprint cannot establish that connection.

The previous invalid-number-input and shared-list expansions are outside the active queue.
Extend analysis when a concrete refactoring, edit or proof task needs the missing rule.
Fix encountered bugs regardless of whether the focused work introduced them.

## Preserve existing behavior and original records

- Selected Rust and Python changes pass independent checks, exact undo/redo and patch replay.
- Saved answers agree with fresh analysis after relevant edits.
- Original-package examples retain their original imports and unfinished-analysis limits.
- Existing pinned Claude Code, Codex and older OpenCode controls remain records of their stated scope.
- Existing Lean models, source anchors and proof receipts retain their separate meanings.

The [earlier checklist](earlier-roadmap-checklist.md) retains the original A–D engineering items.
The [acceptance catalog](../tests/agent-eval/roadmap.json) and [report](roadmap-status.md) retain their original conditions.
Open claims stay open. Do not silently turn them into completion of M1, M2 or M3.

| Record | Location | What to preserve |
| --- | --- | --- |
| Selected current reports | [Explicit report map](../tests/agent-eval/current-scalar-reports.json) | Named files and original fingerprints |
| Earlier qualification | [Retained results](roadmap-status.md) | Original acceptance, current source checks and unfinished claims |
| First project changes | [Project-change record](project-change-milestone.md) | Humanize's original attempt and failed development attempts |
| Original-package questions | [Package records](package-milestones.md) | Original first attempts and their original auditors |
| Complete session costs | [CLI/Python comparison](project-session-cost.md) | Matching answers, setup boundaries and measured overhead |
| Stopped studies | [Candidate review](candidate-review-status.md) | Failed, cancelled and unstarted work |
| Detailed history | [Evaluation index](evaluations.md) | Original sources, revisions, outputs and limits |

A fingerprint identifies file contents; it does not establish correctness.
Do not rewrite an original hash to bless an edited source file.
The previous CodeGraph comparison, broad repeated-task study and stopped studies are inactive.

## Refresh and validate the whole change

1. Integrate current main before freezing inputs.
2. Finish code, instructions and recorded guide inputs before collecting results.
3. Use the [hosted collection groups](ci.md) to refresh every affected record.
   Keep original attempts and run the independent behavior checks.
4. Regenerate the retained-results report and documentation on GitHub.
   Confirm that this preserves earlier acceptance conditions while linking current work to PLAN.md.
5. Run the full static gate and changed runnable lessons before the final PR gate.
6. Check the full PR commit. Fix failures and timeout causes without raising limits or dropping coverage.

Start report publishers after code qualification passes. Run them one at a time and keep the branch free of other pushes until publication finishes.
A failed fast-forward push preserves the checked artifact; integrate the source change before publishing those same measurements again.

Builds, full tests and large collections run on GitHub.
Follow the [workstation rules](development.md#working-on-a-shared-desktop); do not bypass a guard refusal.
Dogfood fr on suitable changes and retain the reviewed edit and check results.

Every commit message is one line of at most 80 characters, with no body, trailers or AI attribution.
When authorized to merge, wait for passing CI and squash with an explicit subject and empty body.
Follow the user's current CLI and publication choices. M1 completion does not authorize a merge.

The [previous handoff](https://github.com/e6qu/fun-refactor/blob/c923d3cde7/docs/continuity.md) preserves its earlier starting point.

