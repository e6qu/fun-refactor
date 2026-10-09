# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. This page contains only the
current handoff; completed PR history belongs in Git, the changelog and retained evidence.

## Current work

The [body-input comparison](workflow-routes.md#preparing-body-edits-and-recovering-failed-checks)
passed all 40 cells in hosted run 37990418031. `AgentGuide.source_body_action` reuses exact retained
targets, checks and delivery for caller-authored bodies. Complete caller/submission size falls by
595 bytes in each of four fixtures; both arms retain three processes and the same internal bytes.
All successful patches and lifecycle stages agree. Failed applied checks retain changed source
without delivery; reopened undo restores checked source and refuses later conflicting edits.

Eight body-report integrity tests, seven refreshed route-report tests and three native boundary
tests cover the result. The existing 38-cell route and scalar comparisons were refreshed against
the new SDK source. These are prescribed workflow measurements, not autonomous agent-efficiency
results. The full PR gate is required before merge.

Hosted run 37990846166 refreshed both existing Python repository tasks after the SDK change.
All 365 behavior cases, interruption, stale-review rejection and patch replay passed. The catalog
selects that report; roadmap status remains 7/18 demonstrated items and 0/4 completed milestones.

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

Measure repair-and-resume versus undo-and-retry through checked patch delivery after applied-state
failure. Current evidence stops at restored source. Also review current caller policies before
adopting the helper: the multi-body runner requests a larger diff and different postconditions.
Preserve those semantics and frozen historical snapshots. The [plan](../PLAN.md#next-large-chunk)
tracks exact delivery, check-receipt and receiver-replay requirements.

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
