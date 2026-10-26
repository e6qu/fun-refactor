# Continue the focused agent work

[PLAN.md](../PLAN.md#next-work) owns the active milestones.
Complete each milestone in one PR, including implementation, checks, required live attempts,
results and tracking. Keep preparation on that milestone's branch until the whole milestone is ready.
The three jobs are refactoring, targeted code changes and Lean proof writing.
[BUGS.md](../BUGS.md) owns actionable defects.
The [retained-results report](roadmap-status.md) audits earlier claims; its A–D counts are not completion counts for the new roadmap.

## M1 results and the next change

Use the [three selected profiles](../PLAN.md#target-agents): Codex with Luna Fast, and OpenCode2 with GLM-5.3-Flash or non-Pro Kimi K3.
The [M1 report](../tests/agent-eval/selected-agents/local-results/README.md) retains all thirty-six local CLI attempts.
Read the successful outcomes, timeouts and measured overhead before changing the next task route.
The [client and task guide](../tests/agent-eval/selected-agents/README.md) explains the pinned versions and scripted controls.
Those controls use supplied answers; they stay separate from the agents' actual patches.
The requests were:

- Rename Boltons' `clamp` to `clamp_value` through its declared consumers; preserve the result of limiting 70 to 0 through 50.
- Repair the recorded Unicode Dice comparison and pass its independent regression set.
- Have the agent write a Lean proof of the Rust access model's exact `staff || owner` rule.

The user chose the installed native CLIs with their existing accounts. Credentials and full run records stay local.
Only submitted patches and summaries are public. GitHub independently checks each patch in an isolated container.
Every timed-out slot remains failed and was not retried. The interrupted-session recording fix continued only unstarted slots
under a second immutable packet; the public summary keeps both packet hashes and each original record hash.
There is no shared enforceable token/request cap, and actual billing is unknown.

M2's implementation and deterministic controls now pass. Three complete 48-attempt comparisons remain public:
[original](../tests/agent-eval/project-edits/first-results/README.md),
[shorter guides](../tests/agent-eval/project-edits/guided-results/README.md) and
[actual clock](../tests/agent-eval/project-edits/clocked-results/README.md).
They accepted 33, 40 and 41 attempts respectively. GLM and Kimi still lack an accepted fr regex result.

The next fix replaces a compilation-only public check with actual byte, text and allocation checks.
It also avoids optimized code generation for those behavior checks and protects the supplied checking files during cleanup.
Validate correct and deliberately wrong controls on the study's macOS and Rust versions before freezing another full comparison.
Keep all old failures and the unchanged independent grader. Keep the task limit at 150 seconds.
Implementation, complete results and tracking still belong in one M2 PR.
Do not feed the model a repair, proof tactic or edit location supplied by the evaluator.

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

Builds, full tests and large collections run on GitHub.
Follow the [workstation rules](development.md#working-on-a-shared-desktop); do not bypass a guard refusal.
Dogfood fr on suitable changes and retain the reviewed edit and check results.

Every commit message is one line of at most 80 characters, with no body, trailers or AI attribution.
When authorized to merge, wait for passing CI and squash with an explicit subject and empty body.
Follow the user's current CLI and publication choices. M1 completion does not authorize a merge.

The [previous handoff](https://github.com/e6qu/fun-refactor/blob/c923d3cde7/docs/continuity.md) preserves its earlier starting point.

