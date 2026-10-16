# Pick up the current work

Start with the [plan](../PLAN.md) for the goal and [current results](roadmap-status.md) for the checked requirements.
This page tells the next contributor what needs doing. It keeps current instructions separate from dated experiment history.

## What users can do now

An agent can find functions and callers, read selected code, preview an edit and run project-declared checks.
Saved changes can be inspected, reversed or continued after a pause.
Small Python examples can also follow a configured input through a helper to an output.

For example, “does customer input reach `send(value)`?” can return a possible connection with source locations.
If importing the helper runs unsupported startup code, the question stays partly unanswered.
An empty trace in that situation cannot establish that the input never reaches the output.

Toolz, funcy and pydash still give incomplete answers when used through their full packages.
Some helpers work when copied into small isolated files. That tests a simpler question with different import behavior.
We have not demonstrated that `fr` saves time or money on unfamiliar tasks.

## The current reader review

The reader review covers all 120 current guides and references, including tutorials, agent instructions and project trackers.
Keep concepts explained where readers first need them, without assuming knowledge of earlier development.
The [writing guide](style.md) describes the review rules; the [measurement guide](benchmarks.md) names baselines, extra costs and missing measurements.

For a rename lesson, supply the starting files and explain a preview, diff and saved transaction before asking the reader to use them.
For a speed claim, explain what ran, its comparison, included setup and validation, and what the measurements leave unknown.
For tracking, state the user outcome and next action before internal IDs or run history.

The Python behavior guide is a recorded test input. Changing its wording requires a new hosted Python collection and audit.
Agent instruction changes also require refreshing the derived text-size report.
Do not change recorded fingerprints to disguise outdated results.

## Continue Python work from actual library failures

Importing a Python package can execute its startup code before exposing a helper.
Use the reported unchecked statements to choose a general language rule; do not special-case library names or expected answers.
The [Python guide](python-contract.md) owns supported behavior. The [library results](python-repository-flow.md) own detailed measurements.

| User question still unanswered | What currently stops the answer | Next investigation |
| --- | --- | --- |
| Does input pass through Toolz's helper? | Startup calls and imported code remain unchecked | Inspect the startup call that records function arguments, and imports needed by the helper |
| Does input pass through Funcy's `inc`? | The search reaches its existing 16-file limit | Check which remaining imports and startup statements affect the selected helper within that bound |
| Does input pass through Pydash's `add`? | The search reaches its existing 256 KiB source limit | Investigate checking the helper and relevant startup code within that bound |

The earlier import-search change shares one provider search among names in the same `from` statement.
For `from helpers import add, subtract`, it finds `helpers` once while retaining both names and their binding order.
Child-module searches still count separately. No search or source limit increased.

In the [audited comparison](https://github.com/e6qu/fun-refactor/actions/runs/38739064758), Funcy uses 75 searches instead of 120.
Pydash uses 47, clears its former repeated-search obstacle, and reaches the separate source-size limit.
Pydash has 420 unchecked startup statements instead of 510 across the same eight loaded files.
Funcy still has 80 across 16 files; Toolz still has 92 across 14.
All 40 full-package cases remain unfinished: 32 incomplete analyses and eight deliberate invalid-output-budget refusals.
These earlier counts describe analysis progress, not complete package support or lower agent cost.

The callback-table change checks harmless registry construction and local dictionary writes.
A registry is a table holding values or functions for later use.
For example, `handlers = dict(send=lambda value: value)` creates a function without calling it.
The [callback-table example](python-contract.md#prepare-a-callback-table-without-calling-its-entries)
compares that setup with an evaluated default or an imported shared dictionary.
Reading the table's contents, calling its stored functions and arbitrary package startup remain unfinished.
Keep those limits visible when reviewing the fresh package replay; more supported syntax alone does not close the real-project requirement.

The import-order change checks imports that return to a file already loading.
For example, a second file can import a function already defined by the first file.
Moving that definition below the import makes Python raise `ImportError`; the analysis must remain incomplete.
The [import-order lesson](python-contract.md#follow-imports-that-return-to-a-file-already-loading) introduces this case before showing the files.
The command-line and Python interface tests compare successful and failing orders with Python execution.
They include package startup, requested child files, written export order and saved answers after a change.
For `from helpers import *`, the file's `__all__` list names what to import.
Reordering that list can change whether Python loads the package successfully.

The fresh familiar-library replay still has no complete full-package answers: 32 analyses remain incomplete and eight deliberately invalid requests are refused.
Copying selected library functions into smaller files gives 16 complete answers.
A separate test file of simple comparison functions gives eight.
Those smaller successes do not establish support for the original packages. Next, investigate their remaining startup operations and imports within the existing limits.

Startup reports now distinguish calls, assigned values, writes, class setup, decorators and control flow.
For example, `metadata = read_input()` reports an unchecked startup call instead of the generic module-statement reason.
Per-file counts include statements beyond the sixteen displayed locations, so later loaded files remain visible.
The [guide](python-contract.md#find-the-statement-that-prevents-an-answer) explains these reasons and their limits.
These reports improve the investigation; they do not add supported Python behavior or check files that the import search could not load.
Use the counts to choose the next general language rule, then compare runtime behavior before changing completeness.

An unused function can now store an earlier function or a built-in object as a default.
A default supplies an argument when the caller leaves it out.
For example, `def unused(value, factory=dict): ...` stores the dictionary constructor without calling it.
Likewise, `follow=selected` stores an earlier function; `follow=selected(0)` calls it during definition and remains unchecked.
An alias gives an existing function another name. Checked aliases and ordinary function imports can supply the reference; later or missing names cannot.
Built-in names must still refer to their original objects or an earlier checked local declaration.
The [default example](python-contract.md#store-a-function-as-a-default-without-calling-it) introduces the two-file case and its commands.
Calling a function with these function or object defaults remains unfinished, including calls with replacement arguments.
Replay full packages after further changes; fewer unchecked statements alone do not establish complete package support.
The new replay removes ten unchecked Toolz declarations, including nine `factory=dict` defaults.
For the original identity-helper question, Toolz falls from 92 to 82 unchecked statements across the same loaded source files.
Funcy stays at 98 and Pydash at 461 for their separate identity-helper questions.
The full-package answers remain unfinished; the [library results](python-repository-flow.md) keep those counts separate from `inc` and `add`.

Keep PR descriptions readable too. The [writing guide](style.md#pull-request-descriptions) and template ask for a user example, limits and relevant checks.
Link detailed test inventories and failure history from the short description.

## Keep the next unfamiliar test useful

Jaraco.functools remains unread. Keep it that way during development, then freeze the implementation before its first evaluation.
Toolz, funcy and pydash are already familiar development examples.
The [selection record](../tests/agent-eval/python-repository-flow/replacement-holdout.json) identifies the reserved project and earlier selection mistake.

The [analyzer collection receipt](../tests/agent-eval/results/2026-10-16-function-defaults-scalar-collection/manifest.json) identifies the unchanged native analysis results.
The separately refreshed [Python example receipt](../tests/agent-eval/results/2026-10-16-incremental-reports-python-contract/manifest.json)
covers 409 programs and 818 original or renamed cases, including function defaults and import order.
The collector now saves each new result without repeatedly rewriting all earlier cases.
The [CI guide](ci.md#save-python-test-progress-without-repeating-earlier-work) explains saved progress and the unchanged final checks.
Use the [measurement guide](benchmarks.md#saving-python-test-results) for the saving-only comparison; Python language support remains unchanged by this collector fix.
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
