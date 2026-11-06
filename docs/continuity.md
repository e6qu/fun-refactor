# Continue the focused agent work

[PLAN.md](../PLAN.md#next-work) owns the roadmap and completion conditions.
M1 and M2 have complete comparisons. M3 remains open: GLM and Kimi still lack
accepted page-count proofs within the original 150 seconds.
Complete each milestone in one PR, including its checks, required attempts and tracking.
[BUGS.md](../BUGS.md) owns defects; [roadmap-status.md](roadmap-status.md) audits earlier claims.

## Current branch

PRs [#601](https://github.com/e6qu/fun-refactor/pull/601) and
[#603](https://github.com/e6qu/fun-refactor/pull/603) are merged.
The [#603 CI gate](https://github.com/e6qu/fun-refactor/actions/runs/42085550950)
passed all 92 jobs; all companion workflows passed too.
The [argument-repair comparison](../tests/agent-eval/proof-argument-repair/README.md)
retains all eight new attempts, each run once. All timed out without a patch.
One Kimi draft passed checking near 135 seconds but was not saved or delivered.
No mapped argument-type error was observed. The proposed repair instruction has no
measured benefit. Earlier interrupted and unstarted slots remain separate.

PR [#604](https://github.com/e6qu/fun-refactor/pull/604) uses
`improve/check-division-fact-guidance` and remains draft.
Refresh the [division-fact diagnostic](../tests/agent-eval/proof-division-facts/README.md),
which has no started attempts or frozen execution plan. The question is whether the
agents can find suitable division declarations and apply their current types.
This tests a different obstacle from the unobserved argument-repair prediction.
The old qualification cannot admit the changed CLI, client or monitor.
Require fresh hosted controls and all four complete local client checks before freezing calls.
Keep the full 150-second limit, ten-second collection reserve and independent resource observer.
The roadmap lists the eight-slot comparison and its fixed decisions. Keep this work in one PR.
Publish only reviewed summaries and submitted patches; leave raw native records private.

The current hosted proof controls pass. They now cover protected skills, complete
usage and full resource records. Fr's source identity excludes generated `.fr`
and `.fr-history` directories, including history created under `src`.
Fresh reports preserve the earlier records and their original source identities.
The [release replay](https://github.com/e6qu/fun-refactor/actions/runs/42137806778)
passes all 21 changes and both renames with `0.89.2` and the corrected input policy.
Release planning now requests this replay and full CI before merging a release.

Keep the refreshed records in this same PR. Require complete CI and fresh hosted
qualification on its final source. Next pass all four local
scripted client checks and freeze the eight slots. No new model call has started.
Automatic approval review requires approval of the exact provider payload before
these live calls. Complete preparation and show that payload before requesting approval.

## Interface decisions

- Use `fr proof list`, `show`, `start`, `state`, `check` and `save`.
  Remove the old proof forms; add no aliases or compatibility fallbacks.
- `start` creates a new `skip` draft and returns the real initial goal.
  `state --at end` resumes after the last nonblank step; explicit branch coordinates also work.
  Each goal separates `context` from `goal`. A closed branch does not accept the whole draft.
- `check` leaves source unchanged and exits 1 for `passed: false`.
  `save` checks and writes in one call, returning the saved edit's `transaction`.
  Check the owning Lean package, then use `history patch` with the saved IDs in order.
- Proof operations require explicit byte and time limits. Dependency preparation and
  checking share one deadline. The collector enforces its ten-second finishing reserve
  inside the original 150-second attempt, rather than giving the client the entire budget.
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
