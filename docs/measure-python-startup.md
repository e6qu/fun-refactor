# Does saving Python setup make later questions cheaper?

You can save checked Python setup before asking about another function.
Saving work also costs time: the client writes a record, reopens it and checks whether it still applies.
This measurement compares those costs with checking the same questions from scratch.

A **baseline** is the alternative used for comparison.
Here, the baseline starts a fresh check directly, without opening a saved record.
**Overhead** means extra work relative to that baseline.
For example, reading a saved answer can cost more than recomputing a very small answer.

## The question and the three ways to answer it

The disclosed example has these two functions:

```python
def forwards():
    return sink(relay(source()))

def stopped():
    return sink(discard(source()))
```

`source()` represents submitted input. `sink(value)` represents an output receiving that value.
The supplied rules tell the checker to follow input from that source to that output.
`relay(value)` returns its argument. `discard(value)` initially returns zero.
The question is whether submitted input can reach the output.

| Approach | What happens for each question |
| --- | --- |
| Fresh check | Find the function and ask `fr` to analyze it directly |
| Saved setup | Find the function, reopen checked declarations and startup writes, then analyze the selected function again |
| Saved answer | Find the function, request its current input identity, then analyze it or validate a saved whole answer |

The input identity records the function, source files, rules and work limits that determine an answer.
The saved-answer client requests that identity through a separate native call, including when its store is empty.
The fresh baseline makes no such preliminary request.
Every approach supplies its rules, context, import setting and work limits explicitly.

## What runs

Each approach follows the same seven requests:

1. Ask about `forwards()` with an empty store.
2. Ask about `stopped()`, which needs a different answer.
3. Ask about `stopped()` again, without editing any source.
4. Change `discard(value)` to return `value`, then ask about `stopped()`.
5. Add an unknown startup call, then ask about `forwards()`.
6. Ask that unfinished question again.
7. Restore the original files, then ask about `stopped()`.

The last request can find an older whole answer still in its store.
The saved-setup client discarded its entry after the unfinished requests and must prepare again.
This difference appears in the results.

Four examples exercise different costs:

| Example | Files and declarations | Work limit |
| --- | --- | --- |
| Local helper | One Python file, including eight unused functions | 4,096 analysis steps |
| Imported helper | The caller imports a second file containing the same helpers | 4,096 analysis steps |
| Many declarations | One Python file, including 200 unused functions | 4,096 analysis steps |
| Unfinished question | The local example with too little work to finish | One analysis step |

An analysis step is one unit of work counted by the checker, rather than a CPU instruction.
All requests allow call depth eight and at most one MiB of output.
One MiB means 1,048,576 bytes.
Import limits remain 16 files, 256 KiB of source and 128 searches.
The complete starting files come from [the example generator](../tests/agent-eval/python-startup-cost/corpus.py).
These examples do not measure complete third-party package imports.

Each request starts a new Python process and fresh native processes.
Saved data passes between requests through a directory on disk.
The measurement repeats each sequence five times and rotates the approach order between repetitions.
It keeps every sample and reports the median, minimum and maximum.
The **median** is the middle of the five values after sorting them.
Whole-sequence totals include the first population request and all six later requests.
Their medians come from per-sequence sums; summing the seven separate medians can give a different number.

## What the timers include

The outer timer starts before launching Python and ends after reading its answer.
It includes:

- Starting Python, importing the client and finding the selected function.
- Starting `fr`, checking files, resolving imports, parsing and analyzing or validating the answer.
- Reading saved records, writing new records and saving the pointer used by the next process.
- Client validation, report serialization and the measurement's worker reply.

Compilation, checkout, dependency installation and fixture edits happen outside those timers.
Store inventories and answer comparisons also happen outside them.
The operating system's file caches stay in their observed state; the experiment does not flush them.
An empty saved-data store therefore does not mean an empty operating-system cache.
Every native call explicitly disables the persistent project cache with `--no-cache`.
Results describe that setting; they cannot predict costs with the project cache enabled.
The outer timer also includes the shared measurement imports and counters.

Native time includes parsing and file checks, but the tool has no separate timers for those operations.
The report cannot assign each operation an individual cost.
Native-call counts include finding the function and the saved-answer client's preliminary request.
Canonical JSON bytes measure the equivalent compact report, rather than exact bytes on the process pipe.

Storage counters show repeated reads, offered writes and newly retained bytes separately.
An offered write can verify an existing immutable object without adding another file.
Temporary reuse files contribute to elapsed time but do not remain in the saved-object inventory.
The small manifest pointer also contributes time; object-byte totals exclude that pointer.
Memory figures show separate lifetime peaks for the Python worker and its largest native child.
They do not measure aggregate simultaneous memory use.

## How correctness limits the comparison

Every saved answer must match the direct fresh answer for the same files, question and limits.
The comparison preserves source locations, unfinished reasons, witnesses and the full analysis input identity.
It removes execution metadata describing how the answer was obtained.
A **witness** is a reported path carrying submitted input to the selected output.
Unknown startup calls and exhausted work budgets must keep their unfinished explanations.
No timing threshold decides whether a check passes.

Ordinary Python also runs each selected function with `source()` returning 17 and `sink(value)` returning its argument.
It observes 17 for forwarding, zero for discarding, and a `NameError` for the unknown startup call.
The edited helper returns 17.
Those executions check one concrete input. The analyzer asks a different question about possible input flow.
Their elapsed times provide context, rather than an interchangeable baseline for analysis.

## Reproduce on GitHub

Open **Actions**, select **Python saved setup costs**, choose the branch, and run the workflow with retention disabled.
The runner installs stable Rust and Python 3.14, then builds the current native executable.
It runs the rejection controls, measures the requests and independently audits the saved result.
Download the `python-startup-cost` artifact to inspect `result.json` and the per-request log.
The artifact also preserves completed requests if a later request fails.

These are the measurement commands from that workflow, after compilation:

```sh
python3 -B tools/python-startup-cost.py --fr target/debug/fr --repetitions 5 \
  --output "$RUNNER_TEMP/python-startup-cost"
python3 -B tools/python-startup-cost.py --audit "$RUNNER_TEMP/python-startup-cost"
```

Run them from the repository root on the runner. The output directory must not already exist.
The audit prints that every request, reuse decision, budget and Python result passed its checks.
The executable uses the development build with debug symbols disabled, as recorded in the workflow.
Results from that build cannot establish optimized-release performance.
The report records the runner's CPU, available CPU set, visible memory, operating system and compiler version.
Measurements from one runner cannot establish costs across different machines.
Source identities in the report bind the analyzer, client, examples, measurement and rejection controls.
Changing those inputs requires a new collection.

This comparison cannot establish savings for an unfamiliar project or for an agent completing a repair.
Use measured whole-request and whole-sequence costs before choosing another optimization.
