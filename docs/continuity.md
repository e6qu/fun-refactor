# Continue the focused agent work

[PLAN.md](../PLAN.md#next-work) owns the active milestones.
Complete each milestone in one PR, including implementation, checks, required live attempts,
results and tracking. Keep preparation on that milestone's branch until the whole milestone is ready.
The three jobs are refactoring, targeted code changes and Lean proof writing.
[BUGS.md](../BUGS.md) owns actionable defects.
The [retained-results report](roadmap-status.md) audits earlier claims; its A–D counts are not completion counts for the new roadmap.

## Current milestone

Continue on `research/proof-guidance` in [draft PR #592](https://github.com/e6qu/fun-refactor/pull/592).
Keep its algorithm experiment, encountered fixes, results and tracking together.
M3 remains open; these diagnostics cannot replace its missing page-count proofs.
M1 and M2 have their merged comparisons. The [task guide](../tests/agent-eval/proof-writing/README.md) owns the fixed proof tasks and original records.

Merged PR #591 added interactive proof authoring. `spec proof-state`
reads Lean's actual goals at a draft cursor, including available assumptions and the original
claim. It cannot issue a proof receipt; `closed-at-position` can describe only one solved branch.
The shared [Lean skill](../skills/fr/references/lean.md) teaches the inspect/edit/check loop.
[Real Lean branch tests](https://github.com/e6qu/fun-refactor/actions/runs/41348371660/job/124973004157),
[deadline controls and packaged skill examples](https://github.com/e6qu/fun-refactor/actions/runs/41348371660/job/124973004148) pass.
[Full proof preparation](https://github.com/e6qu/fun-refactor/actions/runs/41348391677) and
[the next diagnostic’s controls](https://github.com/e6qu/fun-refactor/actions/runs/41353218026) also pass,
using the same checked executable. These remain tool checks, not new agent successes.
Keep full PR CI and current-report checks mandatory. These checks establish tool behavior; they add no agent successes.

**The full M3 comparison remains paused. The current diagnostic proves executable algorithms.**
Start with Euclid's GCD: the fixed function must stop and return the standard GCD
for every pair of nonnegative inputs. Reserve accumulator list reversal for transfer.
The [algorithm guide](../tests/agent-eval/proof-algorithms/README.md) owns the exact task.
Its first batch stopped in slot five. Preserve the [interruption](../tests/agent-eval/proof-algorithms/interrupted-results/README.md),
including three timeouts, one Kimi submission and all unstarted slots.
Fix detached-server accounting, output flushing and multi-input induction before
fresh hosted qualification and new frozen plans. Do not reuse either invalidated plan.
The correctness inspector also kept three background programs alive for one file.
A one-file-worker prototype reduces its memory use. Qualify the rebuilt command
and the complete Mac CLI/server/fr/Lean workload before live calls; the standalone
termination preflight missed that combined cost.
Then compare declarations in eight GLM/Kimi sessions and equal-information
checklist/graph guidance in twelve Luna/GLM/Kimi sessions. Keep 150 seconds for each
complete attempt. Full builds and independent grading stay on GitHub.
Use the [improvement method](agent-improvement.md) and [next tasks](../PLAN.md#next-work).
Keep the old full-study controllers stopped. Do not restart an old full-comparison plan.
Keep the diagnostic's original source, checker and input identities. Do not rerun its closed slots.
Native and hosted regression checks can continue.

The latest complete M3 comparison has 28 accepted patches and eight page-count timeouts.
Both GLM and Kimi still lack accepted page-count proofs.
Each finished four of six fr-available attempts in the first and latest complete comparisons.
The [behavior review](../tests/agent-eval/proof-writing/behavior-review.md) distinguishes changed tool use from unchanged total success.

The evaluator now rejects stopped successes, successes beyond their recorded deadline and invalid timing values.
Its [audit](../tests/agent-eval/proof-writing/report-audit.json) retains hashes and expected statistics for twelve complete reports and 480 attempts.
Focused regression checks preserve all those statistics. Original reports and verdicts stay unchanged.

The [eight-attempt diagnostic](../tests/agent-eval/proof-feedback/README.md#what-the-first-run-established) compared a small division repair with and without the initial error report.
Independent grading accepted all eight submitted proofs. Six complete attempts passed.
GLM timed out after a passing proof and package check; Kimi overwrote `.fr/checks.json` with a proof-check report.
Both remain failed. The decision rules give no demonstrated benefit from supplying the report.

Its shorter prompt omitted the shared execution instructions. The runner now adds those instructions to the diagnostic too.
A regression rejects the measured prompt and passes the correction. The skill distinguishes check settings from reports and explains proof writes' `applied` field.
The [four construction attempts](../tests/agent-eval/proof-construction/README.md#what-the-first-run-established) used these changes after hosted qualification passed.
Independent grading accepted three patches; Kimi timed out once without a patch. All four kept the check settings unchanged.
Both Kimi attempts hit a reader defect: Lean accepts inline calculation steps that measured fr rejects before Lean runs.
All 29 hosted preparation checks passed in run 41214408630. The exact rebuilt-command replay also passed without model calls.
It accepts the earlier valid calculation and lets Lean reject the timed-out draft's unknown names.
That replay exposed a second defect: tagged unknown-name errors lost their structured draft locations.
Its repair passed all 29 hosted preparation checks and Python-record checks.
An [exact replay](../tests/agent-eval/proof-construction/diagnostic-control-summary.json) now keeps the error code, message and submitted line and column.
Keep all four original outcomes unchanged.

The [four page-count attempts](../tests/agent-eval/proof-page-count/README.md) all timed out without patches; independent grading confirms all four failures.
They exposed another reader defect in nested assumption patterns, such as `⟨⟨a, b⟩, c⟩`.
The repair passed all 29 hosted preparation checks.
The [rebuilt-reader replay](../tests/agent-eval/proof-page-count/reader-control-summary.json) reaches Lean and still rejects the original draft.
The [manual error review](../tests/agent-eval/proof-page-count/draft-error-review.json) separates redundant introductions, wrong assumption grouping and missing division facts.
Its final manual control proves the unchanged claim. It is neither an agent success nor a timing comparison.
Retain the [division-fact protocol](../tests/agent-eval/proof-division-facts/README.md)
as an unrun supporting arithmetic diagnostic. Its eight slots stay unstarted;
it does not replace the executable-algorithm study or its original claims.

The requested [team workflow](team-solve.md) uses fresh local Luna, GLM and Kimi sessions with explicit file handoffs.
The [Python pilot](../tests/agent-eval/team-solve/README.md) passed eight cases in 249.9 seconds across five CLI sessions.
It has no solo baseline, and no role invoked fr. The actual number of provider requests is unknown.
The runner now supports a single session, an all-Luna team and the mixed team with explicit complete-run budgets.
Nineteen workflow controls passed the hosted documentation gate.
Complete hosted CI remains required before readiness.
No paid comparison of those conditions has run. Team results cannot close the M3 solo requirements.

Existing native repairs include parameterized local Lean lemmas and warning rejection in submitted drafts.
Hosted M3 checks passed; a clean proof remains checkable while another theorem is unfinished.
The current skill and restored shared instructions passed M1, M2 and documentation qualification before the four construction attempts.
Dependency controls needed a repair because they unfolded the model twice and triggered the new unused-argument warning check.
Focused checks now accept the clean reference and reject the changed dependency. Keep the strict warning rule.
The known-file fr lookup also passes locally by indexing exactly one file; its CI deadline remains three minutes.

All 335 M3 outcomes remain in the [complete and interrupted archives](../tests/agent-eval/proof-writing/README.md#earlier-attempts).
The eight repair, four construction and four page-count attempts are separate; they cannot replace a full-study failure.
Retain original input identities, failures, sleep limitations and record hashes.
Only submitted patches and summaries may leave the workstation.
Keep logs, drafts, source archives, session exports and account data local.
Old diagnostic copies and executables may have verified compressed copies; preserve their private integrity records when restoring them.

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

