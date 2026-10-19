# Finish a package question, then test an unfamiliar project

Start with the [number and date examples](original-package-questions.md) for
the user questions, the meaning of a finished answer and its assumptions.

Milestones 1 and 2 belong in one pull request. The pull request is not ready just
because its regression tests pass. It must retain results demonstrating the user
outcomes below with one frozen implementation.

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

In addition to the existing jaraco.functools reservation, reserve fn.py at
`df53f5d6bf0e94a37f44f6be57d1c87c9b7a6c26`. Only its repository and commit
metadata have been read. Keep both reserved implementations unopened until the
analyzer freezes. For their helper question, inspect for plain top-level
functions returning an argument directly; preserve an unsuccessful inspection
if there is no such function. Do not manufacture one or copy it out of its
package. Preserve every planned project result, including any that do not
qualify for the three-project milestone.

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

## Milestone 2: work beyond a familiar package

With the same frozen implementation, demonstrate the reaching-input and
blocked-input questions across at least three independently maintained projects.
At least one implementation must remain unread until the implementation and
question-selection rule are frozen. Choose projects before using their source
to guide changes. Keep the familiar Toolz, Funcy and Pydash failures visible.

The existing reservation is
[jaraco.functools](../tests/agent-eval/python-repository-flow/replacement-holdout.json).
Its source remains unopened. Inspect its license and requirements on admission;
do not assume that a desired helper exists. Record the first admission,
inspection, runtime and analysis outcomes, including failures.

A failure can inform another fix in this same pull request, but that project
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
