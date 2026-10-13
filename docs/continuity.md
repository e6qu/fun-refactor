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
The current class work admits plain declarations and empty instances made without custom constructors.
Methods stay deferred. Bases, decorators, descriptors and executed class-body code remain unchecked.
Replay complete original modules separately from copied helper declarations and full package imports.
Unchecked initialization now identifies exact source statements and counts locations omitted from the report.
Use these locations to choose further language work; they do not change the tracing conclusion.
Postponed annotation support follows a concrete pydash blocker: its typed helpers use
`from __future__ import annotations`. Such signatures can defer annotations when the defining
module has a supported directive and every local `__future__` candidate is missing.
Defaults, decorators, annotation inspection and local shadows still need separate checks.
The familiar pydash package exhausts its 128 import lookups before reaching the annotation directives;
its retained annotation diagnostics therefore do not mean the focused directive controls failed.
Literal `__version__`, `__author__`, `__license__` and `__copyright__` declarations now receive the
same inert treatment as ordinary module constants. Import controls, global reads, evaluated values
and repeated bindings remain unchecked. This removes a metadata blocker, not the full package boundaries.
Unused literal collections now let otherwise supported helpers receive a complete answer.
For example, `registry = {}` and `modules = ('calc', 'colls')` can coexist with a helper that forwards
its input. This removes Toolz's empty registry declarations and funcy's literal module-name tuple
from the reported startup blockers. Calls, unpacking, comprehensions, unhashable keys, global reads
and mutations remain unchecked; the original packages still receive incomplete answers.
The next function-name work follows Toolz's `comp = compose` declaration.
Plain assignments to already-bound local functions can retain the defining helper's behavior and locations.
Missing names, built-ins, computed assignments, rebinding and shared mutable objects remain unchecked.
Toolz also builds its signature registry, funcy computes exports and changes function attributes,
and pydash still exhausts the import lookup budget. Check these separately before claiming whole-package support.
Keep jaraco.functools unopened during development, and freeze the next implementation before its
first evaluation. Toolz, funcy and pydash are already familiar test projects. The recorded
[holdout selection](../tests/agent-eval/python-repository-flow/replacement-holdout.json) explains the reservation.

## How to keep the results trustworthy

Keep one current analyzer, SDK and cache format. Missing fields and older formats must fail
explicitly. Historical results use their archived original code; they do not require older readers
in the current SDK.

When code or a recorded input changes, run the affected collection on GitHub. Retain the result
with the file fingerprints and run identity that produced it. Do not edit hashes to make an old
result appear current. The [current collection receipt](../tests/agent-eval/results/2026-10-13-literal-collections-scalar-collection/manifest.json)
and [Python behavior examples](python-contract.md) identify the active results.

Keep failures alongside corrected attempts. The [failed generator collection](../tests/agent-eval/results/2026-10-12-declarations-history-python-contract-failure/manifest.json)
retains its original result and validator. The [first initialization gate](https://github.com/e6qu/fun-refactor/actions/runs/38247965263)
records obsolete boundary fixtures and a metadata-size failure; the [corrected gate](https://github.com/e6qu/fun-refactor/actions/runs/38248690526)
passes without increasing the existing output budget. Detailed history belongs in those records.
The [class collection failure](../tests/agent-eval/results/2026-10-13-class-initialization-history-python-contract-failure/manifest.json)
preserves four wrong source-call indices and one renamed boundary label.
Its original validator reproduces those errors; corrected expectations retain the same runtime outcomes and limits.
The [first initialization-location gate](https://github.com/e6qu/fun-refactor/actions/runs/38363037039)
passed the native controls and 845 SDK tests. Its new reuse test stored a report inside the sample project,
which made the next name search ambiguous. The corrected test keeps that report outside the project.
The [first annotation gate](https://github.com/e6qu/fun-refactor/actions/runs/38381948322)
passed native controls and 915 SDK tests. Two reports exceeded the unchanged 8 KiB limit
after contract text grew. Compact current contract text fixes that regression.
The new reuse test also exposed format validation after the freshness check; noncurrent contracts
now fail before dependency changes can trigger fresh analysis.
The daily audit also canceled an unfinished merge audit on the same commit.
Scheduled checks now wait for an active audit; newer pushes can still replace older work.
The first annotation behavior collection found a source-checker gap: Python's annotated argument
span includes its type, while a trace points to the parameter name. The checker now includes
that name's exact byte span and still rejects a substituted annotation span.
The [first collection gate](https://github.com/e6qu/fun-refactor/actions/runs/38417041882)
found an obsolete native fixture that required an empty list declaration to remain incomplete.
The corrected fixture checks accepted literal allocations and keeps mutation unchecked.
The [second collection gate](https://github.com/e6qu/fun-refactor/actions/runs/38419813576)
passed all native controls and 1111 SDK tests. Its imported hash-callback test exposed an observer
that supplied source/sink functions only to the caller's globals. The corrected test uses the
isolated import observer, which supplies those functions to imported modules too.
The [first collection documentation check](https://github.com/e6qu/fun-refactor/actions/runs/38430801404)
found one new sentence beyond the existing prose budget. Splitting the explanation keeps that budget unchanged.
Review also found stale roadmap assertions for the older 211-program inventory.
They now match the independently checked 237 programs; broader package and product goals remain open.
The [first function-alias gate](https://github.com/e6qu/fun-refactor/actions/runs/38440122902)
passed native controls and 1206 SDK tests. Two new tests expected a global declaration to behave
like a local assignment. Corrected controls distinguish lexical declarations from local shadowing
and compare both outcomes with Python. Neither case can establish a complete answer.

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
