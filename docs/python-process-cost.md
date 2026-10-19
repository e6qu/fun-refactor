# Does keeping Python running make questions cheaper?

Suppose you ask whether submitted input reaches an output through a helper.
Then you ask about another helper, repeat the question and edit the helper's return value.
Starting Python for every question repeats interpreter and client loading work.
Keeping one Python program running might avoid that work. It must still notice the edit.

The Python client library, also called the SDK, starts `fr` and checks its replies.
The [earlier measurement](python-optimized-cost-results.md) loaded SDK source in a fresh Python process for every question.
It included compiling that source into instructions for Python.
Those instructions are **bytecode**. Python can save them in `.pyc` files for later starts.
That earlier total does not establish the cost of loading an installed SDK with saved bytecode.

This experiment compares four ways to ask exactly the same questions:

| Approach | SDK loading | Python processes for seven questions |
| --- | --- | --- |
| Source, fresh | Compile the SDK source on each start | Seven |
| Installed, fresh | Load an installed wheel with saved bytecode | Seven |
| Source, running | Compile the SDK source once | One |
| Installed, running | Load the installed wheel once | One |

A **wheel** is a Python package archive that an installer can unpack.
The experiment builds that archive from the current SDK source, installs it and compiles its modules before measuring questions.
It records those three setup costs separately. They still matter when deciding whether installation pays for itself.
Both approaches use the same Python interpreter. This does not compare different virtual environments.

Each question finds its function and requests a fresh analysis through the ordinary optimized `fr` executable.
Every question starts two native processes, even when Python stays running.
Every sequence starts with an empty project cache in its own directory, then retains that cache through later questions and edits.
A project cache saves facts extracted from files. This experiment saves no Python preparation or whole answers.

## The questions and expected answers

The [example generator](../tests/agent-eval/python-startup-cost/corpus.py) supplies the complete files and rules.
Its `source()` represents submitted input; `sink(value)` represents an output receiving a value.
`relay(value)` returns its argument. `discard(value)` initially returns zero.
The rules ask the checker to follow input from the source to the sink.

1. Ask about `sink(relay(source()))`: input can reach the output.
2. Ask about `sink(discard(source()))`: the zero return stops that input.
3. Repeat the second question without changing any file.
4. Change `discard(value)` to return `value`: the answer must change.
5. Add `opaque()` during file startup: the checker must leave the answer unfinished.
6. Repeat that unfinished question.
7. Restore the original files: `discard` stops the input again.

Four examples contain local helpers, imported helpers, 200 unused declarations or an intentionally exhausted work limit.
The first three allow 4,096 analysis steps; the unfinished example allows one.
An analysis step is one unit of work counted by the checker.
All allow eight nested calls and 1,048,576 output bytes, with context `html` and summaries enabled.
The imported example enables import checking; the others disable it.
Import limits remain 16 files, 256 KiB of source and 128 searches.

The collector repeats every sequence five times and rotates the four approaches' order.
That makes 560 questions and 1,120 native requests.
Five repetitions cannot balance four starting orders exactly. Every raw row keeps its position.
Full semantic answers must agree across SDK copies, process choices and repetitions.
Only execution accounting and the enclosing command's project identity leave that comparison.
Sources, rules, work limits, unfinished reasons and dependency records remain in it.

## Which costs count?

The recurring total includes each Python start through its ready reply, every submitted question through its parsed answer, and stopping Python.
It counts SDK loading, finding functions, native startup, source checks, analysis, SDK validation, measurement work and final JSON replies.
The first request populates the project cache inside that total. There is no earlier analysis to warm it.

The controller changes files between questions and saves evidence outside those timed intervals.
It also records elapsed wall time, which includes that extra controller work.
Adding deliberate waiting between questions would change wall time without changing the measured work total.
The collector neither flushes nor controls operating-system file caches.

Separate worker timers cover ordinary imports, reading command options, loading the SDK and answering each question.
The parent startup timer also includes interpreter startup, output transfer and scheduling.
Subtracting worker bootstrap time leaves those costs mixed together; that remainder cannot establish pure interpreter or pure SDK overhead.
This worker imports its small measurement code separately from the controller.
Its timings cannot isolate a change from the earlier, differently instrumented experiment.

Both copies run with `-I -B`.
`-I` isolates Python from user import paths and environment settings.
`-B` prevents writing bytecode; Python can still read existing bytecode.
The collector rejects SDK bytecode in the source copy.
For the installed copy, it checks every module's bytecode header and source hash before and after collection.
**Checked-hash bytecode** tells Python to verify the source contents before accepting the saved instructions.
Each worker records its actual loaded SDK paths. Loading repository source in the installed condition fails the collection.
Existing standard-library bytecode remains available in both approaches.

## Run and inspect it

Use the **Installed and running Python costs** GitHub workflow on the revision under review.
Builds, tests and evidence collection belong on the hosted runner under this project's local resource policy.
The runner installs Python 3.14, `build==1.6.1` and `setuptools==84.0.0`, then builds `fr` with release settings.
The collection step runs:

```sh
python3 -B tools/python-process-cost.py --fr target/release/fr \
  --repetitions 5 --output "$RUNNER_TEMP/python-process-cost"
python3 -B tools/python-process-cost.py \
  --audit "$RUNNER_TEMP/python-process-cost/result.json"
```

`RUNNER_TEMP` names the temporary directory supplied by GitHub Actions.
The collector creates a new output directory; it refuses to overwrite an earlier attempt.
The workflow checks source and binary identities, runs negative controls, collects all requests and audits the saved result again.
The `python-process-cost` artifact contains the result, original wheel and progress logs.
Progress logs survive a failed collection so a later retry cannot hide the first failure.

Compare median seven-question totals with **Source, fresh**, the baseline.
The median is the middle of five sequence totals after sorting them.
Report installation overhead separately and keep every individual sample available.
These small prescribed questions cannot establish lower agent costs, complete package support or translated-program speed.
