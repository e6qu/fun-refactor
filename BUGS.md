# Bugs and current limits

Use this page when a command gives the wrong answer or cannot finish your task.
Report a small example with the command, settings, expected result, actual output and fr version.
For a write failure, keep the transaction ID and say whether files changed.
Check the [open issues](https://github.com/e6qu/fun-refactor/issues) for an existing report.

## Choose what to do after a failure

| Symptom | What to inspect | Next action |
| --- | --- | --- |
| A rename refuses before writing | The stated refusal reason and [command reference](CLI.md) | Resolve the ambiguity or report a supported input that should work |
| A trace has no reported connection | Completeness, unread code and cutoff reasons in [result meanings](docs/understanding-results.md) | Recheck unsupported behavior before concluding there is no connection |
| A check fails after applying an edit | The saved transaction and failed stage | Follow [repairing an applied change](docs/source-repair.md) |
| A write stops partway through | The pending recovery state | Follow [file recovery](docs/host-recovery.md) before another write |
| A cached answer disagrees with a fresh one | The source files, settings and tool inputs recorded behind it | Keep both answers and report the mismatch |
| A command reaches a work or time limit | The last completed work and declared limit | Identify repeated or unnecessary work; keep the limit |

A transaction is a saved source change. Its returned ID lets you inspect, undo or recover that change.

## B5: Calls supplied from outside the project can remain unknown

A caller might receive a function from another program, load a plugin at runtime, or construct a name from data.
Source files alone may not identify the function that will run.

For example, `run(callback)` calling `callback()` cannot name one target when the project never supplies that callback.
An untyped call such as `shape.area()` can have several possible targets with the same method name.

Fr uses written assignments, imports, annotations and declared type relationships to narrow the possibilities.
Its reports retain uncertainty when those facts do not settle the target.

| Report label | What it means for your task |
| --- | --- |
| `function-value` | Source code assigns a particular callable value to the name |
| `receiver-type` | Source evidence narrows the object before the method call |
| `field-based` | Several functions can occupy the named field; the program still chooses at runtime |
| `method-name` | The report found matching method names without a declared type relationship |

A receiver is the object before the dot in `shape.area()`.
Review external callers, plugins and unread files before using an unused-code result to justify deletion.
The [boundary tests](tests/analysis_boundaries.rs) check both the uncertainty and the report that explains it.
The [grammar tests](tests/known_grammar_gaps.rs) check syntax repairs and nearby supported forms.

## Planned features

The [roadmap](PLAN.md#proposed-pr-sequence) covers graphs, analysis, refactoring, proofs and the public interfaces.
The combined graph-and-analysis PR includes invalid-number conversion and shared-list analysis.
Their unfinished earlier claims remain in the [qualification checklist](docs/earlier-roadmap-checklist.md).
Keep current task requirements and completion checks in PLAN.md.

## Closed defect history

Recent closed repairs remain in [the November 12 bug history](https://github.com/e6qu/fun-refactor/blob/7666442e9/BUGS.md).
The [pre-cleanup bug history](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/BUGS.md#fixed)
retains the closed entries and original repair notes.
Use [CHANGELOG.md](CHANGELOG.md) for release history and Git history for the associated changes.

When a defect closes, keep its reproducer as a behavior test and record the repair in its PR.
Keep this page focused on problems a contributor can act on now.

