# Why a simple Python helper can need a whole package

You want to know whether submitted input reaches an output through a library helper.
The helper might contain only `return value`.
Importing it can still run other files that change its behavior.

The current Toolz, Funcy and Pydash package answers remain incomplete.
This investigation observes ordinary Python execution to choose the next analysis change.
It adds no supported language behavior and establishes no agent time or money saving.
The later [package milestones](package-milestones.md) record original-package successes
in Boltons and dateutil and the unsuccessful first unfamiliar-project attempts.
Read the [recorded findings](python-package-startup-results.md) for the observed files, costs and next implementation question.

## Try a package that changes its helper

Use Python 3.14 and a new folder. No extra Python packages are necessary.
From a terminal, create the package folder and enter its parent:

```sh
mkdir -p startup-example/example
cd startup-example
```

A package is a folder of Python modules with an `__init__.py` file.
Python runs that file before finishing an import of a module inside the package.
Create `example/helpers.py` with these contents:

```python
def relay(value):
    return value

def stopped(value):
    return 0
```

Create `example/setup.py`:

```python
from . import helpers
helpers.relay = helpers.stopped
```

The leading dot means another module in this package.
The assignment gives `relay` a different function before the caller receives it.
Create `example/__init__.py`:

```python
from . import helpers
from . import setup
```

From `startup-example`, run:

```sh
python3 -B -c 'from example.helpers import relay; print(relay(17))'
```

Python prints `0`. Reading only the original `relay` body would suggest `17`.
Delete the `from . import setup` line from `example/__init__.py` and run the command again.
The fresh Python process prints `17`.
Changing startup code changed the result without editing the original helper body.
The investigation's controls execute both versions and compare ordinary and instrumented execution.

## Observe execution without claiming complete analysis

Instrumentation means adding a way to observe a running program.
Here, Python's tracing interface records source-line events and function calls inside the pinned package source.
A line event says Python reached that line. It does not establish that the whole statement completed successfully.
It also cannot prove the absence of effects in native code, other environments or untried inputs.

The collector separates events during import from events during the selected helper's calls.
It records the package modules present when import finishes, source fingerprints, source sizes and the selected helper's defining file.
A fingerprint identifies file contents so the comparison cannot silently use different source.
The execution records include first-encounter order, reached lines and function-call counts within each phase.
They keep function bodies separate from module-level lines.

The comparison uses the existing saved analyzer answers for four helper questions:

| Helper | Integer calls observed | Expected Python results |
| --- | --- | --- |
| Toolz `return_none` | `-4`, `0`, `17` | `None`, `None`, `None` |
| Funcy `inc` | `-4`, `0`, `17` | `-3`, `1`, `18` |
| Funcy `dec` | `-4`, `0`, `17` | `-5`, `-1`, `16` |
| Pydash `add` | `(-4, 3)`, `(0, 3)`, `(17, 3)` | `-1`, `3`, `20` |

These familiar source snapshots match the [existing package replay](python-repository-flow.md).
The [plan](../tests/agent-eval/python-startup/plan.json) fixes their archive fingerprints, selected questions, inputs, expected outputs and earlier report fingerprint.
Each helper receives two ordinary runs and two instrumented runs, in a fixed shuffled order.
Each run starts Python afresh, imports the unchanged package, then calls the helper three times.
The complete schedule contains sixteen processes and forty-eight helper calls.

The report compares files Python reached with files the analyzer read.
It also checks whether Python reached a module-level line inside each displayed unchecked statement.
The analyzer displays at most sixteen such statements; omitted statements remain counted and unchecked.
An unobserved statement stays unchecked. A reached statement also stays unchecked by the analyzer.
Runtime observations cannot turn its incomplete answer into a complete one.

## Understand the comparison costs

The baseline uses ordinary Python importing the same package and making the same three calls.
The instrumented run adds the tracing callback and its records.
Both runs also inspect source identities and write their observation after the calls finish.
Their internal import and call timers exclude that later inspection and output.
The outer elapsed timer includes interpreter startup, imports, calls, inspection, output and host monitoring.
Sampled CPU and memory cover the child process group and exclude the monitor.
Short peaks can fall between samples.

Checkout, dependency installation, archive admission and the collector's own work fall outside those per-process timers.
The workflow records those steps separately without claiming a complete setup-cost measurement.
Two repetitions are too few to estimate a stable slowdown, especially for calls lasting only microseconds.
Filesystem caches and other runner activity remain uncontrolled.
These measurements describe observation overhead on Python. They do not measure `fr` overhead against Python or predict application performance.
The saved `fr` answers come from an earlier run, so this investigation makes no timing comparison with the analyzer.

## Run and inspect the hosted investigation

Run the [Python package startup investigation workflow](../.github/workflows/python-startup.yml) on the branch containing this change.
It installs the existing pinned runtime dependency, checks the controls, collects observations and audits the saved records in a separate process.
Collection requires a GitHub Linux runner; the shared desktop does not run these workloads.
Download the `python-package-startup` artifact from that run.

The artifact contains `result.json`, each attempt's raw standard output and errors, and the unchanged package source with its licenses.
The result identifies the collector commit, input fingerprints, Python version, processor and process limits.
Each attempt keeps its host measurements and comparison with the earlier analyzer answer.
Read an attempt's `.stderr` file when its `error` field has a value.

Every process has a twenty-second elapsed limit, ten sampled CPU seconds, 512 MiB sampled memory and 16 MiB workspace growth.
Its combined output limit is 1 MiB. MiB means 1,048,576 bytes.
Instrumentation also stops after 200,000 package trace events; the job deadline is five minutes.
The existing analyzer limits remain sixteen files or package directories, 256 KiB of source and 128 module searches.
The collector preserves raw attempts before interpreting their output.
A stop, changed source, missing result or altered comparison fails collection and its audit.
The first failed process stops the schedule after saving its raw output; remaining processes stay unstarted.
This leaves time to upload the failure instead of spending the job deadline on repeated failures.
Fix the cause before collecting again; keep unsuccessful attempts alongside corrected ones.

The [measurement guide](benchmarks.md) explains how these costs differ from a complete agent-task comparison.
The [earlier checklist](earlier-roadmap-checklist.md) retains the remaining original-import conditions.
Broader startup analysis is [outside the active queue](../PLAN.md#work-outside-the-active-queue) unless a named code-change or proof task needs it.
