# Reported bugs and current limits

Use this page when fr refuses a command, gives a wrong answer, or cannot finish your task.
The [roadmap](PLAN.md#next-work) lists planned features. A missing feature and an incorrect answer need different reports.

## Repairs under review

PR #588 repairs Lean proof insertion and syntax handling. Its checks record the remaining validation.
Pasting an indented proof changed its first line's indentation relative to the following lines.
For example, an indented `have h := …` followed by `exact h` failed before Lean could check it.
A `calc` proof could also consume the next calculation step and reject its `:=`.
A blank line containing spaces could set the wrong block indentation. Pinned Lean accepts the exact rejected proof.
A typed proof such as `(by rfl : x = x)` also needs its annotation kept outside its proof steps.
The [regression fixture](tests/fixtures/lean-proof-locations.lean) checks calculation steps, nested proofs and following declarations.
The proof API tests check pasted indentation and require read-only checks to preserve the original file.
Fr also rejected `refine ?_` before Lean could check whether later tactics solved that temporary goal.
The repair lets Lean check the complete proof. An unsolved goal must still fail.
Goal discovery now supplies the exact `target` to copy into a proof command.

The local attempt timer also omitted macOS system sleep, understating ten failed attempts in the latest packet.
The repaired timer includes sleep and keeps the original 150-second deadline.
The affected timing totals cannot establish speed or cost differences. All 29 rebuilt-tool checks passed.
Independent grading accepted 25 proofs and retained all 11 timeouts as failures.
The [PR checks](https://github.com/e6qu/fun-refactor/pull/588/checks) show complete validation status.
The earlier `show … by …` repair passed its rebuilt-tool controls; the live comparison still exposed these separate defects.
The saved-answer benchmark also treated a missing memory sample as zero memory and rejected very short completed edits.
The repair records unknown memory explicitly and counts edits without a sample in the report. Elapsed time, CPU and limits stay checked.
Its [replacement collection passed on GitHub](https://github.com/e6qu/fun-refactor/actions/runs/40670597090); earlier failed artifacts remain unchanged.
The latest agent records also exposed a repair gap after a direct proof edit removed `sorry`.
Fr could no longer select that theorem for a draft check, even when Lean rejected its proof.
The repair resolves an exact named proof region directly. It also distinguishes names such as
`claim` and `claim_extra`. The [rebuilt-tool checks passed](https://github.com/e6qu/fun-refactor/actions/runs/40678657753).
Dogfooding the documented Lean-library search also found false syntax errors.
Examples include subscript names such as `h₁`, named goals such as `?goal₁`, and explicit conversions such as `↑n`.
Pinned Lean accepts the reproducer. GitHub generated the repaired parser, and all 26 translation tests passed.
The earlier integer-library scan reported gaps in 14 of 18 files, so it cannot establish complete lookup.
CI also exposed a keyword-test defect: apostrophes in JavaScript comments confused its quote scanner.
The test now reads generated grammar tokens. Its fuller inventory found two missing writer escapes, `obtain` and `suffices`.
The repair escapes both names when writing Lean; the translation tests cover both escapes.
The proof gate also reached its 15-minute limit after its Rust tests passed.
Checkout spent about 100 seconds on files, and setup installed unrelated Terraform, Helm and web-application fixtures.
The repair excludes historical result directories from that job's checkout and installs only its required pinned tools.
All compiler, proof, source-execution and refusal tests remain, with the same deadline.
The reduced setup passed the complete model job in ten minutes under its original fifteen-minute limit.
Report retention separately reached its five-minute limit after fetching every branch and tag for nearly two and a half minutes.
It now checks out the current commit and explicitly fetches each recorded producer revision, preserving the same audit and deadline.
Other proof jobs also spent their deadlines checking out historical results before running their checks.
Their checkouts now exclude that directory. Each proof matrix runs at most two jobs concurrently to reduce checkout and artifact-upload bursts.
All language cases and original per-job deadlines remain unchanged.
The SDK examples still need the saved flow-cache measurements to check the benchmark table.
Their checkout includes that specific fixture; it does not skip the measurement check.
Older session-cost and saved-answer PR checks also blocked checks of newer commits.
New commits now cancel those superseded PR checks. Deliberate report-writing runs stay serialized.
Language-report retention also spent five minutes fetching the complete result archive before running a check.
It now fetches each listed report, its artifacts, trial records and referenced source records.
The checkout checks every recorded hash and keeps genuine source changes visible. Final documentation generation updates the saved status.
The same source checks, artifact hashes, retention audits and five-minute deadline remain.
The [complete proof workflow](https://github.com/e6qu/fun-refactor/actions/runs/40737530552) passed all 27 jobs.
The [language-report job](https://github.com/e6qu/fun-refactor/actions/runs/40743704178) passed in 75 seconds under its original five-minute limit.
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

