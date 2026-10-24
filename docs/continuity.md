# Continue work without repeating the investigation

[PLAN.md](../PLAN.md#next-work) owns the upcoming tasks.
The [status report](roadmap-status.md) owns demonstrated checks.
[BUGS.md](../BUGS.md) owns reported defects.
Use this page for the starting point and the checks to preserve.

## Start here

PR #577 added ordered handlers for selected explicit Python errors.
Its [worked example](catch-python-errors.md) repairs an application that catches `TypeError` when Boltons raises `ValueError`.
The [cleanup lesson](check-python-cleanup.md) shows why a return inside `finally` can replace a customer's message.

The next code package is [R1: recover from invalid number input](../PLAN.md#r1-recover-from-invalid-number-input).
Extend the same authored application through the original Boltons import.
Commit its requirements and Python behavior checks before changing the analyzer.
Do not present this familiar example as a new unread-project evaluation.

Follow [R2](../PLAN.md#r2-follow-shared-lists-through-original-imports) for list aliases and the remaining full-package questions.
An alias is another name for the same object.
Changing a shared list must affect later reads through either name; changing a separate list must preserve the original.

## Preserve these working behaviors

- Selected Rust and Python changes pass independent checks, undo, redo and patch replay in another checkout.
- Saved answers agree with fresh analysis after relevant edits; unrelated findings remain usable.
- The Boltons, dateutil, JDcal and Stringcase questions use original imports.
- Explicit supported errors follow matching handlers, helper calls, nested cleanup and known reraises.
- Claude Code, Codex and OpenCode discover the portable skill and pass scripted change exercises.
- The bounded [page-count translation](check-language-behavior.md) keeps integer division exact in both directions.

An empty trace from an incomplete analysis cannot establish that input never reaches an output.
A finished simple-value analysis also leaves unsupported runtime operations unchecked.
The [Python guide](python-contract.md) states the supported rules and the remaining limits.

## Where current records live

| Record | How to find it | What to preserve |
| --- | --- | --- |
| Current analysis reports | [Explicit report selection](../tests/agent-eval/current-scalar-reports.json) | Named paths and original file fingerprints; no automatic older selection |
| Requirement status | [Acceptance catalog](../tests/agent-eval/roadmap.json) | Existing acceptance conditions and engineering counts |
| First independent project changes | [Project-change milestone](project-change-milestone.md) | Humanize's first evaluation and all failed development attempts |
| First original-package questions | [Package milestones](package-milestones.md) | First unread evaluations and their original auditors |
| Current whole-session costs | [CLI and Python comparison](project-session-cost.md) | Complete matching answers, setup boundaries and measured overhead |
| Closed study attempts | [Candidate review](candidate-review-status.md) | Failed, cancelled and unstarted work |
| Dated investigations | [Evaluation history](evaluations.md) | Original inputs, outputs, revisions and limits |

A file fingerprint is a value computed from its contents.
It detects an edit; it does not establish that an answer is correct.
Current report selection names each file explicitly. A missing selection must fail.

The [Python example receipt](../tests/agent-eval/results/2026-10-24-handlers-native-python-contract/manifest.json)
records 495 programs and 990 original or renamed cases.
It includes 370 incomplete cases, 18 refusals and 134 documented differences between the supported model and Python execution.
These counts describe the checked examples, not complete Python support.
Refresh this link and description when a new collection replaces it.

## Refresh after changing code or instructions

1. Integrate current `main` before freezing the collection source.
   A release can change the Python version file, which some reports record as an input.
2. Finish implementation, examples and source-bound documentation before collecting.
   Changing a guide or skill can change a report's input fingerprint even when runtime behavior stays the same.
3. Use the hosted collection groups in the [CI guide](ci.md).
   Refresh the affected records and preserve their originals. Never rewrite an old recorded hash to bless new inputs.
4. Include the separate affected-code, project-change, package, session-cost and saved-answer checks when their recorded inputs change.
   The general analysis group does not own all of them.
5. Combine successful records, then run the hosted documentation review and roadmap generation on the combined branch.
6. Run the full PR gate on the final commit. Fix every failure, including unrelated failures, before marking the PR ready.

The [developer guide](development.md#refresh-retained-evidence) explains collection and retention.
The original historical report uses its original auditor; current readers accept the current format.
An old qualification does not turn a later familiar-project replay into another first attempt.

## Agent comparisons and proofs

The [eighteen-attempt preparation](agent-comparison-preparation.md) is scripted and makes no live model calls.
The [comparison plan](product-review.md#first-comparison-with-codegraph) still needs agreed models, settings, prices and spending limits.
The separate [repeated-task study](product-review.md#repeated-task-study) has a larger completion requirement.

Keep the closed client investigation and stopped packaging study stopped.
This handoff does not authorize new paid attempts.
[R3](../PLAN.md#r3-prove-the-properties-of-the-code-we-ship) specifies the actual-code proof tasks; existing model theorems do not complete them.

## Before running or publishing work

Follow the [shared-workstation rules](development.md#working-on-a-shared-desktop).
Builds, full tests and large collections run on GitHub.
If the local guard refuses, move the work to a runner. Keep its limits.

Every commit message is one line of at most 80 characters, with no body, trailers or AI attribution.
Wait for passing CI before merging. When authorized, squash with an explicit subject and empty body.

The [pre-cleanup handoff](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/continuity.md)
keeps the older investigation narrative. Current instructions and priorities live in the pages linked above.

