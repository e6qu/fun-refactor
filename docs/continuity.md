# Continue the full-product roadmap

[PLAN.md](../PLAN.md#next-work) owns the roadmap and completion conditions.
M1 and M2 have complete comparisons. M3 remains open: GLM and Kimi still lack
accepted page-count proofs within the original 150 seconds.
Complete each milestone in one PR, including its checks, required attempts and tracking.
[BUGS.md](../BUGS.md) owns defects; [roadmap-status.md](roadmap-status.md) audits earlier claims.

## Starting point

PR #647 merged with passing CI, release and deep validation.
[PR #649](https://github.com/e6qu/fun-refactor/pull/649) merged the combined change:
Python signature refactoring, dictionary flow and variadic calls, with imported callers,
independent runtime checks, safe refusal, checked delivery and replay. Focused refactoring
and graph checks passed. Signature regressions cover Unicode binding safety and method receivers.
SDK regressions compare typed variadic flow with Python execution, including empty argument collections.
Python qualification passed 585 programs, 1,170 paired cases
and 2,455 selected SDK checks, retaining 352 incomplete cases and 18 refusals.
Current evidence refresh, independent audits and PR checks passed before merge. Release 0.105.0 followed.

The current Python increment covers dictionary unions, in-place unions and construction or updates from bounded pairs.
It preserves shared nested objects, argument side effects and writes completed before an error.
Unknown keys, custom iterators and indexed augmented targets remain incomplete.
The focused gate passed 1,385 Python tests; broader qualification and evidence refresh are in progress.
The separate [memory investigation](https://github.com/e6qu/fun-refactor/pull/651) tested Lean 4.36.0 against the pinned checker.
It used more memory on both platforms. Keep the pin and native admission block.
Both measured candidates have failed; review import-memory attribution before testing another correction.

The [six-PR plan](../PLAN.md#proposed-pr-sequence) covers the full product, including
code and call graphs, analysis, refactoring, transformations, proofs, recovery and public
interfaces. Its coverage guides track what is implemented and checked:

- [Analysis and graphs](analysis-coverage.md), including the list and sequence increments.
- [Refactoring](refactoring-coverage.md), including signature binding and coordinated edits.
- [Proof authoring](proof-coverage.md), including bounded Rust implementation connections.
- [Interfaces and recovery](interface-coverage.md), including browser review and native handoffs.
- [Package qualification](product-qualification.md), including source-bound complete-bundle verification.

A missing promised feature stays open; marking it unsupported cannot complete the product.
Follow PLAN.md for remaining work rather than maintaining a second backlog here.
Run the [evidence preflight](evidence-refresh.md) before collecting reports and before full CI.

## Remaining comparison gates

M3's original 150-second gate remains open. The seventh page-count study delivered four
accepted proofs within a separate 300-second limit; those results do not close the original
gate. PR #619 added product-order facts to `fr proof start`, but its live effect remains
unmeasured. The [plan's evidence section](../PLAN.md#evidence-behind-the-sequence) links the studies.
Do not change acceptance limits or rerun a full comparison to seek success.

PR 5's [comparison protocol](../tests/agent-eval/product-validation/README.md) authorizes
at most 96 attempts. None has run. Validation projects stay unopened until the implementation
and initial-round decision are frozen. Native admission remains blocked:

- Six Codex non-proof checks passed. The proof check exceeded the unchanged 768 MiB memory
  ceiling, stopping the sequence with 17 checks unstarted.
- The [memory diagnostic](../tests/agent-eval/product-validation/memory/README.md) found
  ARM macOS checking alone uses 589–605 MiB, before the client and controllers. Linux
  checking nearly fills the ceiling without a client.
- The [allocator comparison](../tests/agent-eval/product-validation/memory/2026-11-21/README.md)
  found no usable reduction. ARM macOS missed the frozen 10% threshold; Linux's candidate
  exceeded the ceiling. Keep the setting out of product defaults.

Further native work needs a different measured correction, followed by all 24 complete
rehearsals under the original proof, imports, package command, memory metric and limits.
Do not start live calls or raise those limits. The workstation is again below its 64 GiB
free-space floor, so execution and evidence collection remain on GitHub.

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
