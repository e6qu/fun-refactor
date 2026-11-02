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

**The full M3 comparison remains paused. All twenty Euclid development sessions are finished.**
The [results review](../tests/agent-eval/proof-algorithms/local-results/review-context.md)
records eight independently accepted delivered patches and twelve complete-attempt
timeouts. Facts: GLM 0/2 with either handout; Kimi 0/2 selecting and 1/2 supplied.
Presentation: Luna 2/2 with either layout; Kimi checklist 1/2 and graph 2/2;
GLM 0/2 with either. One GLM session hit a confirmed reader bug, so that comparison
is uncertain. No profile met the planned two-versus-zero transfer rule. Keep list
reversal reserved and do not add model sessions or rerun closed slots.

Original independent runs: [facts](https://github.com/e6qu/fun-refactor/actions/runs/41427685363)
and [presentation](https://github.com/e6qu/fun-refactor/actions/runs/41427693183).
The frozen-source manifest pins the original checking code. Current qualification
must audit those reports against that source and replay retained patches separately.
A ninth diff was written outside the project and timed out; keep it as a diagnostic,
never a delivered success. Raw records and drafts remain private.

The multiline-lambda reader repair passes both executable-GCD reference controls
and all twelve rejection controls. The measured fr had rejected the valid reference
before Lean ran. Current-tool replay accepts all eight delivered patches and the
separate misplaced diagnostic; original scores stay unchanged.
The original GLM draft also had Lean errors; repairing the reader cannot credit it.
The shared skills now address observed flag mistakes, clipped diagnostics, neighboring
broken proofs, already-present writes and project-root patch delivery. These changes
have not been measured as agent improvements. No evidence yet supports building an
automatic proof-backtracking controller.

Both earlier interrupted algorithm cohorts remain unchanged. Complete scripted
admission passed for all three clients with 128 MiB reserved inside the original
768-MiB ceiling. The observer runs outside the CPU throttle; actual work stays
inside the workstation guard. Keep complete attempts at 150 seconds.
The smaller task disables automatic imports in both entry and algorithm files,
then imports only the required GCD library. Implementation and claims stay fixed.
Older OpenCode resource records omitted detached servers; they cannot establish
whole-client CPU or memory compliance. Their original times and patch verdicts stay.

The 335 original M3 outcomes and sixteen earlier diagnostic outcomes remain unchanged.
The division-fact plan stays unrun. The mixed-agent Python pilot has no solo baseline
and no useful fr step; it does not close M3. Preserve all original report hashes.

The full evidence family has fresh successful collections. Retain it on GitHub,
with one publisher at a time on the PR branch. Verify both complete pinned benchmark
corpora: worktrees must disable inherited sparse exclusions and retain every tracked
source. Both complete audits now pass; see the reader control summary in the
index-consumer records. Invalid additional modern-corpus probes supply no complete-source result.
Use the reviewed probe record; do not raise any timeout. Then publish one source change at a time,
then pass the full final-head PR gate. Keep [PLAN.md](../PLAN.md#next-work) current.
Do not merge PR #592. Only reviewed summaries and original patches may be uploaded;
logs, source archives, drafts and account data stay local.

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

