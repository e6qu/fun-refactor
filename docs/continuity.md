# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. Keep completed PR history
in Git and retained reports; this page describes the current handoff.

## Current result

The [Python behavior contract](python-contract.md) compares 36 independent programs and their
renamed/relocated equivalents: 72 cases across twelve declared rule families. It retains CPython
outcomes separately from possible-flow answers, including unsupported and conservative results.
Corruption controls check missing cases, altered outcomes, invented authority and source locations.

The review caught comments being traversed as operands and reported as unsupported expressions.
The general traversal now skips parser extras. Focused regressions cover three comment placements
and compare runtime effects and exact witness locations. Full builds and evidence refreshes run
on GitHub; scalar refreshes are split into four jobs with 15-minute limits.

Roadmap status is 8/18 demonstrated acceptance items and 0/4 completed milestones. This does
not establish a general agent-efficiency advantage or complete Rust/Python semantics.

## Next work

Finish [B.contract](roadmap-status.md#bcontract-specify-reusable-python-behavior). Map recursive
summaries, static imports and every admitted call-binding form to the explicit rule inventory.
Existing focused tests cover these separately; the current contract report does not claim them.
Add independent executable examples and renamed/relocated variants before closing the gate.
The [plan](../PLAN.md#next-large-chunk) defines the deliverable and remaining product gaps.

## Client-study boundary

The [delivery investigation](../tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md)
is closed. It found no justified client configuration fix. The
[packaging pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md) remains
permanently stopped; preserve its failed and unstarted cells. No local live client call is admitted.

Streaming CI replays frozen evidence. Separate path-selected transport controls use hosted clients
with scripted replies. Their success does not establish workstation or representative-task admission.
A later live study needs fresh resource admission, actual fr-use accounting and complete agent costs.
The [candidate review table](candidate-review-status.md) owns task admission and review gaps.

## Before editing or merging

- Follow the [development guide](development.md#working-on-a-shared-desktop): guarded, serial local
  work; full builds, candidate execution and evidence regeneration on GitHub.
- Use `fr` for supported inspection and reviewed edits. Treat incorrect or impractical behavior
  as product evidence; use direct edits where no suitable operation exists.
- Use configured OpenCode access and the existing `gh` login. Do not inspect or copy credentials.
- Keep every commit message to one line of at most 80 characters, without a body or trailers.
  When authorized, wait for passing CI and squash-merge with an explicit subject and empty body.
- Retain failed and unstarted evaluation cells. Do not promote partial output to a submission,
  rewrite old results, raise limits or retry a stopped collection.

Use `fr audit` and `fr capabilities` for current support boundaries. Parsing does not establish
arbitrary behavior; model theorems do not prove the host filesystem or unrelated runtime effects.
