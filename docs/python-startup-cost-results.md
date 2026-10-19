# Saving Python setup did not reduce total time in these examples

This is the earlier development-build collection, with project caching disabled.
The [optimized-build follow-up](python-optimized-cost-results.md) now tests project caching both off and on and records separate native-operation profiles.

The [hosted comparison](https://github.com/e6qu/fun-refactor/actions/runs/39153927428) asked the same seven questions three ways.
It compared direct fresh checks, saved setup and saved whole answers.
Every saved result agreed with the direct fresh answer, including unfinished questions and changed files.
Neither reuse approach reduced the median total time for any of the four sequences.

The [method and runnable examples](measure-python-startup.md) explain the questions, work limits and costs.
The [raw result](../tests/agent-eval/results/2026-10-18-python-startup-cost/result.json) preserves all 420 requests, their answers and timing ranges.
Eighteen rejection controls passed before collection. An independent audit checked the saved result afterward.

## Total time for seven questions

Each number below is the median of five complete sequence totals, in seconds.
Totals include the initial request, a different function, a repeated question, an edit, two unfinished requests and restored source.
The percentage compares each approach with direct fresh checks using unrounded values.

| Example | Direct fresh checks | Saved setup | Extra total time | Saved whole answers | Extra total time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Local helper | 3.384 | 3.443 | 1.8% | 4.405 | 30.2% |
| Imported helper | 3.457 | 3.503 | 1.3% | 4.492 | 29.9% |
| 200 unused declarations | 3.806 | 3.914 | 2.8% | 5.092 | 33.8% |
| One-step unfinished question | 3.296 | 3.321 | 0.8% | 4.242 | 28.7% |

Calculate the difference as `(saved total / fresh total - 1) × 100`.
Saving setup reduced its separate preparation count, but the full sequence still took longer in this run.
It still checked files, resolved imports when enabled, parsed source and answered the selected question.
Small individual differences overlap the recorded timing ranges. Five repetitions on one host cannot establish a general performance difference.

## Repeating the same question still has costs

For the local example, the third request asks about `stopped()` for the second time.
The function returns zero, so all three approaches report no path carrying submitted input to the output.
The table separates that request from the initial population cost.

| Third request | Median elapsed time | Observed range | Native process starts | Function analysis steps |
| --- | ---: | ---: | ---: | ---: |
| Direct fresh check | 0.479 s | 0.474–0.490 s | 2 | 21 |
| Saved setup | 0.486 s | 0.474–0.515 s | 2 | 21 |
| Saved whole answer | 0.633 s | 0.628–0.647 s | 3 | 0 |

One native call finds the function. A direct fresh check or saved-setup request then makes one analysis call.
The saved-answer client first asks for the current input identity, then makes a separate call to validate the saved answer.
Reusing the whole answer removes function analysis steps while retaining that extra native call.
Combined native time was 0.285 s fresh, 0.277 s with saved setup and 0.420 s with the saved answer.
These timers include file checks, parsing, import work and client reply parsing; they cannot isolate each operation's share.

Storage also remains active during reuse:

| Third request | Object reads | Offered object writes | Object-store time | Newly retained object bytes | Canonical native reply bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Direct fresh check | 0 | 0 | 0 ms | 0 | 22,866 |
| Saved setup | 16 | 7 | 1.55 ms | 0 | 25,489 |
| Saved whole answer | 20 | 9 | 5.77 ms | 24,992 | 26,799 |

An offered write can check an existing object without adding bytes.
The first whole-answer reuse saves the validated report with changed execution metadata, which creates additional objects here.
The saved-setup reply carries its checked declarations again, even when preparation was reused.
Neither a zero analysis-step count nor an unchanged store size means a free request.
Reply sizes use compact canonical JSON; they do not measure exact pipe traffic.

For the first local question, elapsed medians were 0.489 s fresh, 0.491 s with saved setup and 0.627 s with saved answers.
The latter two approaches initially retained 3,115 and 43,415 object bytes respectively.
The one-step case retained only empty-cache metadata: 349 setup-cache bytes or 347 answer-cache bytes.
Its unfinished answers never became reusable entries.

## What ordinary Python establishes

Python returned 17 through the forwarding helper and zero through the original discarding helper.
After the edit, the discarding helper returned 17.
The unknown startup call raised `NameError`; the analyzer kept that question unfinished.
The one-step analysis also stayed unfinished, even where ordinary Python finished its concrete execution.

The 28 ordinary Python observations took 0.018–0.026 s each, including interpreter startup and result output.
Each observation ran once with one concrete input.
The analyzer checked possible input flow and produced explanatory reports under its stated limits.
Those tasks differ, so their elapsed times cannot establish a translation overhead or an equivalent checking speedup.

## Scope and next decision

The runner used four available Intel Xeon Platinum 8573C CPUs, Python 3.14.8 and Rust 1.99.1 on Linux.
It saw about 15.6 GiB of memory. The native executable used the development build with debug symbols disabled.
Every native call disabled the persistent project cache. Operating-system file caches remained active.
Timers include Python/client startup, finding functions, native calls, storage and the shared measurement machinery.
They exclude compilation, installation, fixture edits, object inventories and answer comparisons.

The raw memory fields are separate lifetime high-water marks, rather than simultaneous aggregate memory.
Every Python worker reported the same 120,082,432-byte peak; those readings do not establish memory savings between approaches.
Native-child peaks ranged from 40,075,264 to 44,126,208 bytes.

These small disclosed examples establish correct reuse and visible overhead under this earlier setup.
The [follow-up](python-optimized-cost-results.md) now measures optimized requests with project caching off and on.
It preserves source checks, import checks and unfinished answers while locating those costs.
Full-package acceptance and the stopped live-agent studies retain their existing limits.
