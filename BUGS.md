# Reported bugs and current limits

Use this page when fr refuses a command, gives a wrong answer, or cannot finish your task.
The [roadmap](PLAN.md#next-work) lists planned features. A missing feature and an incorrect answer need different reports.

## Repairs under review

An agent's page-count draft exposed another valid Lean form that fr rejected before checking it.
`rintro ⟨ha, -, -, -⟩` introduces several hypotheses and discards the unused ones.
Pinned Lean accepts this form; fr treated its dash as a negative expression.
The same draft used `↓reduceIte`, a simplifier instruction that runs before simplifying arguments.
The repair gives both forms explicit syntax rules and checks typed introductions, alternatives and following declarations.
The comparison stopped after eighteen recorded attempts. Keep them unchanged while checking the repair.

Fr misses integer-division theorems when reading Lean's installed library.
For example, searching for `tdiv` finds its definition but misses later facts about nonnegative division.
The reader rejects Unicode lambda arrows such as `fun n ↦ n` and branching bindings such as `obtain hp | hq := h`.
A rejected lambda can also hide declarations that follow it. Dated deprecation attributes also interrupt library lookup.
The repair covers all three forms and checks later declarations against the actual installed library.
The regression examples must pass pinned Lean as well as fr's syntax and lookup checks.

The deep conformance job also failed after PR #588 merged because `tsc` was missing from its path.
That job installed Node but omitted the declared TypeScript checker.
Its repair installs the locked checker and exposes its executable before running the unchanged conformance tests.
The job keeps its original deadline and rejects missing compilers.

Three repository-audit jobs also reached their original 15-minute limits while building reports.
Usages reread the same file for each occurrence. Impact rebuilt its line map for each occurrence.
Column lookup also rescanned the line prefix for every position, repeating work on long lines.
The repair keeps one source and line map per file within each report and indexes UTF-8 character boundaries once.
Unicode columns and fresh reads between reports must remain correct. The complete audit keeps its inputs, samples and deadline.
For example, `名` occupies three UTF-8 bytes but still advances a reported character column by one.
Two previously timed-out report groups passed after that repair. The third completed its assertions at the deadline and still lost the job.
Its tests rebuilt the same immutable call graph three times, taking about five extra minutes.
They now share that graph while retaining every sample and assertion.

[PR #588](https://github.com/e6qu/fun-refactor/pull/588) contains the earlier proof, timer and CI repairs.
Its final seven workflows passed, including all 90 main CI jobs.
M3 still needs accepted fr-assisted page-count proofs from GLM and Kimi; that is an open milestone condition.

Also check the [open issues](https://github.com/e6qu/fun-refactor/issues) before reporting one. This list cannot establish that the tool has no bugs.

Report a small program when a supported input produces the wrong result.
Include the command, explicit settings, expected result, actual output, fr version and source revision.
For a write failure, also keep the saved transaction ID and say whether source files changed.

An unfinished trace leaves a question unanswered.
Reporting that unfinished trace as a complete negative answer is a correctness defect.

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

The [active roadmap](PLAN.md#next-work) names the selected agent setup, concrete refactorings and repairs, and Lean proof-writing tasks.
Invalid-number conversion and shared-list analysis are outside the active queue unless one of those tasks needs them.
Their unfinished earlier claims remain in the [qualification checklist](docs/earlier-roadmap-checklist.md).
Keep current task requirements and completion checks in PLAN.md.

## Closed defect history

The [pre-cleanup bug history](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/BUGS.md#fixed)
retains the closed entries and original repair notes.
Use [CHANGELOG.md](CHANGELOG.md) for release history and Git history for the associated changes.

When a defect closes, keep its reproducer as a behavior test and record the repair in its PR.
Keep this page focused on problems a contributor can act on now.

