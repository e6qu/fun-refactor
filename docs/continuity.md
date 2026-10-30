# Continue the focused agent work

[PLAN.md](../PLAN.md#next-work) owns the active milestones.
Complete each milestone in one PR, including implementation, checks, required live attempts,
results and tracking. Keep preparation on that milestone's branch until the whole milestone is ready.
The three jobs are refactoring, targeted code changes and Lean proof writing.
[BUGS.md](../BUGS.md) owns actionable defects.
The [retained-results report](roadmap-status.md) audits earlier claims; its A–D counts are not completion counts for the new roadmap.

## Current milestone

M1's [36-attempt report](../tests/agent-eval/selected-agents/local-results/README.md) retains the Boltons rename,
Unicode repair and Rust access-rule proof. M2's [48-attempt report](../tests/agent-eval/project-edits/local-results/README.md)
retains 43 accepted attempts and five timeouts; all 48 submitted patches passed.
Both milestones were merged. Their original reports and failures remain intact.

Finish M3 on `milestone/complete-proof-authoring`, including the merged release update.
The [task guide](../tests/agent-eval/proof-writing/README.md) owns the exact claims and limits.
Keep the complete milestone in one PR. M3 remains in progress.

The latest comparison ran 36 local attempts with the requested native Codex and OpenCode2 releases.
Luna kept its high/fast selection; GLM and Kimi kept explicit low reasoning.
All 29 preparation jobs, the deep audit and the unchanged M1/M2 checks passed before the first model call.
The fresh measurement collections and all eight report publishers also passed.

The latest complete comparison adds explicit checking commands and an arithmetic lesson.
It retains 28 submitted patches and eight page-count timeouts without patches.
All three agents used actual fr checks and writes in their access and range submissions.
[Independent grading](https://github.com/e6qu/fun-refactor/actions/runs/41062241970) accepted all 28 patches and retained all eight failures.
GLM and Kimi still submitted no page-count proof in either setup. M3 remains open.
Read the [behavior review](../tests/agent-eval/proof-writing/behavior-review.md) before choosing another change.

The preceding [claim-view report](../tests/agent-eval/proof-writing/claim-view-results/README.md)
keeps all 27 independently accepted patches and nine timeouts unchanged.
Its corrected [grading run](https://github.com/e6qu/fun-refactor/actions/runs/41050239510) passed all twelve jobs.
The collector finished in 22 seconds under its original five-minute deadline by retaining a limited checkout
with every required checker input. Its combined verdicts match the nine preserved checking groups.

The measured tool and checker are identified in the public summary's `fr_build` and `controls` fields.
The controller source is in `collection_receipts`. Audit those exact inputs; do not rewrite historical source IDs.
The new export used its unchanged frozen collector. Checker identities now exclude archived reports.

The next fixes and instructions were prepared after the new comparison:

- Valid parameterized local Lean lemmas now have reader rules and regression examples.
- Draft checking now reports and rejects warnings in the submitted tactics. Other unfinished theorems remain separate.
- The root skill links arithmetic directly and starts with a checked `refine ?_` draft, so feedback comes before a complete proof.
- The [arithmetic lesson](../skills/fr/references/lean-arithmetic.md) also demonstrates `exact?` on a specific missing fact.
  All four exact lesson blocks compile with Lean 4.28.0. A separate native test must confirm the new local-lemma syntax through fr.
- The root is 816 bytes; the largest route is 7,145 bytes. All original context ceilings are unchanged.

The earlier instructions passed the [proof workflow](https://github.com/e6qu/fun-refactor/actions/runs/41047411811),
[M1](https://github.com/e6qu/fun-refactor/actions/runs/41047510059) and [M2](https://github.com/e6qu/fun-refactor/actions/runs/41047513202)
before all 36 new calls. The latest repairs change native source and require a new build and hosted qualification.
Parser generation exposed an ambiguity between a local lemma's name, its parameters and constructor patterns.
The grammar now gives the local declaration priority; generation and fixture checks must pass before a native rebuild.
Validate the repairs and refresh affected records before freezing another complete comparison.
Keep the same models, claims, two repetitions and 150-second limit.

All 335 outcomes remain: the latest 36 plus the 299 in [earlier packets](../tests/agent-eval/proof-writing/README.md#earlier-attempts).
The previous low-setting publication is archived byte-for-byte in `low-reasoning-results`.
The preceding checked publication is archived unchanged in `claim-view-results`.
The new checked publication is archived unchanged in `direct-proof-results`.
The earlier low-setting comparison keeps its disclosed sleep pause and deleted-check failure.
Every closed slot is immutable. No recorded attempt was retried.
Only submitted patches and summaries may be published; full CLI records, prompts, drafts and account data stay local.
New studies retain verified copies of their pinned CLI executables so application cleanup cannot remove them.
The workstation guard stopped a whole-repository fr search at 1 GiB aggregate RSS. Its limits stay unchanged;
that search is now a bounded step in the hosted proof workflow. Do not repeat it locally.

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

