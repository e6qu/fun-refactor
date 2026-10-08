# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. This page contains only the
current handoff; completed PR history belongs in Git, the changelog and retained evidence.

## Current work

[PR #456](https://github.com/e6qu/fun-refactor/pull/456) adds exact-submission grading,
failure-preserving reports and frozen Python runners for future replay. It also retains the
[configured packaging pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md).

That collection is permanently stopped: Kimi's first ordinary-tool attempt hit the 20-CPU-second
limit before submitting, and three cells remain unstarted. No partial edit was graded. Hosted
replay verifies every outcome. The report records admission, executable and source identities,
resource measurements, failed work and unknown usage; do not duplicate those records here.

## Next work

Run representative scripted streaming and editing workloads on GitHub to attribute CPU use.
The live trace has 851 streamed text deltas, while the passing workstation control has none.
This difference is a hypothesis to test. Keep resource limits unchanged and do not resume the
stopped collection. Any later live comparison needs a new design and fresh admission.

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
