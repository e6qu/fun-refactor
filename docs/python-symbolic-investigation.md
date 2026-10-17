# Can checking conditions help answer a real Python question?

Suppose an application lets someone request a quantity between two limits.
It uses Pydash's `clamp` function to keep the quantity in that range.
For example, `clamp(20, 1, 10)` returns 10.
Before trusting this helper with limits supplied by a user, ask what happens when the lower limit exceeds the upper limit.

This investigation compares ways to answer that question. It adds no language support to `fr`.
The [current library results](python-repository-flow.md) still leave full-package tracing unfinished.
Jaraco.functools remains unread; this experiment uses the already familiar Pydash project.

## State the required behavior first

The experiment uses integers and three explicit requirements:

| Question | Required behavior | Inputs admitted |
| --- | --- | --- |
| Does the result stay between ordered limits? | `lower <= result <= upper` | Only `lower <= upper` |
| Can callers omit that ordering check? | `lower <= result <= upper` | Any integer limits, including reversed limits |
| What does the two-argument call mean? | Return the input unless it exceeds the upper limit | Any integer input and upper limit |

The second requirement deliberately asks too much. There is no number between reversed limits.
A failing example exposes a missing assumption in the caller's requirement; it does not by itself establish a Pydash bug.
The two-argument call provides only an upper limit. For example, `clamp(-10, 5)` returns -10, not 5.
These requirements do not cover floats, decimal numbers, custom objects or application behavior outside this function.

## Compare three checking methods

A test supplies inputs and compares the result with the required behavior.
The handpicked tests include values below, inside and above the limits, boundary values and reversed limits.
The generated tests try every combination from seven stated integers: -100, -10, -1, 0, 1, 10 and 100.
They exclude reversed limits only for the requirement that explicitly forbids them.
Passing those tests establishes agreement on those inputs, not on every possible integer.

Symbolic execution keeps input values unknown while exploring conditions in the function.
A solver asks which values could satisfy those conditions and violate the required behavior.
This experiment uses CrossHair with typed wrapper functions that state the three requirements.
Read [how CrossHair works](https://crosshair.readthedocs.io/en/latest/how_does_it_work.html) and its
[limitations](https://crosshair.readthedocs.io/en/latest/limitations.html).
Every reported failing input must reproduce in an ordinary Python process.
The collector reads only literal integer arguments from tool output; it never executes generated tool text.
Finding no failing input remains a limited observation, not a proof about the application.

## Keep the original package separate from a simpler copy

The project revision is `24ae1aad60dec4d9d5a0e88b8d8d692ee85f232e` in `dgilland/pydash`.
One context imports its unchanged original package, including package startup.
Another copies the function body into a small module and removes its type annotations, which refer to definitions in the omitted module.
Both contexts use the same integer wrappers and requirements.
The report retains the original declaration, complete source archive, source inventory and license.
Success in the copied context cannot establish support for importing the original package.

The experiment also asks `fr` to follow a submitted value through `clamp(value, 1, 10)` to an output.
That is a diagnostic of the existing tracing capability.
It cannot answer the numeric requirement in the table, so its results are not counted as competing numeric checks.
Its report must keep unsupported imports, startup operations and exhausted searches visible.

## Account for the extra work

The comparison is between handpicked tests, generated integer tests and CrossHair checking the same stated requirements.
Each operation starts a fresh Python process. Elapsed time includes startup, imports, checking, output and host monitoring.
Sampled memory and CPU cover the child process group; they exclude the monitoring process.
Short resource peaks can fall between samples.
The report records two repetitions per method, requirement and context, with a fixed shuffled order.
Two repetitions can reveal large costs or failures, but cannot establish a stable speed distribution.
Filesystem caches and other runner activity remain uncontrolled.

Dependency installation, source preparation, the `fr` build and counterexample replay are separate costs.
The artifact keeps installation and build time records, resolved packages and downloaded package hashes.
Compare the full costs needed for the intended use before drawing a speed conclusion.
No agent runs in this experiment, so it cannot establish fewer agent calls, lower model charges or cheaper repairs.

Each measured process has a 20-second elapsed limit, 10 sampled CPU seconds, 512 MiB sampled memory,
16 MiB workspace growth and 1 MiB output.
CrossHair also has a three-second budget per requirement and a one-second budget per route.
The GitHub job has a 15-minute deadline including setup and build.
Every attempt is retained, including stops, malformed results and failing controls.
A resource stop leaves the question unfinished; investigate its cause without increasing the limit.

## Run and inspect the investigation

Use the [Python numeric investigation workflow](../.github/workflows/python-symbolic.yml) on a GitHub runner.
It runs collector controls before building `fr`, collects the comparison and uploads an artifact even after failure.
The artifact contains `result.json`, raw output for each attempt, original package source and setup records.
The [collector](../tools/python-symbolic-experiment.py) records the exact project commit and input file hashes.
There are 36 planned measured checks: two contexts, three requirements, three methods and two repetitions.
Replays and the two `fr` diagnostics appear separately.

No results have been accepted yet. After collection, inspect the first attempts before interpreting corrected runs.
Ask whether symbolic execution found useful inputs the ordinary tests missed, whether those inputs reproduced,
and whether the extra work was worthwhile.
If ordinary tests already answer the question clearly, that is a reason to keep the simpler approach for this task.
Only propose integration into `fr` after identifying a concrete benefit or a missing connection that the experiment demonstrates.
