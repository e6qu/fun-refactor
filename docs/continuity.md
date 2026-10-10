# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. Keep completed PR history
in Git and retained reports; this page describes the current handoff.

## Current result

The [Rust/Python explanation profile](compiler-profile.md) covers eleven declared features and
eighteen original or renamed/relocated cases, together with compiler and checked-origin evidence.
Its 26 corruption tests reject missing coverage, invented facts and mismatched inputs.

Profile review caught a wrong-body selection for duplicate Python functions. Semantic queries
and edit preparation now require unique source and lowered declarations, including containing
classes. Decorated duplicates refuse; methods in distinct classes remain available. File-level
inspection and bounded source remain the fallback when a declaration cannot be matched.

Current source-bound repository, indexing and workflow evidence was collected again after that
fix. Historical reports stay unchanged. The [source-repair](source-repair.md) and
[resume](workflow-recovery.md) comparisons remain finite workflow evidence, without live models.

Roadmap status is 8/18 demonstrated acceptance items and 0/4 completed milestones. This does
not establish a general agent-efficiency advantage or complete Rust/Python semantics.

## Next work

Implement [B.contract](roadmap-status.md#bcontract-specify-reusable-python-behavior): explicit
language rules for evaluation order, assignments, calls, exceptions and source positions.
Compare supported rules with independently authored executable examples and renamed/relocated
variants. Preserve unsupported cases; fix defects through general rules, never repository names.
The [plan](../PLAN.md#next-large-chunk) defines the next deliverable.

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
