# Continue the focused agent work

[PLAN.md](../PLAN.md#next-work) owns the roadmap and completion conditions.
M1 and M2 have complete comparisons. M3 remains open: GLM and Kimi still lack
accepted page-count proofs within the original 150 seconds.
Complete each milestone in one PR, including its checks, required attempts and tracking.
[BUGS.md](../BUGS.md) owns defects; [roadmap-status.md](roadmap-status.md) audits earlier claims.

## Current branch

PR [#601](https://github.com/e6qu/fun-refactor/pull/601) is merged.
Its [final CI gate](https://github.com/e6qu/fun-refactor/actions/runs/41919641540)
passed all 92 jobs; companion workflows also passed.
The [argument-repair comparison](../tests/agent-eval/proof-argument-repair/README.md)
retains all eight new attempts, each run once. All timed out without a patch.
One Kimi draft passed checking near 135 seconds but was not saved or delivered.
No mapped argument-type error was observed. The proposed repair instruction has no
measured benefit. Earlier interrupted and unstarted slots remain separate.

PR [#603](https://github.com/e6qu/fun-refactor/pull/603), branch
`improve/clear-proof-and-cli-workflows`, improves the commands and proof workflow.
The [expanded command checks](https://github.com/e6qu/fun-refactor/actions/runs/42038465557)
pass the complete skill exercise, proof lesson, JSON refusals, stalled-process controls and Lean reader tests.
The [reader generator](https://github.com/e6qu/fun-refactor/actions/runs/42036519082)
passes the syntax fixtures and affected project files.
Finish collecting and auditing fresh measurements, then pass the full CI gate before review.
There have been no new model calls for this change.

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
