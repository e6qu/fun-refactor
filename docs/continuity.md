# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. This page contains only the
current handoff; completed PR history belongs in Git, the changelog and retained evidence.

## Current work

The [recovery comparison](workflow-recovery.md#retained-results) passed 32 cells in hosted run
38043857792. Explicit `resume-applied` workflows recheck an unchanged applied transaction and
finish delivery without another initial apply. Both resume and undo/retry deliver identical
patches that pass independent receiver replay, behavior and conflict checks.

Resume uses seven total fr calls versus nine for undo/retry, including the initial failed attempt.
Twelve offline corruption tests bind caller accounting, requests, required check receipts and
receiver outcomes. Native tests cover missing/stale reviews, changed source/check declarations,
repeated check failures and wrong history states. The run also refreshed the earlier 40-cell
body, 38-cell route and scalar comparisons. These are prescribed workflows, without a live model.

Hosted run 38043863822 refreshed both Python repository tasks against the native change.
All 365 behavior cases, interruption and patch replay passed. The catalog selects that report;
roadmap status remains 7/18 demonstrated items and 0/4 completed milestones.
Host-recovery run 38044199608 also refreshed the CLI binding: 430 handled-failure boundaries,
430 process-exit boundaries and 224 model cases passed within the 15-minute hosted job limit.

[PR #459](https://github.com/e6qu/fun-refactor/pull/459) closed the bounded client-delivery
investigation. Its [retained results](../tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md)
show inconsistent CPU differences; no targeted client fix is justified. Do not dispatch another
collection.

The [earlier streaming comparison](../tests/agent-eval/opencode/changes/streaming-2026-10-09/README.md)
failed admission on macOS, with OpenCode dominating sampled CPU. Short controls alone cannot
admit local client work. Routine PR checks replay frozen evidence without launching clients.

The earlier [packaging pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md)
remains permanently stopped. No new workstation capture or live model call is admitted.

## Next work

Test a genuinely wrong body and a corrected edit. Current recovery repairs an external check
prerequisite; source changes invalidate resume of the old transaction. Compare a new repair on
applied source with undo-and-correct, including an exact original-to-final receiver patch, before
adding a combined-delivery API. Also review caller policies before adopting the body helper:
the multi-body runner requests a larger diff and different postconditions. Preserve those
semantics and frozen snapshots. The [plan](../PLAN.md#next-large-chunk) tracks these boundaries.

Any later live comparison needs fresh resource admission, actual fr use and complete costs;
it cannot resume a stopped collection.

The [candidate review table](candidate-review-status.md) owns task admission and review gaps.
The [product review](product-review.md) owns evaluation and removal criteria. There is no general
efficiency advantage established, and all four product milestones remain open.

## Before editing or merging

- Follow the [development guide](development.md#working-on-a-shared-desktop): guarded, serial local
  work; full builds, candidate execution and evidence regeneration on GitHub.
- Use `fr` for supported inspection and reviewed edits. Treat incorrect or impractical behavior
  as product evidence; direct edits are appropriate where no suitable operation exists.
- Use configured OpenCode access and the existing `gh` login. Do not inspect or copy credentials.
- Keep every commit message to one line of at most 80 characters, with no body, trailers or
  attribution lines. When authorized, wait for passing CI and squash-merge with an explicit
  subject and empty body.
- Retain failed and unstarted evaluation cells. Do not promote partial output to a submission,
  rewrite old results, raise limits or retry a stopped collection.

Use `fr audit` and `fr capabilities` for live support boundaries. Parsing does not establish
arbitrary behavior; model theorems do not prove the host filesystem or unrelated runtime effects.
