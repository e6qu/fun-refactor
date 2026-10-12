# Pick up the current work

Start with the [plan](../PLAN.md) for the goal and next work. The [current results](roadmap-status.md)
own the test counts. This page gives the next contributor the decisions they need to continue.

## What works now

An agent can find functions and callers, inspect selected code, preview edits, run declared checks,
and keep saved changes for undo or resumed work. Recent Python fixes improve small trace questions.
For example, an unused function body and `metadata = 0` no longer prevent an otherwise supported answer.

The remaining real-library problem is still visible. Toolz, funcy and pydash imports often execute
code that the analyzer cannot handle. Their package results remain incomplete. Copying a helper
into a small isolated module can produce a complete answer, but that does not establish that its
original package is understood. See the [library examples and results](python-repository-flow.md).

## Current priority

The documentation review now explains the user’s task first, shows an example,
and describes the result before presenting commands or record fields. Keep that order in later edits.
The [writing guide](style.md) applies that rule. [Result meanings](understanding-results.md) distinguishes a found location,
a possible input connection, an unfinished answer and a checked repair.

Next, continue the Python work from the actual import and initialization failures.
Imports preserve a package’s classes, decorated functions and assigned values before considering
a file with the same name. These unsupported values still leave the answer unfinished.
Literal export lists now let fr follow `from helpers import *`. Computed or directly mutated exports
remain unfinished, along with external dependencies and other code that runs while importing a package.
Keep jaraco.functools unopened during development, and freeze the next implementation before its
first evaluation. Toolz, funcy and pydash are already familiar test projects. The recorded
[holdout selection](../tests/agent-eval/python-repository-flow/replacement-holdout.json) explains the reservation.

## How to keep the results trustworthy

Keep one current analyzer, SDK and cache format. Missing fields and older formats must fail
explicitly. Historical results use their archived original code; they do not require older readers
in the current SDK.

When code or a recorded input changes, run the affected collection on GitHub. Retain the result
with the file fingerprints and run identity that produced it. Do not edit hashes to make an old
result appear current. The [current collection receipt](../tests/agent-eval/results/2026-10-12-star-imports-scalar-collection/manifest.json)
and [Python behavior examples](python-contract.md) identify the active results.

Keep failures alongside corrected attempts. The [failed generator collection](../tests/agent-eval/results/2026-10-12-declarations-history-python-contract-failure/manifest.json)
retains its original result and validator. The [first initialization gate](https://github.com/e6qu/fun-refactor/actions/runs/38247965263)
records obsolete boundary fixtures and a metadata-size failure; the [corrected gate](https://github.com/e6qu/fun-refactor/actions/runs/38248690526)
passes without increasing the existing output budget. Detailed history belongs in those records.

## Studies that stay stopped

The [client investigation](../tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md) is closed.
The [packaging pilot](../tests/agent-eval/opencode/changes/configured-2026-10-08/README.md) stays stopped.
Preserve its failed and unstarted attempts. A new live comparison needs suitable independent tasks,
resource approval and complete cost records before it can run. Read the [candidate review](candidate-review-status.md)
for the remaining gaps.

## Before editing or merging

- Follow the [workstation rules](development.md#working-on-a-shared-desktop). Use the local guard; full builds, full gates and large result collections run on GitHub.
- If the guard refuses, move the check to CI. Do not bypass it or raise its limits.
- Fix the cause of a failure or timeout. Keep the existing deadline.
- Check the recorded file fingerprints before calling a result current. Native edits also affect the index and compiler reports.
- Use the configured GitHub login. Do not inspect or copy credentials.
- Keep commit messages to one line of at most 80 characters, with no body or trailers.
- Wait for passing CI before a merge. When merging is authorized, squash with an explicit subject and an empty body.
