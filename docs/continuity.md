# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. This page contains only the
current handoff; completed PR history belongs in Git, the changelog and retained evidence.

## Current work

The [workflow inventory](workflow-routes.md) records the separate purposes and consumers of
guide, intent, task and direct-author routes. Guided writes now validate the retained goal/review
in the SDK and revalidate the guide inside native intent execution. The separate SDK guide read
before writing is removed; the read/preview refresh remains.

The hosted comparison passed 15 successful edits and 23 refusal controls. Guided writes use
three processes instead of four and carry 5,057–5,949 fewer internal bytes in these fixtures.
Exact source, patches and all eight lifecycle stages agree; seven offline audit tests pass.
These are workflow-overhead measurements, not autonomous agent-efficiency results. The full PR
gate is required before merge.

The SDK edit also required fresh full-repository delivery evidence. Hosted run 37978664543 passed
the existing boltons and more-itertools tasks, all 365 independent behavior cases, interruption,
stale-review rejection and patch replay. The catalog and generated roadmap use that report;
the count remains 7/18 demonstrated items and 0/4 completed milestones.

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

Measure body-input preparation and recovery after an applied-state check fails. A guide already
knows its targets, checks and delivery policy, but source-body callers still repeat those fields
when constructing a task operation. Assess a typed builder using those exact retained values,
with agent-authored bodies and explicit ambiguity/staleness refusals. Keep every public route
until evidence supports removing it. The [plan](../PLAN.md#next-large-chunk) tracks this work.

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
