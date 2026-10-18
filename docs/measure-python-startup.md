# Does saving Python setup make later questions cheaper?

You can save checked Python setup before asking about another function.
Here, setup means declarations, default values and checked statements that run before the function call.
For example, a file can create a dictionary and fill it by calling `register()` before either helper runs.
Saving work also costs time: the client writes a record, reopens it and checks whether it still applies.
This measurement compares those costs with checking the same questions from scratch.

The [earlier development-build result](python-startup-cost-results.md) found no reduction in total time for these four examples.
The current measurement uses an optimized executable and tests project caching both off and on.
Rust's release settings spend more compilation time producing an optimized executable that aims to run faster.

A **baseline** is the alternative used for comparison.
Here, the baseline starts a fresh check directly, without opening a saved record.
**Overhead** means extra work relative to that baseline.
For example, reading a saved answer can cost more than recomputing a very small answer.

## The question and the three ways to answer it

Illustration from `app.py`; the example generator supplies the complete files:

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
A native call starts the compiled `fr` executable separately from Python.
A process means one running program.

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
All requests allow eight nested function calls and at most one MiB of output.
That nesting limit is the call depth.
One MiB means 1,048,576 bytes. One KiB means 1,024 bytes.
Import limits remain 16 files, 256 KiB of source and 128 searches.
The complete starting files come from [the example generator](../tests/agent-eval/python-startup-cost/corpus.py).
These examples do not measure complete third-party package imports.

Each request starts a new Python process and fresh native processes.
Saved data passes between requests through a directory on disk.
The ordinary command-line measurement repeats each sequence five times for each project-cache setting.
That makes 840 requests. A separate diagnostic measurement repeats both settings three times, adding 504 requests.
It uses the existing profiling program: a program that records time spent in individual operations.
The diagnostic times help locate work; they do not substitute for ordinary command-line latency.
The measurement rotates approach order and reverses cache-setting order between repetitions.
It also reverses whether the ordinary or diagnostic sequences run first.
Five repetitions cannot divide evenly between two starting orders or three approaches; the raw rows retain their positions.

It keeps every sample and reports the median, minimum and maximum.
The **median** is the middle value after sorting the samples.
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
Each sequence explicitly chooses whether to use the persistent project cache.
That cache remembers functions discovered in files and which declarations their names refer to.
It reuses those facts between native calls, separately from saved setup or saved answers.
Each approach and repetition has its own project-cache directory outside the project being scanned.
It starts empty. The first function lookup populates it inside the timed request; there is no free preliminary fill.
The next native call can reuse those facts, including within that first request.
Edits require facts for the changed file to be collected again. Restoring original files can reuse their earlier facts.
Both cache-file growth and the saved-answer object store appear separately in the report.
The outer timer also includes the shared measurement imports and counters.

Ordinary native-call timers separate finding the function, requesting its input identity and answering.
The separate profiling sequences divide each call into scanning, indexing, constructing the project, answering,
verifying files and serializing the reply. Indexing includes loading or extracting facts and resolving references.
Project-construction timers further separate source reads, source identities and declaration/reference identities.
An identity here means the content-derived value used to detect changed inputs.
The answering phase includes parsing Python, resolving Python imports, and analysis or saved-answer validation.
Those operations still have no individual timers, so their separate costs remain unknown.
Profiler phases exclude process startup, argument parsing and writing its outer diagnostic reply.
Subtracting those phases from elapsed time does not isolate operating-system process startup.
The ordinary CLI also packages an identity for the project behind the answer.
The profiler omits that packaging and adds diagnostic output. Its totals therefore cannot stand in for CLI request times.
Native-call counts include finding the function and the saved-answer client's preliminary request.
Canonical JSON bytes measure the equivalent compact report, rather than exact bytes on the process pipe.
Canonical JSON uses consistent key order and compact punctuation, giving equivalent data the same representation.

Storage counters show repeated reads, offered writes and newly retained bytes separately.
An offered write can verify an existing immutable object without adding another file.
An immutable object's contents stay fixed; saving it again checks the existing contents.
Temporary reuse files contribute to elapsed time but do not remain in the saved-object inventory.
The small manifest pointer also contributes time; object-byte totals exclude that pointer.
Memory figures show separate lifetime peaks for the Python worker and its largest native child.
They do not measure aggregate simultaneous memory use.

## How correctness limits the comparison

Every saved answer must match the direct fresh answer for the same files, question and limits.
Every answer must also agree across cache settings and between the ordinary executable and the profiler.
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

Open **Actions** and select **Optimized Python request costs**.
Choose the branch containing the code you want to measure.
The runner installs stable Rust and Python 3.14.
It runs the rejection controls, builds the release executable and existing profiler, then passes those binaries to the measurement job.
Compilation and measurement have separate bounded jobs; their costs are outside the per-request timers.
It measures the requests and independently audits the saved result.
Download the `optimized-python-startup-cost` artifact.
Its four directories are `cli-disabled`, `cli-enabled`, `profile-disabled` and `profile-enabled`.
Each contains `result.json` and the per-request log. Here, `cli` means the ordinary command-line executable.
The artifact also preserves completed requests if a later request fails.

These are the measurement commands from that workflow, after compilation:

```sh
python3 -B tools/python-startup-cost.py --fr target/release/fr \
  --profiler target/release/examples/project-profile \
  --repetitions 5 --profile-repetitions 3 \
  --output "$RUNNER_TEMP/python-startup-cost"
python3 -B tools/python-startup-cost.py --audit-set "$RUNNER_TEMP/python-startup-cost"
```

Run them from the repository root on the runner. The output directory must not already exist.
The final audit prints that optimized CLI requests and separate profiles agree with project caching off and on.
The build command is `cargo build --locked --release --bin fr --example project-profile`.
It keeps the normal language features. The executable hashes appear in the report.
The report records the runner's CPU, available CPU set, visible memory, operating system and compiler version.
Measurements from one runner cannot establish costs across different machines.
Source identities in the report bind the analyzer, client, examples, measurement and rejection controls.
Changing those inputs requires a new collection.
The existing evidence policy ignores distribution version labels when the implementation stays the same.
The older development-build report remains historical. Audit it with its original code from
[the recorded source](https://github.com/e6qu/fun-refactor/tree/24decae109f5721f9bdc1df520aebec29336d240),
using that source's `--audit` command. The current reader accepts the current report fields only.
The two collections run on different occasions with different instrumentation; their difference is not a controlled estimate of compilation's effect.

This comparison cannot establish savings for an unfamiliar project or for an agent completing a repair.
Use measured whole-request and whole-sequence costs before choosing another optimization.
