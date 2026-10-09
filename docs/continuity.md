# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. This page contains only the
current handoff; completed PR history belongs in Git, the changelog and retained evidence.

## Current work

[PR #459](https://github.com/e6qu/fun-refactor/pull/459) implements the committed delivery diagnostic.
The single [collection run 37952719938](https://github.com/e6qu/fun-refactor/actions/runs/37952719938)
uses commit `8f00d13f203ad9a1274cfa2fb98f9d4f8e359d9c`; results and the decision are pending.
Do not dispatch another collection. Retain the original artifacts and exact runner sources,
then replay them on GitHub before completing this PR.

The [earlier streaming comparison](../tests/agent-eval/opencode/changes/streaming-2026-10-09/README.md)
failed admission on macOS, with OpenCode dominating sampled CPU. Short controls alone cannot
admit local client work. Routine PR checks replay frozen evidence without launching clients.

The earlier [packaging pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md)
remains permanently stopped. No new workstation capture or live model call is admitted.

## Next work

Use finer Linux CPU sampling and matched pacing to isolate client streaming overhead on GitHub.
The [committed delivery design](../tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md)
sets sixteen independent cells and at most 32 captures, with no retries or adaptive extension.
Test only a supported configuration or targeted fix against an observed cause, under unchanged
limits and a new committed design. Any later live comparison needs fresh admission,
actual fr-use accounting and complete costs; it cannot resume a stopped collection.

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
