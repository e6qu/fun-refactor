# What saved project work costs

You have already found a function and its callers. Does reopening that answer save work after a pause?

This comparison asks the same bounded questions with saved answers and with fresh discovery. Both approaches start with separate empty native caches and retain those caches between requests.

The projects are Rust glob, Python Boltons and Python JMESPath. JMESPath stayed unread until the procedure and optimized binary were fixed. Its first independent evaluation passed without changing the checker after reading its code.

Each project has three repetitions. Each repetition includes first use, unchanged reuse and a source edit. The edit changes occurrences of one function name in one declared file. Both approaches then ask about the original name. All 27 paired answers agree, including warnings about callers left unchecked.

The [comparison guide](saved-work-cost.md) explains the questions, extra work and measurement limits.

## Request time

Times below are medians in milliseconds. Each request includes a new Python process and SDK imports. Saved requests include restoring, checking and writing their plan and answer files.

Both approaches also hash source files and write diagnostic records. Those measurement costs remain in request totals.

| Project | First fresh | First saved | Repeat fresh | Repeat saved | Edited fresh | Edited saved |
|---|---:|---:|---:|---:|---:|---:|
| glob | 398.4 | 1859.0 | 251.7 | 1430.8 | 290.8 | 420.1 |
| boltons | 1442.4 | 1904.3 | 408.5 | 491.3 | 615.2 | 777.5 |
| jmespath | 601.2 | 2812.8 | 388.5 | 1803.3 | 407.4 | 1934.1 |

## Complete three-question session

The session includes its source copy, all three requests, cleanup and the measured edit through `fr file replace`. Shared source download and extraction remain separate. These totals expose whether an unchanged reuse repays the initial saved-work overhead.

| Project | Fresh seconds | Saved seconds | Saved change from fresh |
|---|---:|---:|---:|
| glob | 0.960 | 3.874 | +303.3% |
| boltons | 2.505 | 3.263 | +30.3% |
| jmespath | 1.426 | 6.835 | +379.4% |

Positive percentages mean saved work took longer. These scripts already know which questions to ask. They establish no saving on a complete agent task.

## Repeating the unchanged question

This table separates waiting for `fr` calls from the rest of each request. The remainder includes Python startup, reading saved objects and writing diagnostics. Call time includes SDK reply parsing. It is not a separate native CPU measurement.

| Project | Approach | fr commands | Call milliseconds | Other request milliseconds |
|---|---|---:|---:|---:|
| glob | fresh | 2 | 53.4 | 198.1 |
| glob | saved | 1 | 6.7 | 1424.3 |
| boltons | fresh | 2 | 223.5 | 185.9 |
| boltons | saved | 1 | 29.4 | 462.3 |
| jmespath | fresh | 3 | 151.7 | 234.6 |
| jmespath | saved | 1 | 17.7 | 1785.3 |

## CPU, memory and storage

CPU totals include the source copy, cleanup, requests and edit. GNU time, the runner's process timer, rounds combined CPU use to hundredths of a second. The raw record also includes finer CPU readings from Python and its commands.

Memory shows the largest sampled total for Python and its running commands. The table gives the median of three session peaks. Short peaks can escape sampling. Final storage includes the native cache, edit history and saved plan and answer files. Source files and retained measurement logs have separate sizes.

Storage and growth sum file lengths. Filesystem block and directory overhead remain unmeasured.

CPU time measures processor work. Elapsed time measures how long the request takes. KiB means 1,024 bytes. MiB means 1,048,576 bytes.

| Project | Approach | CPU seconds | Peak sampled MiB | Final state KiB | Request disk growth KiB |
|---|---|---:|---:|---:|---:|
| glob | fresh | 0.912 | 53.2 | 263.7 | 153.6 |
| glob | saved | 3.303 | 53.0 | 577.2 | 467.1 |
| boltons | fresh | 2.430 | 77.6 | 924.4 | 907.8 |
| boltons | saved | 3.014 | 77.6 | 1025.0 | 1008.4 |
| jmespath | fresh | 1.361 | 52.9 | 298.2 | 297.2 |
| jmespath | saved | 5.805 | 52.8 | 846.5 | 845.5 |

## Text and preparation costs

Every request includes the full worker program, its invocation and the question. Both approaches return the same answer bytes for the same project state. The raw record separates agent-visible text from intermediate `fr` replies. Those reply counts use compact JSON with sorted field names, excluding command-output whitespace differences.

Both approaches use Python. The comparison cannot isolate its extra cost compared with calling `fr` directly.

| Project | Source download and extraction seconds | Source KiB | Archive KiB |
|---|---:|---:|---:|
| glob | 0.274 | 87.2 | 21.7 |
| boltons | 0.384 | 906.5 | 244.8 |
| jmespath | 0.118 | 267.9 | 50.8 |

## Reproduction and limits

The optimized compilation took 174 seconds, outside the recurring request totals. The [collection steps](../tests/agent-eval/saved-work-cost/record.json) preserve checkout, toolchain setup and build timings separately.

The [original workflow](https://github.com/e6qu/fun-refactor/actions/runs/39531778541) records build and environment setup outside request totals. The [retained measurement](../tests/agent-eval/results/2026-10-21-saved-work-39531778541/result.json) identifies the source, optimized binary, questions and raw archive.

Operating-system caches remain uncontrolled. The order alternates, and each approach owns a separate native cache. Three repetitions provide a small project comparison. They cannot establish general speed or memory improvements.

The next change should target measured repeated work and preserve answer agreement. Compare complete sessions again before recommending saved work as a faster approach.

Earlier attempts remain in the first-evaluation record:

- [Run 39526264523](https://github.com/e6qu/fun-refactor/actions/runs/39526264523): cancelled; reserved implementation opened: false.
- [Run 39526530194](https://github.com/e6qu/fun-refactor/actions/runs/39526530194): cancelled; reserved implementation opened: false.
- [Run 39527089726](https://github.com/e6qu/fun-refactor/actions/runs/39527089726): cancelled; reserved implementation opened: false.
- [Run 39527620656](https://github.com/e6qu/fun-refactor/actions/runs/39527620656): failure; reserved implementation opened: false.
- [Run 39528768739](https://github.com/e6qu/fun-refactor/actions/runs/39528768739): failure; reserved implementation opened: false.
- [Run 39531651322](https://github.com/e6qu/fun-refactor/actions/runs/39531651322): cancelled; reserved implementation opened: false.
