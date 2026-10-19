# What do the performance numbers mean?

You want to know whether using `fr` makes a correct repair cheaper or faster.
That requires comparing the same task with and without `fr`, including the work needed to use the tool.
A smaller report is one possible saving. Starting the tool, reading instructions and reviewing extra metadata are possible costs.

This page explains recorded measurements. It does not start a new study.

## First choose the question

A **benchmark** is a repeatable measurement of a stated operation.
Its **baseline** is the alternative used for comparison.
**Overhead** is the extra work introduced by the approach being tested.
For example, building an index costs time even if later searches become faster.

| Question | What to compare | What the comparison must include |
| --- | --- | --- |
| Does an agent fix the bug more efficiently? | The same model and task with ordinary tools, then ordinary tools plus `fr` | Correctness, instructions, all calls and replies, retries, failed attempts, checks and any helper agents |
| Does a Python client send less text to the agent? | The same `fr` operations exposed separately, then performed inside one Python program | The complete Python program, its returned text and the same resulting source or report |
| Does saving an analysis help after an edit? | A fresh analysis, then reuse on the same project and after the same edit | Initial analysis, validation, work repeated, storage, memory and agreement with the fresh answer |
| Does generated code run as fast as the original? | Original and translated functions on equivalent inputs and build settings | Compilation settings, startup, conversions, execution time, memory and matching outputs |
| Does a recipe reduce cleanup work? | The same edits through separate commands, then a recipe | Writing and parsing the recipe, each analysis and edit, review, checks and recovery |

The last two comparisons have no general measured overhead result in this project.
The tables below concern agent use and tool operations. They do not measure the execution speed of translated programs.

## Which “language” is involved?

`fr` is a tool, not a replacement runtime for Python or Rust.
It reads those languages and can write supported code in another language.
The resulting program runs with that language's compiler or interpreter.
We have not established a general runtime cost for that translation.

The [recipe language](../RECIPES.md) describes a sequence of edits.
Its cost includes reading the recipe and performing those edits.
The [structured code model](../IR.md), also called IR, describes supported statements and expressions as data.
JSON records and the Python client carry that data between programs.
Their construction, validation, serialization and process startup take work, even when less text reaches the agent.
Those costs must appear in a speed comparison; byte counts alone do not measure them.

## The complete-task comparison we have

