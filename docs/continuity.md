# Pick up the current work

Start with the [plan](../PLAN.md) for the goal and [current results](roadmap-status.md) for the checked requirements.
This page tells the next contributor what needs doing. It keeps current instructions separate from dated experiment history.

## What users can do now

An agent can find functions and callers, read selected code, preview an edit and run project-declared checks.
Saved changes can be inspected, reversed or continued after a pause.
Small Python examples can also follow a configured input through a helper to an output.

The [original-package checks](package-milestones.md) now pass for Boltons' integer bounds
and dateutil's Easter dates. They use the original imports, independently stated Python
results, renamed and moved callers, and saved versus fresh answers after relevant changes.
JDcal's leap-year question also passed on its first evaluation after the analyzer freeze.
This completes both near-term package milestones across three independent projects.
The same analyzer handled all three. JDcal's README supplied the question before we read its implementation.
The [milestone record](package-milestones.md) links the original results and limits.

For example, “does customer input reach `send(value)`?” can return a possible connection with source locations.
If importing the helper runs unsupported startup code, the question stays partly unanswered.
An empty trace in that situation cannot establish that the input never reaches the output.

Toolz, funcy and pydash still give incomplete answers when used through their full packages.
Some helpers work when copied into small isolated files. That tests a simpler question with different import behavior.
We have not demonstrated that `fr` saves time or money on unfamiliar tasks.

## Next: change a project and keep the answers trustworthy

The [next milestone](project-change-milestone.md) shifts work from questions about unchanged packages to changes across a project.
Its two outcomes are correct saved answers after edits and a complete function change in Rust and Python.
For example, renaming `subtotal` must update the required callers and imports, invalidate affected findings and preserve unrelated ones.
The changed program must pass its checks. Its patch must also work in another checkout.

The tasks were selected before support was added: rename `glob_with` in Rust glob,
rename `clamp` in Python Boltons, and keep Humanize discovery current through the
declared project changes. The [task record](../tests/agent-eval/project-changes/tasks.json)
pins the original projects and expected behavior. Humanize's implementation and
tests remain unread while developing the checker and procedure.

