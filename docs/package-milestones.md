# Finish a package question, then test an unfamiliar project

Start with the [number and date examples](original-package-questions.md) for
the user questions, the meaning of a finished answer and its assumptions.

Milestone 1 was demonstrated in [PR #545](https://github.com/e6qu/fun-refactor/pull/545).
That PR merged before milestone 2 was complete. The follow-up comparison now passes
for Boltons, dateutil and JDcal with one unchanged analyzer. This completes both
near-term package milestones. The original run records below support that conclusion;
regression tests alone would not establish either outcome.

These package milestones remain complete. The [project-change milestone](project-change-milestone.md)
now also has passing original records for saved answers after edits and complete Rust/Python rename delivery.
The [saved-work comparison](saved-work-cost-results.md) measures continued work against fresh questions on the same files.
The [matched follow-up](saved-answer-improvement.md) checks the fix for repeated hashing while saving and reopening answers.
The [next work](../PLAN.md#next-three-prs) combines agent setup, measured costs and a fair CodeGraph comparison in the first PR.

Here, a freeze means recording the code used for the check, then keeping it
unchanged while examining unfamiliar projects.

The question is concrete: after importing a real package, can an input passed to
one of its helpers reach a selected output? For example, if a helper adds one,
`record_value(helper(emit_value()))` carries the input to the output.
Replacing the intermediate value with zero must remove that connection.
Here `emit_value` supplies the tracked input and `record_value` is the output
selected by the user. Neither name gives permission to ignore package startup.

## Analysis choice

Keep the answer based on source. Use ordinary Python execution as an independent
check, without treating one observed startup as proof about other executions.
Supported standard-library operations need explicit models selected for the
question. Record those assumptions and all competing local import candidates;
a same-named local module must not inherit a standard-library model.

For broader development coverage, pin Boltons at
`55dfe5077bcb5fa76611b5f2a557ea6c442ad87c` and python-dateutil at
`2642afacc33fb839c404b75b230fff58b79793f2` before changing the analyzer.
The questions concern limiting an integer to specified bounds and computing an
Easter date from its year. These exercise arithmetic and immutable date values,
respectively. Boltons is familiar coverage, including prior delivery tests.
Neither project is a reserved unread evaluation.

The first unfamiliar-project comparison reserved jaraco.functools and fn.py,
the latter at `df53f5d6bf0e94a37f44f6be57d1c87c9b7a6c26`. Both stayed
unread until the analyzer and question-selection rule were recorded. That rule
looks for plain top-level functions returning their only argument directly,
such as `def identity(value): return value`. It excludes `@decorator` markers
and type labels such as `value: int` or `-> int`. An unsuccessful inspection
remains a result; do not manufacture
a helper or copy one out of its package. Keep every planned project outcome,
including those that do not qualify for the three-project milestone.

## Milestone 1: complete the original package question

Use an original, pinned package snapshot and its original import. Keep its
startup code, helper declarations and package files unchanged. Copied helper
files and manually simplified packages remain diagnostic examples.

For at least one helper, retain all of these results:

- A complete answer showing that the input can reach the selected output.
- A complete answer showing that overwriting the value blocks that connection.
- Ordinary Python executions agreeing with the independently stated outcomes.
- The same answers after renaming the calling function and moving its file.
- An unfinished answer for an unknown call and for an exhausted search budget.
- Saved and fresh answers agreeing before and after a relevant source edit.
- An import change that replaces the helper cannot reuse the previous answer.

The result must state its assumptions. An unfinished answer with no reported
connection is not a complete answer establishing that the input is blocked.
Keep existing resource limits. If checking is split into smaller steps, retain
the checked connection and dependencies between them.

The [first development attempt](../tests/agent-eval/package-questions/first-development/receipt.json)
answered both original package questions and their renamed callers correctly.
Dateutil passed all saved-answer controls. Boltons tried all six saved-answer
checks in one Python process and reached its existing ten-second CPU limit.
Stopping that process prevented its cleanup from restoring a temporary helper
edit. Keep that failure. Each check now runs in a separate process with the same
limits and passes its saved answer and supporting files to the next check. The
program that starts these checks restores the original source even if a check
process stops. The record retains every check and its resource use; splitting
the work does not establish lower total cost.

[Frozen familiar-project coverage](https://github.com/e6qu/fun-refactor/actions/runs/39296005764)
passed all of these checks for both Boltons and dateutil. The complete original
records are retained with their
[source and artifact identities](../tests/agent-eval/results/2026-10-19-original-package-questions-package-questions-coverage/receipt.json).

## Milestone 2: work beyond a familiar package

With the same frozen implementation, demonstrate the reaching-input and
blocked-input questions across at least three independently maintained projects.
At least one implementation must remain unread until the implementation and
question-selection rule are frozen. Choose projects before using their source
to guide changes. Keep the familiar Toolz, Funcy and Pydash failures visible.

The [first unfamiliar-project run](https://github.com/e6qu/fun-refactor/actions/runs/39304111929)
used the same recorded analyzer as the successful familiar-project coverage.
Jaraco.functools stopped at admission: its pinned archive has no license file
required by that check. fn.py passed admission, but inspection found no helper
matching the selection rule. Neither question reached analysis or ordinary
Python execution. Both original outcomes and inputs remain in the
[first-comparison record](../tests/agent-eval/results/2026-10-19-original-package-questions-package-questions-held-out/receipt.json).

That first comparison did not demonstrate milestone 2. It supplied no qualifying third project.
The [jaraco.functools reservation](../tests/agent-eval/python-repository-flow/replacement-holdout.json)
records its original selection; it must not be read as a successful result.

The next comparison selected [JDcal's leap-year question](leap-year-package-question.md)
from its public README before opening its implementation or tests.
[Fresh familiar coverage](https://github.com/e6qu/fun-refactor/actions/runs/39382493915)
passed before the [first JDcal evaluation](https://github.com/e6qu/fun-refactor/actions/runs/39388270081)
at the exact same commit. Both use the recorded analyzer and question plan.

JDcal finished the original and renamed or moved callers: the year can reach the answer,
while overwriting it with 2000 blocks the original value route.
Ordinary Python returned `True` for 2000 and `False` for 2100, as declared before inspection.
Overwriting the year produced `True` in both examples. Unknown calls and exhausted search budgets stayed unfinished.
All original package files stayed unchanged, and no worker reached a resource limit.

The [familiar-package receipt](../tests/agent-eval/results/2026-10-20-leap-year-package-questions-coverage/receipt.json)
and [JDcal receipt](../tests/agent-eval/results/2026-10-20-leap-year-package-questions-held-out/receipt.json)
retain the complete original archives, commands, observations and source identities.
Together they demonstrate milestone 2 across three independently maintained projects.
JDcal is now familiar; it cannot serve as another unread test.

A failure can inform another fix, but that project
then becomes a development example. Reserve another unread project before the
next evaluation. A renamed adapter or a new revision of a familiar package does
not supply an independent project.

## What the pull request must retain

Keep source identities, the implementation freeze, exact commands, ordinary
Python outcomes, analysis answers and resource stops together. Identify which
questions finished and which did not. Complete these outcomes before claiming
either milestone is done; do not infer completion from the number of tests.

Run full builds, complete test gates and large result collection on GitHub.
Fix CI failures and timeout causes without increasing deadlines, dropping checks
or adding compatibility paths. The final pull request description should start
with a complete user example, followed by the demonstrated scope and limits.

These milestones do not establish lower total agent cost, general Python
support, proof of source behavior or permission to restart the stopped live
studies. Those remain separate outcomes in the [project plan](../PLAN.md).