The [recorded two-task experiment](evaluations.md#unknown-target-investigations) used the same Luna model with two tool setups.
The baseline offered ordinary file tools. The other setup also offered `fr`.
Each task has one pair of attempts, so these observations cannot predict a general success rate.

| Task | Baseline: ordinary tools | Ordinary tools plus fr | Observed extra work with fr |
| --- | --- | --- | --- |
| Repair Unicode comparison | Passed; 17 calls; 27,604 visible bytes; 326,494 input tokens; 126.8 s | Passed; 23 calls; 75,165 visible bytes; 618,428 input tokens; 185.6 s | 6 calls; 47,561 bytes; 291,934 input tokens; 58.8 s |
| Add escaping with an allocation constraint | Failed; 22 calls; 59,707 visible bytes; 644,478 input tokens; 223.9 s | Passed; 33 calls; 70,857 visible bytes; 763,904 input tokens; 307.0 s | 11 calls; 11,150 bytes; 119,426 input tokens; 83.1 s; different correctness outcomes |

For the passing bug-fix pair, elapsed time increased by about 46.4% relative to the ordinary-tools attempt.
Calculate a relative difference as `(fr amount - baseline amount) / baseline amount × 100`.
The feature pair compares a success with a failure; its time difference is not the cost of two equivalent successful repairs.

A **token** is a unit of text counted by a model's tokenizer.
Input totals here include cached input: text the provider may already have processed.
They are not dollar totals. Provider prices and cache discounts are not verified billing for these attempts.
Agent seconds include tool work in the recorded phases, rather than isolating the tool's own CPU time.
Bytes measure stored or transferred text size. KiB means 1,024 bytes; MiB means 1,048,576 bytes.

## Smaller messages, with their extra work visible

These experiments run prescribed programs. The programs already know which operations to perform.
They measure data movement, not an agent finding a solution.

| Measurement and source | Baseline | Compared approach | Included cost and missing measurement |
| --- | --- | --- | --- |
| [Python runtime client](agent-runtime-sdk.md#controlled-comparison) | Five exposed requests and replies: 17,689 bytes | Complete program plus result: 1,893 bytes | Includes the 1,471-byte program and 422-byte result. Both still make five internal `fr` calls. Python execution time is not measured here |
| [Keeping report pages in Python](agent-context-workspace.md#controlled-comparison) | Sixteen exposed requests and replies: 59,825 bytes | Complete program plus selected result: 4,550 bytes | Includes the 686-byte program and 3,864-byte result. Both still make sixteen internal calls. Memory and CPU overhead are not quantified here |
| [One native request versus following report pages](agent-runtime-sdk.md#controlled-comparison) | 89 subprocesses; 302,380 internal reply bytes; 3,909-byte final packet | One subprocess; 4,975 internal reply bytes; 4,975-byte final packet | The final packet grows by 1,066 bytes, about 27.3%, because it includes coverage information. Fewer subprocesses alone do not measure elapsed time |
| [Omitting repeated check declarations](project-checks.md#controlled-report-measurement) | Four check replies: 2,720 tokens | The same replies with repeated declarations omitted: 1,804 tokens | Saves 916 reply tokens. Excludes the earlier listing, prompts and the rest of the task. Replays old replies; no new agent ran |

“Internal” means traffic between the Python program and `fr`.
“Exposed” or “agent-visible” means text sent to the agent's conversation.
Moving work inside Python changes that boundary; it does not make the work free.

## Faster parts can still make the whole operation slower

The [cache measurements](evaluations.md#fixed-point-and-reuse-acceptance) include storage and validation costs.
Read each report's actual baseline before interpreting a warm-run improvement.
**Cold** means the relevant saved result is unavailable; **warm** means an earlier result is available for reuse.
An edited run also measures checking which earlier results still apply.
A faster warm run must repay the initial work and added storage before it benefits the whole task.

The [earlier saved Python setup comparison](python-startup-cost-results.md) measures 420 requests across four disclosed examples.
The baseline checks each function directly. The alternatives reopen saved setup or saved whole answers under the same limits.
Median seven-question totals increased by 0.8–2.8% with saved setup and 28.7–33.8% with saved whole answers.
The totals include initial population, finding functions, Python and native startup, file and import checks, parsing and storage.
That development build disabled persistent project caching.

The [optimized follow-up](python-optimized-cost-results.md) adds project caching off and on, with 840 ordinary requests and 504 separate diagnostic requests.
Project caching reduced fresh-check sequence totals by 21.9–23.3% relative to the same optimized executable with caching off.
With project caching on, saved setup added 0.9–4.1% and saved whole answers added 2.7–6.2% relative to fresh checks.
Those totals include the initial cache fill, later questions, edits, unfinished replies and source restoration.

The [installed-client and running-process comparison](python-process-cost.md) separates two remaining questions.
Does saved Python bytecode reduce client loading costs? Does one running Python program avoid repeated startup across several questions?
It counts the initial start, every fresh analysis and process shutdown, and records wheel installation separately.
Its [560-question result](python-process-cost-results.md) found 26.7–33.0% lower fresh-process totals with the installed client.
Keeping that installed client running reduced totals by another 64.1–69.1% relative to restarting it for every question.
The comparisons check the same seven answers after the same source edits; they do not measure complete agent tasks.
Separate native-operation timers locate work; their diagnostic totals do not substitute for ordinary request times.
The roughly 242 ms outside one repeated question's inner worker includes launch, imports and reply handling, without identifying their individual costs.

The [saved cache report](../tests/agent-eval/results/2026-09-25-flow-cache/result.json.gz) contains three repetitions on one host.
The table below derives warm-versus-cold differences from its unrounded median elapsed times for the newer report-chunk format.
The baseline here is fresh analysis using that format, not the older cache format.

| Workload | Fresh analysis, seconds | Warm reuse, seconds | Extra elapsed time relative to fresh analysis |
| --- | ---: | ---: | ---: |
| Pipeline, 8 helpers | 0.451 | 0.482 | 6.8% |
| Pipeline, 16 helpers | 0.574 | 0.709 | 23.5% |
| Imported calls, 8 helpers | 0.410 | 0.413 | 0.7% |
| Imported calls, 24 helpers | 0.480 | 0.504 | 5.1% |
| Recursive calls, 8 helpers | 0.470 | 0.496 | 5.6% |
| Recursive calls, 12 helpers | 0.542 | 0.599 | 10.4% |

A pipeline passes values through successive helpers; imported calls use helpers in other modules; recursive calls can call themselves again.
Times include the worker's measured request and storage work. They are not model response times or whole-task costs.
Operating-system caches were uncontrolled, so these small differences do not establish a production speed distribution.
They also do not isolate how much extra time belongs to validation, storage or another component.

The newer warm stores use about 1.34–2.90 times the older format's stored bytes, calculated from the recorded warm medians.
That is a storage comparison between formats, with a different baseline from the timing table above.
The evidence supports a faster representation of saved reports; it does not show that enabling reuse made these requests faster than fresh analysis.

The [compiler investigation](evaluations.md#python-development-history) measured hashing the same 32 MiB on one runner.
Optimizing the development build changed that operation from 0.830 s to 0.021 s.
That measures hashing, not a 39-fold improvement in all analysis or agent work.
The corrected complete job took 9 minutes 55 seconds under its existing 15-minute limit.

CI duration compares jobs or tests on GitHub runners.
Runner queue time, compilation time and test execution are separate costs.
See [CI measurements](ci.md) for the named baseline runs and retained failures.

## Observing Python package startup

The [package startup investigation](python-package-startup-results.md) compares ordinary Python imports and integer calls with the same work under line tracing.
Its timers include the costs described in the [method](python-package-startup.md#understand-the-comparison-costs).
That comparison measures observation costs. It does not measure `fr` overhead against Python or complete agent-task cost.

## Saving Python test results

You want the Python test job to finish without spending its deadline rewriting results it already saved.
The baseline saves the entire growing report after each case.
The new method writes the new case during collection and saves the full report once after the final audit.
A case is one program with its chosen names and file locations.

The [recorded comparison](../tests/agent-eval/results/2026-10-16-incremental-reports-python-report-writes/result.json)
replays the same 818 retained observations through both saving methods on one GitHub Ubuntu 24 runner.
It uses CPython 3.14.8 and image `20261012.334.1`; the record identifies the processor and exact source inputs.
Each method has one sample, with the baseline first. Filesystem caching and other runner activity remain uncontrolled.

The timer includes JSON encoding, file writes, flushes, adding rows to the list and the final full save.
JSON is the text format holding the structured results; flushing sends buffered text to the file.
Both methods run inside Python, so their serialization and loop overhead appears in these times.
Input loading, result comparison, builds, execution of the example programs, analysis and audits fall outside the timer.

| Saving method | Elapsed seconds | CPU seconds | Bytes written by the process |
| --- | ---: | ---: | ---: |
| Rewrite all earlier cases after each case | 115.694 | 78.373 | 19,344,364,657 |
| Write each new case, then save the full report once | 0.609 | 0.496 | 97,882,761 |

CPU seconds count the process's work; elapsed seconds include time waiting for the operating system.
Both final files contain the same observations and have identical bytes: 48,938,610 bytes each.
The byte counter includes overwritten data. It measures application writes, with filesystem caching and no forced durable-disk synchronization.
It does not measure physical device traffic or storage wear.

The [fresh complete job](https://github.com/e6qu/fun-refactor/actions/runs/38899354891)
passed in 12 minutes 2 seconds under its unchanged 15-minute deadline, including the separate build, execution and audit work.
That job ran separately from this comparison. The saving-only timings establish a cheaper collection step, without measuring faster analysis or cheaper complete agent tasks.
The [CI guide](ci.md#save-python-test-progress-without-repeating-earlier-work) explains readable progress after interruptions and how to rerun the comparison.

## What every new measurement must explain

Start with a user question, such as “Will repeated caller queries save time while I fix checkout?”
Then record:

1. The operation, inputs, expected answer and correctness check.
2. The baseline, including its exact tools and commands.
3. What differs in the tested approach, including extra instructions or setup.
4. The machine, tool versions, build settings, repetitions and cache state.
5. What the timer and counters include, and what they leave unknown.
6. Both raw amounts, units and the denominator for every percentage.
7. Failures, cancellations, timeouts and unstarted attempts alongside successes.
8. The practical limit: a result for this operation, this task or a broader repeated study.

Count setup, review, validation and storage when they are needed to use the result.
If runtime or memory overhead is unmeasured, say so instead of treating it as zero.
The [study guide](agent-study.md) explains the recording format.
Paid studies and stopped pilots require their own authorization; this documentation is not an instruction to restart them.
