# Reported bugs and current limits

Use this page when fr refuses a command, gives a wrong answer, or cannot finish your task.
The [roadmap](PLAN.md#next-work) lists planned features. A missing feature and an incorrect answer need different reports.

## Repairs under review

These repairs are on `milestone/complete-proof-authoring`; the complete M3 PR is still in progress.
The measured tool passed all 29 proof-preparation jobs and the 17-job deep audit before its latest agent comparison.

| Earlier problem | Current branch behavior |
| --- | --- |
| Valid Lean proof forms were rejected before checking | Accepts discarded assumptions such as `rintro ⟨h, -, -⟩`, simplifier phase markers, Unicode lambda arrows, branching bindings and dated attributes; checks later declarations too |
| Lean propositions were grouped incorrectly | Reads `p ∧ q ∧ r` as `p ∧ (q ∧ r)` and gives disjunction its proper precedence |
| Library lookup lost a declaration's type | Keeps declared types for equation-style definitions and proofs; returns bounded headers without proof bodies |
| A draft error pointed only to generated code | Reports the draft line and source excerpt; retains the actual goal and counts omitted context |
| A valid local Lean lemma with parameters was rejected | Generated reader checks and focused native examples pass for named and anonymous parameterized lemmas; full workflow qualification continues |
| A draft passed fr but its warnings later failed package verification | Rejects draft warnings before saving; a clean proof remains checkable while another theorem is unfinished; focused native checks pass |
| A report calculator accepted late successes or invalid times | Checks elapsed time against the recorded deadline and rejects invalid times; twelve retained complete reports keep their original statistics |
| The small proof diagnostic skipped shared execution instructions | Uses the common instructions for check-file preservation, serial work and finishing; a regression rejects the measured omission |
| Rebuilding the original proof auditor omitted required task files | Keeps both checkouts in the same explicit sparse mode and imports the original auditor before building; hosted validation is pending |
| An application update deleted a frozen CLI executable | New studies keep verified executable and companion copies inside the study directory |
| Deep conformance omitted TypeScript's compiler | Installs the locked checker before running the unchanged checks |
| Repository reports repeated file reads and line scans until timeout | Reuses source/line maps within a report and the immutable test graph within a test; preserves Unicode positions, fresh later reads and every original sample |

For example, `名` occupies three UTF-8 bytes but advances a reported character column by one.
Both frozen repository reviews retain the current reviewed declarations and references.
Installed Lean-library lookup still reports syntax gaps; an empty search in that state cannot establish that a fact is absent.

The latest investigation also found two reporting defects:

- Adding an archived result changed the identity of an otherwise unchanged proof checker.
  The identity now covers checker code, settings, fixtures and control proofs, with a regression for archives.
- The final independent-result job exhausted five minutes while disabling its limited checkout and fetching the whole repository.
  It now keeps only the original checker and required inputs. Rebuilding the original auditor also excludes the large report archive.
  Shared receiver fixtures, including Boltons, remain present. The corrected collector finished in 22 seconds.
  The deadlines, proof checks and original agent outcomes stay unchanged.

The proof gate also searched the whole project for a function whose file it already knew.
That read 2,933 files and began resolving 871,433 references before its three-minute limit.
The query now selects `src/spec.rs` explicitly and checks the returned name, signature and one-file coverage.
The proof checks and repository audits retain their original limits and coverage.

The preceding comparison's nine checking groups accepted all 27 submitted patches; nine attempts timed out without one.
The new complete comparison retained eight timeouts and 28 independently accepted patches.
Its accepted access and range proofs use actual fr checks and writes. M3 still lacks GLM and Kimi page-count proofs.
The [behavior review](tests/agent-eval/proof-writing/behavior-review.md) records the exact mistakes and timing.
The root skill now links arithmetic directly and starts by checking an unfinished draft.
The lesson explains Boolean comparisons, proof-valued arguments and a search for a missing fact.
All four examples compile with pinned Lean; the complete skill passes hosted M2 checks.
The [small diagnostic](tests/agent-eval/proof-feedback/README.md#what-the-first-run-established) retained eight valid proof patches and six completed attempts.
GLM timed out after passing checks; Kimi replaced the check settings with a report. Both remain failed.
The restored shared instructions and clarified skill have no later live measurement yet.
Full comparisons stay paused while the [improvement method](docs/agent-improvement.md) guides the next construction check.

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

