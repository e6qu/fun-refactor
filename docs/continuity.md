# Continue the full-product roadmap

[PLAN.md](../PLAN.md#next-work) owns the roadmap and completion conditions.
M1 and M2 have complete comparisons. M3 remains open: GLM and Kimi still lack
accepted page-count proofs within the original 150 seconds.
Complete each milestone in one PR, including its checks, required attempts and tracking.
[BUGS.md](../BUGS.md) owns defects; [roadmap-status.md](roadmap-status.md) audits earlier claims.

## Starting point

The [proposed six-PR sequence](../PLAN.md#proposed-pr-sequence) covers the full product:
code and call graphs, analysis, refactoring, transformations, proofs, recovery and public interfaces.
The smaller-agent milestones validate this scope; they do not replace it.

The seventh page-count study delivered four accepted proofs within its separate 300-second limit.
PR #619 added product-order facts to `fr proof start`.
Its effect on live proof delivery remains unmeasured.
The [plan's evidence section](../PLAN.md#evidence-behind-the-sequence) links the retained studies.

PR 1 landed in [#621](https://github.com/e6qu/fun-refactor/pull/621). Its [coverage guide](analysis-coverage.md) links features to checks.
The hosted behavior checks, evidence refresh and full CI passed before merge.
PR 2 landed in [#623](https://github.com/e6qu/fun-refactor/pull/623) with passing hosted checks and refreshed evidence.
Its [coverage guide](refactoring-coverage.md) tracks the operation families and signature binding repairs.
PR 3 merged in [#624](https://github.com/e6qu/fun-refactor/pull/624).
Its [coverage guide](proof-coverage.md) tracks proof authoring, bounded Rust connections and the remaining M3 gate.
PR 4 merged in [#625](https://github.com/e6qu/fun-refactor/pull/625).
The [interface guide](interface-coverage.md) covers reviewed browser edits, recovery, graph reports and native handoffs.
PR 5's harness merged in [#628](https://github.com/e6qu/fun-refactor/pull/628) with passing full CI.
The live comparison remains open at 0 of 96 attempts. Earlier disk cleanup restored the required free space;
the workstation is now below its 64 GiB floor again, so checks and collections run on GitHub.
Six Codex non-proof admission checks passed; the proof check exceeded the unchanged 768 MiB
memory ceiling, stopping the sequence with 17 checks unstarted. Preserve the failure and investigate
proof memory on hosted runners before fresh admission. The [memory diagnostic](../tests/agent-eval/product-validation/memory/README.md)
retains both platform results and their audits, including the final Linux memory refusal.
ARM macOS checking alone takes 589–605 MiB, before adding the client and controllers.
The original native admission block remains. Do not raise limits or start live calls.
Independent analysis work merged in [#637](https://github.com/e6qu/fun-refactor/pull/637), adding signed list indices,
insert/pop/reverse and checked helper binding. Full CI and post-merge release checks passed.
[PR #639](https://github.com/e6qu/fun-refactor/pull/639) merged returned list identities and nested aliases with passing full CI,
independent runtime checks,
cross-file cache invalidation and 12 new paired contract examples. Run the
[evidence preflight](evidence-refresh.md) before collecting reports and before full CI.
[PR #641](https://github.com/e6qu/fun-refactor/pull/641) merged bounded slices, shallow copies,
concatenation and deletion with 113 passing PR checks and complete deep validation.
[PR #643](https://github.com/e6qu/fun-refactor/pull/643) retains the
[allocator comparison](../tests/agent-eval/product-validation/memory/2026-11-21/README.md).
It merged after 112 passing checks. Immediate purging did not provide a usable reduction: ARM macOS missed the 10% threshold,
and Linux's candidate exceeded the memory ceiling. Keep the setting out of product defaults.
The original proof, imports, package command, memory metric and limits remain fixed.
Further native work needs a different measured correction and all 24 complete rehearsals.
The next analysis increment follows known list bounds and slice errors through helpers,
handlers and cleanup. It preserves mutations before failure and separates normal from
exceptional list states. Independent Python checks cover source changes and cached answers;
18 paired contract examples expand the corpus to 545 programs and 1,090 cases.
The full-product and live-comparison completion gates remain open.
Its [protocol](../tests/agent-eval/product-validation/README.md) owns the new comparison and the 96-attempt ceiling.
Validation projects stay unopened until the implementation and initial-round decision are frozen.
PR 6's [package qualification](product-qualification.md) merged in [#629](https://github.com/e6qu/fun-refactor/pull/629).
Full CI and all six package jobs passed. Complete-bundle verification merged in
[#631](https://github.com/e6qu/fun-refactor/pull/631) with 109 successful checks. It checks the download set,
receipt digests, consumer outcomes and source/checker revisions before publishing.
The Python contract qualification now requires the acceptance catalog to match the independently audited corpus;
new support must retain original and renamed/moved cases and receive an explicit acceptance-count review.
Keep those artifact results separate from the unfinished comparison and M3 timing gate.
A missing promised feature stays open; marking it unsupported cannot satisfy the completion gate.
Follow PLAN.md for the remaining work packages rather than maintaining a separate backlog here.

The original 150-second failures and later 300-second successes remain separate records.
Do not silently change an acceptance limit or rerun the full proof comparison to seek success.
A new protocol must declare its question, controls, limits and decision before calls.

## Interface decisions

- Use `fr proof list`, `show`, `start`, `state`, `check` and `save`.
  Remove the old proof forms; add no aliases or compatibility fallbacks.
- `start` creates a new `skip` draft and returns the real initial goal.
  `state --at end` resumes after the last nonblank step; explicit branch coordinates also work.
  Each goal separates `context` from `goal`. A closed branch does not accept the whole draft.
- `check` leaves source unchanged and exits 1 for `passed: false`.
  `save` checks and writes in one call, returning the saved edit's `transaction`.
  A rejected save returns the check diagnostics and writes nothing.
  Use `save --deliver` to include the supplied package check and patch export in one call.
  Explicit `history patch` export remains available with the saved IDs in order.
- Proof operations require explicit byte and time limits. Dependency preparation and
  checking share one deadline. The collector enforces its ten-second finishing reserve
  inside each study's declared complete-attempt limit.
- Project queries print labelled text for people and retain `--json` for programs.
  The current directory is the scan root unless the caller supplies `-C`.

The [Lean skill](../skills/fr/references/lean.md) and
[complete lesson](first-lean-proof.md) teach this workflow.
Use fr on its own work where it fits. Full builds, full tests and large collections
run on GitHub under the [workstation rules](development.md#working-on-a-shared-desktop).

## Before another comparison

Use the [improvement method](agent-improvement.md). Freeze the proposed change,
its predicted effect, both conditions and independent checks before calls.
Fresh complete native admission must cover proof work, package checks, patch delivery,
final accounting and the existing resource margin. Prior source-bound receipts cannot
admit changed code. The official OpenCode2 2.2.5 pin fixes missing final usage;
missing or duplicate accounting must still fail the attempt.

The [Euclid review](../tests/agent-eval/proof-algorithms/local-results/review-context.md)
retains eight accepted delivered patches and twelve complete-attempt timeouts.
It establishes no general graph advantage, and list reversal stays reserved.
The [supplied-proof delivery study](../tests/agent-eval/proof-algorithms/local-results/M3-delivery/README.md)
gave correct proofs to every attempt; it measures delivery rather than discovery.
Keep all original outcomes and distinguish accepted patches from complete successes.
Publish reviewed summaries and submitted patches only. Keep transcripts, prompts,
provider exports and credentials local. Unknown charges remain unknown.

## Local resource limits

Use `python3 /Users/zardoz/.codex/tools/fr-local-guard.py COMMAND...` for local fr invocations and checks.
Keep workloads serial, preserve 64 GiB free disk, keep target data below 2 GiB
and sampled aggregate RSS within 1 GiB. Do not raise the guard's 180-second deadline.
Full builds, full gates and large evidence regeneration run on GitHub.
A longer study needs a separately admitted execution environment; local resource refusal stops local work.
