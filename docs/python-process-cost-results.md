# Keeping Python running helped these repeated questions

These completed measurements describe the checker before bounded startup loops.
CI audits them with their unchanged source and SDK; they do not measure the later loop-support change.

For seven questions about a local helper, starting Python from SDK source each time took a median of 1.212 seconds.
Installing the same client with saved compiled modules reduced that total to 0.888 seconds.
Keeping the installed client running for all seven questions reduced it to 0.280 seconds, including its first start and final shutdown.

For example, `discard(value)` initially returns zero, so submitted input cannot pass through its return value.
Change it to return `value`, then ask again. Every approach must notice that edit.
All 560 questions returned matching answers, including edits, unknown startup calls and exhausted work limits.
An unfinished check still reported an unfinished answer.

The [hosted collection](https://github.com/e6qu/fun-refactor/actions/runs/39211486634) ran 80 complete sequences and 1,120 native requests.
It started 320 Python processes. Nineteen negative controls passed before collection, and the saved report passed an independent audit afterward.
The [method](python-process-cost.md) introduces the examples, process choices and work limits.
The [unchanged report and original artifact](../tests/agent-eval/results/2026-10-19-python-process-cost/) retain every sample and the original installed package.

## Seven-question totals, including the first start

The baseline loads the Python client library, or SDK, from source in a fresh Python process for every question.
The installed copy contains the same source plus saved bytecode: instructions that Python can load without compiling the module again.
Both copies use the same interpreter and an optimized `fr` executable.
Each sequence starts with an empty project cache and retains it through later questions and source edits.
Every question still finds its function, checks files and requests fresh analysis.

The entries below are median whole-sequence seconds from five repetitions.
The median is the middle value after sorting those five totals.

| Example | Source, fresh Python | Installed, fresh Python | Source, one running Python | Installed, one running Python |
| --- | ---: | ---: | ---: | ---: |
| Local helper | 1.212 | 0.888 | 0.340 | 0.280 |
| Imported helper | 1.181 | 0.823 | 0.317 | 0.285 |
| 200 unused declarations | 1.426 | 1.000 | 0.404 | 0.358 |
| One-step unfinished question | 1.359 | 0.911 | 0.334 | 0.282 |

Installation reduced fresh-process totals by 26.7–33.0% relative to source with fresh Python.
Using one running source process reduced totals by 71.7–75.4% relative to restarting the source process for every question.
Using one running installed process reduced totals by 74.9–79.3% relative to that source baseline.
Compared with restarting the **installed** client each time, keeping it running reduced totals by 64.1–69.1%.
Each percentage uses the unrounded sequence medians: `(1 - compared total / baseline total) × 100`.

The original samples also show variation. For example, the imported installed-running sequence ranged from 0.267 to 0.562 seconds.
Its five totals were 0.500, 0.267, 0.285, 0.562 and 0.270 seconds; all remain in the report.
The two larger totals include slower native calls when populating the cache and checking changed files.
The experiment does not isolate the cause inside those native calls. No sample or retry replaces them.
The installed-fresh comparison for that example ranged from 0.808 to 0.931 seconds.
Five repetitions on one host cannot predict a general speedup.

## Where the startup time went

For the local example, the 35 fresh source starts reached their ready reply in a median of 133.6 milliseconds.
The 35 fresh installed starts reached it in 82.2 milliseconds.
One millisecond is one thousandth of a second.

| Part of one fresh local start | Source copy, median ms | Installed copy, median ms |
| --- | ---: | ---: |
| Worker imports before reading options | 8.4 | 8.6 |
| Reading command options | 12.9 | 13.0 |
| Importing the SDK | 101.2 | 49.4 |
| Parent startup minus worker bootstrap | 10.6 | 10.8 |
| Stopping Python after the question | 15.4 | 15.4 |

SDK import time includes compiling source in the source condition and checking saved bytecode in the installed condition.
The parent remainder combines interpreter startup, output transfer, scheduling and small uninstrumented gaps.
It cannot establish pure interpreter cost or final-output cost separately.
The entries use separate medians, so adding them need not reproduce the median parent startup time.
The installed condition also changes the SDK's directory; this experiment does not isolate bytecode as the only possible source of difference.

Keeping Python running avoids six of those starts and shutdowns per sequence.
It still launches `fr` twice for every question, once to find the function and once to analyze it.
For the repeated local question, the installed running process took a median of 10.9 ms through the parsed reply.
Its two native calls took 8.9 ms combined; the measured Python body took 10.4 ms.
Those later-question times exclude the initial Python start, which remains in the seven-question total above.

## Setup and other excluded work

This collection spent 0.615 seconds building the wheel, 0.365 seconds installing it and 0.164 seconds compiling checked-hash bytecode.
A wheel is an installable Python package archive.
Checked-hash bytecode makes Python verify source contents before accepting the saved instructions.
These three one-time costs total 1.144 seconds and sit outside the recurring question totals.
Provisioning the runner and interpreter, installing build dependencies and transferring the archive also sit outside those timers.
The separate hosted Rust build took 3 minutes 26 seconds.
These measurements do not establish complete installation cost or a universal number of questions needed to repay it.

The recurring total includes starting Python, imports, command options, both native calls, SDK reply checks, instrumentation, JSON output and shutdown.
The controller edits fixture files, verifies SDK paths and saves evidence between timed intervals.
Elapsed wall time includes those controller operations.
For the local example, median wall totals were 1.240 seconds with fresh source and 0.294 seconds with the installed running process.

The worker uses a smaller measurement program than the [earlier saved-setup comparison](python-optimized-cost-results.md).
The runner also differs. Comparing this page's totals with that earlier collection cannot isolate a code improvement.
Only the four conditions within this collection provide the controlled comparisons above.
The analyzer and SDK implementation did not change for this experiment.

The runner exposed four CPU threads from an AMD EPYC 9V45 and about 15.6 GiB of memory.
It used Python 3.14.8 and Rust 1.99.1 on Linux, with the ordinary optimized `fr` executable.
The collector checked all 31 SDK source files and their installed bytecode before and after collection.
Worker receipts identify the loaded modules and reject loading the source checkout in the installed condition.
Operating-system file caches and existing standard-library bytecode remain uncontrolled.
These are disclosed small examples. They establish no memory saving, complete package support, translation overhead or agent-task cost reduction.

## What to do next

Use an installed client in one running Python program as the next comparison's baseline.
For example, find both `forwards()` and `stopped()`, then ask their questions without restarting Python between them.
Both questions can already use the existing client; this result needs no new saved-work mechanism.

The later [two-helper comparison](python-together-cost-results.md) measures that combined request against separate requests in the same running program.
The [current handoff](continuity.md#next-use-the-continuing-work-cost-results) uses the newer real-project cost results to choose the next improvement.
State the work limits explicitly: a combined request shares its limit, while separate requests each receive their own limit.
Count finding both functions, initial setup, all answers and checking source edits.
Keep exhausted or unsupported questions unfinished, then return to the real libraries' remaining import and startup gaps.
