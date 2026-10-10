# Contributor handoff

Read [PLAN.md](../PLAN.md) for the objective, next chunk and remaining milestones.
The [generated status](roadmap-status.md) owns acceptance counts. Keep completed PR history
in Git and retained reports; this page describes the current handoff.

## Current result

The [source-repair comparison](source-repair.md) passed 40 cells and 16 corruption tests in
hosted run 38055066754. Both routes start with an incorrect body and failed applied check.
They preserve required checks and reverse the complete delivery chain. They check original
and restored final source, then replay the patch in an independent receiver.

Undo-and-correct used 17 total fr calls versus 20 for repairing applied source. Protocol bytes
were lower for undo-and-correct in all four Python/Rust fixtures. Both totals include initial
failed work. Existing ordered transaction patches sufficed; no new public operation was added.
Those patches retain intermediate incorrect source. The caller sequence is not atomic.

The [body-caller review](source-repair.md#body-helper-adoption-review) preserves custom diff
limits, postconditions and acceptance checks. Historical runner snapshots remain unchanged.
The earlier [applied-state resume](workflow-recovery.md) remains for unchanged edited source
after repairing an external check prerequisite. It cannot repair source within the old transaction.

These are prescribed workflows without a live model or agent-efficiency result. Roadmap status
remains 7/18 demonstrated items and 0/4 completed milestones. Existing repository, host recovery
and proof reports remain regression evidence with their stated limits.

## Next work

Build the consolidated Rust/Python explanation profile described in
[A.compiler-profile](roadmap-status.md#acompiler-profile-relate-explanations-to-compiler-and-runtime-checks).
Inventory existing compiler and source-location tests before adding machinery. Retain versions,
build inputs, repeated calls, shadowing, Unicode positions, disagreements and unsupported cases.
Explain which facts come from syntax, references, analysis, compiler checks or runtime execution.
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
