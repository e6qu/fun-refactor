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
The [dependency list](../tests/agent-eval/python-symbolic/requirements.txt) freezes the exact packages selected in the first capture.
Its hashes admit only those downloaded files on the Linux Python 3.14 runner.
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

## What the first capture tells us

The [first hosted collection](https://github.com/e6qu/fun-refactor/actions/runs/39037586407) completed all 36 planned checks without a resource stop or collector error.
All three methods found a failing input for the requirement that admitted reversed limits.
CrossHair selected `clamp(1, 1, 0)`, which returns 0. Ordinary Python reproduced all four symbolic reports across the two contexts and repetitions.
That input was also in the generated tests; this capture found no additional failing behavior through symbolic execution.
The handpicked and generated inputs passed the ordered-limit and upper-only requirements.
CrossHair reported those two requirements confirmed within its analysis; the collector retains these as no failing input reported, rather than a proof about the application.

For the ordered-limit question through the original package, the recorded elapsed times were:

| Method | First repetition, seconds | Second repetition, seconds | Highest sampled memory, MiB |
| --- | ---: | ---: | ---: |
| Handpicked tests | 0.200843 | 0.206527 | 20.961 |
| Generated integer tests | 0.195958 | 0.210627 | 20.938 |
| CrossHair | 0.412477 | 0.414322 | 71.047 |

These are operations on one runner, with the startup and monitoring costs described above.
They are not an agent-cost comparison or a prediction for other functions.
Source preparation took 0.234 seconds. Dependency installation took 7.59 seconds; the separate `fr` build took 80.68 seconds with that run's cache state.
The source archive, raw outputs, separate replay costs and setup records remain in the
[original artifact](../tests/agent-eval/python-symbolic/2026-10-17-first-run.zip).
Its [receipt](../tests/agent-eval/python-symbolic/first-run.json) records the producer, tested merge commit and archive digest.
The artifact's machine record identifies Python 3.14.8 and Linux x86-64; the first collector did not record the processor model.
Later captures must record that missing detail. Do not supply it retrospectively from another runner.

The separate `fr` diagnostic loaded eight source files and reached its existing 256 KiB source limit through the original package.
It also left 420 startup statements unchecked, including decorators, calls and module writes, and could not resolve several imported names.
Those obstacles are separate: teaching the analyzer one more assignment form cannot establish a complete package answer.
The copied function produced a complete value trace under the declared simplified rules.
Neither trace checked whether the numeric requirement holds.

For this question, ordinary tests already expose the missing input assumption clearly.
The useful application change is to require ordered limits or deliberately define how reversed limits should behave.
This capture does not justify building a symbolic execution engine inside `fr`.
Keep an existing checker available for a later task whose conditions are harder to test directly, then measure whether its search adds useful findings.
Before expanding package tracing, identify which startup operations affect the selected task and how to check them within the current limits.
Skipping those operations or raising the source limit would leave their behavior unexplained.
