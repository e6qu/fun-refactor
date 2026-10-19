# Project caching helped; saving Python setup added cost in these examples

These completed measurements describe the checker before bounded startup loops.
CI audits them with their unchanged source and SDK; they do not measure the later loop-support change.

You want a later Python question to take less time without losing checks after an edit.
The [optimized-build comparison](https://github.com/e6qu/fun-refactor/actions/runs/39179055709) measured three ways to answer the same questions.
It checked from scratch, reopened saved setup, or reopened saved whole answers.
It tested each approach with the persistent project cache switched off and on.
The Python SDK is the client library that starts `fr` and checks its replies.

Each ordinary command-line comparison uses five complete runs per condition.
The median is the middle total after sorting those five.

Project caching reduced the median total time for direct fresh checks by 21.9–23.3% in these four examples.
On top of that cache, saved setup added 0.9–4.1% and saved whole answers added 2.7–6.2%.
Neither saved approach reduced the median complete-sequence time in any example or cache setting.
All answers agreed, including edits and unfinished questions.

The [method and examples](measure-python-startup.md) explain the starting files, work limits and included costs.
The [four raw reports](../tests/agent-eval/results/2026-10-19-python-optimized-cost/) preserve every sample and its answer.
There are 840 ordinary command-line requests and 504 separate diagnostic requests.
Thirty-five rejection controls passed before collection; an independent audit checked the finished reports afterward.

## First, what the project cache changes

The project cache remembers functions found in files and which declarations their names refer to.
It saves different work from Python setup, which includes declarations, default values and checked startup writes.
For example, setup can create a dictionary and call `register()` to fill it before either selected helper runs.

Each sequence asks seven questions: a first helper, a different helper, a repeated question, an edit,
two unfinished requests and restored source.
Both cache settings use the same optimized executable and explicit analysis limits.
Every cache starts empty. The first fill, later file checks and invalidation after edits count toward these totals.

These numbers are medians of five whole-sequence times, in seconds.
The baseline is direct fresh analysis with project caching off.

| Example | Project cache off | Project cache on | Less total time |
| --- | ---: | ---: | ---: |
| Local helper | 2.555 | 1.994 | 21.9% |
| Imported helper | 2.609 | 2.006 | 23.1% |
| 200 unused declarations | 2.642 | 2.027 | 23.3% |
| One-step unfinished question | 2.506 | 1.941 | 22.5% |

The last example intentionally has too little analysis work available to finish.
Its quicker reply still says the answer is unfinished.
An empty project cache describes saved project data, not the operating system's file cache, which remains uncontrolled.

## Does another saved record help on top of that?

Here the baseline is direct fresh analysis with project caching **on**.
Each alternative uses the same project-cache setting and answers the same seven questions.
The percentage is `(saved total / fresh total - 1) × 100`, calculated before rounding.

| Example | Direct fresh checks | Saved setup | Extra time | Saved whole answers | Extra time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Local helper | 1.994 | 2.031 | 1.8% | 2.078 | 4.2% |
| Imported helper | 2.006 | 2.043 | 1.9% | 2.100 | 4.7% |
| 200 unused declarations | 2.027 | 2.110 | 4.1% | 2.153 | 6.2% |
| One-step unfinished question | 1.941 | 1.959 | 0.9% | 1.994 | 2.7% |

With project caching off, saved setup added 0.5–3.3% and saved answers added 16.0–19.9%.
The raw reports retain all five totals and every individual request's timing range.
Small differences can overlap those ranges. Five repetitions on one host do not establish a general performance difference.

## A repeated question still starts programs and checks records

The third local request asks about `stopped()` again.
That helper returns zero, so all approaches report no path carrying submitted input to the output.
The project cache is already populated for this request.
A native process here means one separate start of the compiled `fr` program.

| Approach | Median elapsed time | Observed range | Native process starts | Function analysis steps |
| --- | ---: | ---: | ---: | ---: |
| Direct fresh check | 0.258 s | 0.256–0.260 s | 2 | 21 |
| Saved setup | 0.266 s | 0.263–0.267 s | 2 | 21 |
| Saved whole answer | 0.275 s | 0.272–0.276 s | 3 | 0 |

One native process finds the function. Another answers the question.
The saved-answer client also starts a process to request the current input identity before reading its saved answer.
An input identity records the files, function, rules and work limits that determine an answer.
Reusing the whole answer removes function analysis steps while keeping this preliminary request and the record checks.

The object store is the directory of saved records; each object's contents stay fixed.
The ordinary-call timers show these median costs, in milliseconds:

| Approach | Find the function | Request input identity | Answer or validate | Object-store operations |
| --- | ---: | ---: | ---: | ---: |
| Direct fresh check | 5.50 | 0 | 6.70 | 0 |
| Saved setup | 5.48 | 0 | 7.05 | 2.90 |
| Saved whole answer | 5.51 | 5.41 | 6.22 | 7.06 |

These separate medians do not partition the median total request exactly.
Store timing includes the measurement's JSON encoding used to count bytes, alongside actual object reads and writes.
Saved setup performs 16 reads and offers seven writes here; saved answers perform 20 reads and offer nine writes.
Repeated writes can verify existing objects. The first whole-answer reuse also adds 24,992 object bytes for changed execution metadata.

The first local fresh request takes 0.315 s with project caching on.
It creates three project-cache files totaling 1,433 bytes, inside the timed request.
The native call answering that first question can already use the facts saved by its function lookup.
That initial fill remains included in the seven-question comparison.

## Where the diagnostic run found work

A separate profiling program records time spent in individual native operations.
Its three repetitions help locate work; ordinary CLI timings above remain the request comparison.
The table shows medians of phase totals across both native calls for the repeated local fresh question.

| Native operation | Project cache off | Project cache on |
| --- | ---: | ---: |
| Scan files | 1.08 ms | 1.05 ms |
| Index facts and resolve references | 107.95 ms | 0.83 ms |
| Construct the project | 0.62 ms | 0.61 ms |
| Answer the queries | 1.12 ms | 1.20 ms |
| Verify source files | 0.45 ms | 0.43 ms |

Both populated-cache calls reported a fact-cache hit for each of their two indexed files and a resolution-cache hit.
The first empty-cache lookup reported zero fact hits. Edits caused fresh fact extraction for changed files.
The audit checks those observations and requires the answers to agree across cache settings.
Index time includes fact extraction or loading and reference resolution; it does not isolate each operation.
Answering still includes Python parsing, import work and analysis or validation, without separate timers for those parts.

These workers load the SDK from repository source.
Python can save a compiled form of a module, called bytecode, to avoid compiling that source again.
The checkout has no SDK bytecode, and `-B` prevents saving it. Each worker compiles those SDK sources again.
Existing standard-library bytecode stays available.
An installed SDK with saved compiled modules needs a separate comparison.

Another large part lies outside the inner Python worker timer.
For the repeated local fresh request, that gap has a median of about 242 ms, even with project caching on.
It includes interpreter launch, loading and compiling SDK and measurement code, command parsing and final reply output.
It cannot identify the client import cost separately.
The inner worker itself takes about 16 ms in that case; the entire request takes about 258 ms.

The [installed-client follow-up](python-process-cost-results.md) now compares saved bytecode and several questions from one running Python program.
It uses smaller measurement code and another runner, so its controlled comparisons apply within that new collection.
Next measure [asking about two helpers together](check-python-together.md) against separate questions in the same running program with explicit limits.

## Limits and retained unsuccessful work

The runner exposed four CPU threads from an AMD EPYC 7763 and about 15.6 GiB of visible memory.
One GiB is 1,073,741,824 bytes.
It used Python 3.14.8 and Rust 1.99.1 on Linux, with Rust's optimized release build and normal language features.
Timers include shared measurement imports and counters, native calls, client validation and saved-data I/O.
They exclude building the Rust executables, installation, fixture edits, inventories and answer comparisons.
Python source compilation during imports remains included.
Memory counters record separate lifetime peaks for the Python worker and its native children, rather than simultaneous total memory.

Twenty-eight ordinary Python executions checked concrete results once, with submitted input represented by 17.
They took 0.023–0.029 s, including interpreter startup and output.
The analyzer asks about possible input flow, a different task, so those times cannot establish a translation overhead.
The four reports carry the same observations; they do not represent 112 Python executions.

These are disclosed small examples. They establish no savings for unfamiliar packages or an agent completing a repair.
The [earlier development-build collection](python-startup-cost-results.md) ran on another occasion with different instrumentation.
Comparing the two collections cannot isolate compilation's effect.

The [first diagnostic attempt](../tests/agent-eval/results/2026-10-19-python-optimized-cost/first-attempt/collection.json) stopped when the SDK rejected a report missing its required project identity.
Its original artifact preserves 49 completed requests; the adjacent error file preserves the next request's rejection.
The correction supplies the current identity before the same strict SDK checks.
The complete comparison above comes from a fresh collection with every condition repeated.