The implementation is in [draft PR #551](https://github.com/e6qu/fun-refactor/pull/551).
It adds Python function renames through original imports, preserves local import
names, distinguishes same-named Rust declarations, and ties saved findings to the
actual checker implementation. Documentation joins the reviewed scope as separate
text mentions. An exact `fr file replace` transaction handles Boltons' documentation
instruction, with its own check and undo history.

Next, finish the hosted development gate and the two familiar projects' deliveries
and saved-answer comparisons. Apply any runner formatting corrections and fix
failures before recording the implementation freeze. Only then run Humanize's first
evaluation. Keep every failure and resource stop, retain the original results,
recheck the earlier package questions, refresh affected reports and reconcile tracking.
Full validation and the unread-project evaluation are still pending.
The tracking requirements for project changes and complete function edits remain open.
Language work should address an obstacle in a selected task. Broader support and cost savings need their own results.

## The current reader review

The reader review covered 120 guides and references, including tutorials, agent instructions and project trackers.
New guides follow the same rules and receive link and example checks.
Keep concepts explained where readers first need them, without assuming knowledge of earlier development.
The [writing guide](style.md) describes the review rules; the [measurement guide](benchmarks.md) names baselines, extra costs and missing measurements.

For a rename lesson, supply the starting files and explain a preview, diff and saved transaction before asking the reader to use them.
For a speed claim, explain what ran, its comparison, included setup and validation, and what the measurements leave unknown.
For tracking, state the user outcome and next action before internal IDs or run history.

The Python behavior guide is a recorded test input. Changing its wording requires a new hosted Python collection and audit.
Agent instruction changes also require refreshing the derived text-size report.
Do not change recorded fingerprints to disguise outdated results.

## Continue Python work from actual library failures

These findings remain useful when a selected change task asks whether submitted input reaches `send` through a package helper.
Use the helper through its original package imports. Copying it into a smaller file changes the question.
Importing a package can run setup code before making its helper available.
That code can replace a helper, so ignoring it can give the wrong answer.

A small setup function can now fill dictionaries created earlier in its own file.
For example, `register(labels, table)` can copy known labels into the table.
The [complete startup lesson](check-python-startup.md) introduces arguments, parameters and defaults before checking this example against Python.
Its `output=table` parameter stores a reference to the earlier dictionary when Python defines the function.
Leaving `output` out uses that stored object; supplying another dictionary replaces it for that call.

The checker follows which dictionary each parameter refers to.
Passing the source dictionary as the destination cannot hide a write during iteration.
Changed source contents, rebound defaults, shared dictionaries imported from other files and unknown calls leave the answer unfinished.
Replacing a helper during setup also stays unfinished: the lesson makes Python return zero instead of the submitted input.
Both the function's code and its repeated loop work keep the existing 128-node limit.
A node is a piece of parsed code, such as a name or call, rather than a line of text.
Ordinary helper calls still cannot consume stored function or object defaults.
This allowance covers the small setup functions described above.

Saved answers depend on the source files and places searched for imports.
Editing a file or adding a competing import file requires another check.
The [two-question lesson](check-python-together.md) prepares shared setup once within a request, then answers each helper question separately.
The [saved-startup lesson](save-python-startup.md) reuses checked preparation in a later process while those inputs and settings match.
It supplies all settings explicitly. Damaged saved records fail visibly, and unknown setup effects create no reusable entry.
These features do not establish complete package support.

The [package startup findings](python-package-startup-results.md) identify the remaining obstacles.
A module is a loaded Python file or package.

| Question | What still stops an answer | Next step |
| --- | --- | --- |
| Does input pass through Toolz's helper? | Setup builds an argument table using module objects, conditional imports and code that reads functions' accepted arguments | Follow that setup call and establish what it reads and changes |
| Does input pass through Funcy's `inc`? | Python observes sixteen package files plus the caller, beyond the sixteen-file limit | Identify the imports and setup needed for this helper within the limit |
| Does input pass through Pydash's `add`? | Python observes about 420 KiB of package source, beyond the 256 KiB limit | Establish which source and setup affect this helper within the limit |

A KiB is 1,024 bytes. The [findings](python-package-startup-results.md) explain how Python's observed files differ from the analyzer's loaded files.
Use the [import explanation](explain-python-imports.md) to locate unread files, unresolved names and unchecked setup statements.
Choose a general language rule from the actual statement, then compare its behavior with Python.
Do not teach the checker library names or expected answers, increase limits, or assume unknown setup has no effect.
Any future split into smaller checks must record how the results depend on one another.
Combining smaller checks to finish these full-package questions remains unimplemented.

The [Python behavior guide](python-contract.md) owns supported language rules and runnable examples.
It covers stored function defaults, explicit replacements, several names assigned together, and simple settings shared between files.
The [library results](python-repository-flow.md) keep earlier progress counts and distinguish full packages from copied helpers.
Fewer unchecked statements alone do not establish a complete answer.

The earlier timing studies concern examples and earlier code, not the new dictionary-argument rule:

- The [optimized comparison](python-optimized-cost-results.md) found that caching the parsed project helped fresh checks in its examples.
  Saving setup or complete answers did not reduce median sequence time there.
- The [installed-client comparison](python-process-cost-results.md) found lower totals from keeping an installed client running between questions.
  Its baseline restarted the same installed client for each question; totals included first startup and final shutdown.
- The [two-helper comparison](python-together-cost-results.md) compared separate and combined questions in running installed clients.
  Three matching-answer examples had lower totals together and larger reports. Its one-step example performed different work and cannot establish a saving.

The guides state setup costs, missing measurements and the original source used to audit each study.
No separate measurement establishes the new language rule's time or memory cost, complete environment setup cost, or savings on unfamiliar agent tasks.

The [numeric investigation](python-symbolic-investigation.md) used ordinary tests and an existing symbolic checker to search for failing inputs.
Both found the same missing assumption: callers must order the lower and upper quantity limits.
That example does not justify another execution engine inside `fr`; use the simpler tests for this question.
The investigation adds no package support and does not restart stopped studies.

A previous impact audit passed its assertions but exhausted the fifteen-minute job deadline during cleanup.
The test now avoids building reports for sampled names without editable uses.
It also reuses each file's table of line positions.
Every existing behavior check remains; file and analysis errors fail visibly.
The [complete recheck](https://github.com/e6qu/fun-refactor/actions/runs/39179402214) and two merged-main runs passed at the same deadlines.
Those main runs took [667 seconds](https://github.com/e6qu/fun-refactor/actions/runs/39210593046) and
[764 seconds](https://github.com/e6qu/fun-refactor/actions/runs/39231454941) for the complete impact job.
These diagnose CI execution, not analyzer speed or language overhead.

Keep PR descriptions readable too. The [writing guide](style.md#pull-request-descriptions) asks for a concrete example, limits and relevant checks.
Link detailed inventories and failed attempts from that short description.

## Keep the next unfamiliar test useful

Jaraco.functools and fn.py have already had their first evaluation. Both stopped before
analysis, for the admission and selection reasons recorded in the [package milestones](package-milestones.md).
Do not describe either as an unused reservation or a successful result.
The next comparison, [JDcal's leap-year check](leap-year-package-question.md), passed after the new freeze.
Its public README supplied the question before we read its implementation. JDcal is now a familiar project.
Reserve another unread project before using any later comparison to claim progress on unfamiliar code.
Toolz, funcy and pydash are already familiar development examples.
The [selection record](../tests/agent-eval/python-repository-flow/replacement-holdout.json) identifies the reserved project and earlier selection mistake.

The [current checker collection receipt](../tests/agent-eval/results/2026-10-20-leap-year-scalar-collection/manifest.json) records the refreshed analysis results.
The [Python example receipt](../tests/agent-eval/results/2026-10-20-leap-year-python-contract/manifest.json)
covers 476 programs and 952 original or renamed cases, including dictionary arguments and defaults during startup.
It preserves 364 incomplete cases, 18 refusals and 134 documented differences between the checker's model and Python's observed behavior.
These include programs where startup replaces a helper or raises before the application runs.
Those answers remain unfinished; the tests keep the differences visible instead of turning them into complete answers.
The Python interface check passed 1,656 cases, including 75 cases for saving preparation between requests.
Within that suite, 76 cases check bounded loops and 96 check dictionary arguments, stored references and unsafe aliases.
Those cases compare saved and fresh answers, ordinary Python, source edits, competing imports, damaged records and unchanged limits.
These counts describe the checked examples; they do not mean every example has a complete analysis.
Each capture keeps its original commit. Retention verifies identical current analyzer files and reruns every report's input checks.
The [CI guide](ci.md#collect-python-results-while-checking-the-interface) explains concurrent checking and unchanged deadlines.
The earlier [collector receipt](../tests/agent-eval/results/2026-10-16-incremental-reports-python-contract/manifest.json)
preserves the 818-case comparison after saving each new result without repeatedly rewriting earlier cases.
The [CI guide](ci.md#save-python-test-progress-without-repeating-earlier-work) explains saved progress and the unchanged final checks.
Use the [measurement guide](benchmarks.md#saving-python-test-results) for that saving-only comparison; it does not measure the later language-support change.
A receipt records the run and its inputs; it does not establish general correctness by itself.
Keep failed attempts beside corrected ones in the [development history](evaluations.md#python-development-history).
Old report formats use their archived original code. Current readers accept one current format and reject missing or obsolete fields.

## Studies that stay stopped

The [client investigation](../tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md) is closed.
The [packaging pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md) stays stopped, with its failed and unstarted attempts preserved.
A later live comparison needs independent tasks, agreed resource and cost limits, and complete work records.
See the [candidate review](candidate-review-status.md) for unresolved task-review questions.

## Before editing or merging

- Follow the [workstation limits](development.md#working-on-a-shared-desktop). Use the local guard; builds, full test suites and large collections run on GitHub.
- If the guard refuses, move the work to CI. Keep its limits.
- Fix the cause of a failure or timeout. Keep the existing deadline.
- Verify recorded input fingerprints before calling results current.
- Use the configured GitHub login; leave credential files alone.
- Keep each commit message to one line of at most 80 characters, without a body or trailers.
- Wait for passing CI before merging. When authorized, squash with an explicit subject and empty body.
