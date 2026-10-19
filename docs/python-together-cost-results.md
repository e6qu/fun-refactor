# Two helper questions together saved a little time in these examples

Checking two local helpers separately took a median of 0.444 seconds across seven pairs of questions.
Asking about both helpers in one request took 0.429 seconds, about 3.2% less.
Both approaches kept one installed Python client running and included its first start and final shutdown.
The combined approach made fewer native calls, but returned more report text.

For example, `relay(value)` returns its argument and `discard(value)` returns zero.
Submitted input can reach an output through `relay`; the zero return stops it through `discard`.
After editing `discard` to return its argument, both answers must show the possible connection.
Every approach noticed that edit and kept unknown startup code unfinished.
Startup is code that runs while Python loads a file, before the selected helper is called.

The [hosted collection](https://github.com/e6qu/fun-refactor/actions/runs/39238028286) passed 16 corruption controls, all 40 measured Python sessions and a separate audit.
It checked 280 question pairs, 560 function answers and 980 calls to the compiled `fr` program.
The [method](python-together-cost.md) introduces the examples, settings and timing boundaries.
The [unchanged report and original archive](../tests/agent-eval/results/2026-10-19-python-together-cost/) retain every sample, search reply, analysis reply and the installed package.

## Seven pairs, including setup inside the Python program

The baseline finds both functions and requests two fresh answers in one running installed Python client.
The compared approach finds the same functions and uses the existing `analyze_together` operation.
Both use the same optimized `fr` executable and start each sequence with an empty project cache.
A project cache saves facts extracted from files. Its first fill counts toward both totals.
These examples save no checked Python preparation or whole answers between requests.

The sequence repeats a pair, reverses its order, edits the helper, adds unknown startup code and restores the original files.
Each entry is the median of five complete sequence times: the middle value after sorting the five samples.

| Example | Separate questions, seconds | Together, seconds | Lower total relative to separate |
| --- | ---: | ---: | ---: |
| Local helpers | 0.444 | 0.429 | 3.2% |
| Imported helpers | 0.480 | 0.467 | 2.7% |
| 200 unused declarations | 0.526 | 0.479 | 9.0% |

Percentages use unrounded medians: `(1 - together / separate) × 100`.
Full answers agree across approaches and repetitions for all three examples, including the unfinished startup questions.
Only records of how each command ran and its enclosing project identity leave that comparison.
The answer contents, dependencies, limits and work counts stay in it.

These are modest differences in disclosed examples, rather than a general speed claim.
All five local separate totals were 0.461500, 0.443283, 0.443654, 0.447568 and 0.442857 seconds.
Together, they were 0.424119, 0.429713, 0.429327, 0.428600 and 0.436403 seconds.
The original samples remain unchanged; no slower observation was dropped or replaced.
Five repetitions on one host cannot predict savings across other programs or complete agent tasks.

## The combined answer costs more text

Both approaches find both functions for every pair.
Separate questions then start `fr` twice for analysis; together starts it once.
That reduces native calls from 28 to 21 over seven pairs.
A native call means one start of the compiled executable.
It does not reduce all work by one quarter: Python still loads, searches still run and both answers still need validation.
The shared startup record also adds report fields.

These are compact JSON bytes reconstructed from all parsed native reports, including searches.
JSON is a structured text format. These counts exclude surrounding protocol text and do not measure agent-visible conversation, raw pipe traffic or memory.

| Example | Separate report bytes | Together report bytes | Additional bytes |
| --- | ---: | ---: | ---: |
| Local helpers | 450,028 | 479,519 | 29,491 |
| Imported helpers | 508,808 | 552,901 | 44,093 |
| 200 unused declarations | 452,251 | 498,556 | 46,305 |

The extra text ranges from about 6.6% to 10.2% relative to separate reports.
For the repeated local pair, the median complete reply time was 27.1 milliseconds separately and 24.7 milliseconds together.
One millisecond is one thousandth of a second.
The native-call timers were 19.2 ms and 15.9 ms, respectively.
These timers locate work but do not isolate the cost of each client check or output operation.
The roughly 100 ms first Python start and 15 ms shutdown still count in the complete sequence.

## An exhausted limit changes the comparison

Each separate question receives 4,096 steps and up to 1,048,576 response bytes.
A step is one unit of analysis work counted by the checker.
The combined request shares that same allowance between both answers.
The three examples above finish their supported work within the shared allowance.

The fourth example permits only one step.
Separately, each function receives and uses one step.
Together, the first uses the available step and the second receives zero.
Both stay unfinished, but their recorded work and second answers differ.
Reversing the questions reverses which function gets that first step.

| One-step example | Separate, seconds | Together, seconds |
| --- | ---: | ---: |
| Seven pairs, with different work allocations | 0.391 | 0.360 |

These times do not establish savings for equivalent answers.
The original reports preserve that difference instead of removing work fields to make the answers match.
The output allowance also differs: one combined reply shares the space that each separate reply could use.
The timed examples do not approach that output limit; an existing SDK test checks visible refusal when a combined reply exceeds it.

## Costs outside the recurring total

The recorded total includes the first Python start, imports, both searches, every analysis, client validation, measurement code, writing and reading replies, and shutdown.
The measurement controller changes the example files and saves progress between timed requests.
Its separate session wall time includes those operations and loaded-file checks:

| Example | Separate wall time, seconds | Together wall time, seconds |
| --- | ---: | ---: |
| Local helpers | 0.458 | 0.444 |
| Imported helpers | 0.496 | 0.484 |
| 200 unused declarations | 0.541 | 0.494 |
| One-step unfinished answers | 0.399 | 0.368 |

The common one-time setup cost 0.665 seconds to build the Python package archive, 0.365 seconds to install it and 0.214 seconds to compile saved Python instructions.
Those three costs total 1.244 seconds and sit outside recurring totals for both approaches.
The separate optimized Rust build took 2 minutes 29 seconds.
Runner preparation, interpreter setup, build dependencies, the controller's own startup and ordinary-Python correctness checks also sit outside the session timers.
This does not establish complete environment setup cost or a universal point where installation pays for itself.

The runner exposed four CPU threads from an AMD EPYC 9V74 and about 15.6 GiB of memory.
It used Python 3.14.8 and Rust 1.99.1 on Linux.
The collector verified all 31 client source files and their checked-hash bytecode before and after collection.
Bytecode is saved compiled Python instructions; checked-hash bytecode verifies the source contents before loading those instructions.
Every process also recorded the installed client files it loaded.
Operating-system file caches and standard-library bytecode remain uncontrolled.

This sequence asks seven pairs, while the [earlier process experiment](python-process-cost-results.md) asked seven individual questions on another runner with different instrumentation.
Comparing absolute totals across those experiments cannot isolate a code improvement or regression.
No analyzer or client behavior changed here.
The ordinary-Python runs check the examples' returned values; they do not compare Python execution speed with analysis speed or measure translation overhead.

## The failed first audit remains visible

The [first collection](https://github.com/e6qu/fun-refactor/actions/runs/39233464634) finished its questions, then failed the audit.
Rules have a fingerprint, an identifier calculated from the exact bytes of their record.
The audit calculated that identifier with the fields in the wrong order.
It also left the field limiting nested calls (`call_depth`) out of its expected work-limit record.
Both mistakes belonged to the measurement code.
The correction checks the native format and adds a regression using the preserved real reply.
The [original failed archive and explanation](../tests/agent-eval/results/2026-10-19-python-together-cost/failed-run.txt) remain beside the successful collection.
Its samples and source bindings were not rewritten or used for the passing timing table.

## What this changes for the next work

The existing two-helper operation modestly reduced elapsed totals here while increasing report text and sharing the work allowance.
It is an available choice when both questions fit those limits.
The experiment used the existing analyzer and client.

Return to the [real package startup gaps](python-package-startup-results.md).
For example, importing Toolz still builds an argument table using loops and imported code that examines functions' arguments.
The checker does not yet understand that work.
The next useful outcome is a complete helper answer through its original package import, with that startup accounted for and the existing limits preserved.
Use the [import explanation](explain-python-imports.md) to choose the next concrete statement to support and compare its behavior with Python.
Full package support, memory savings and cheaper complete agent tasks remain unproven.
