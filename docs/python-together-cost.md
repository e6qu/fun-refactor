# Is asking about two helpers together faster?

You want to know whether submitted input can reach an output through either of two helpers.
`relay(value)` returns its argument. `discard(value)` returns zero.
The expected answers differ: input can pass through `relay`, while `discard` stops it.
After changing `discard` to return its argument, both answers must show that possible connection.

The [complete lesson](check-python-together.md) supplies the files, rules, commands and expected replies.
The [measurement generator](../tests/agent-eval/python-startup-cost/corpus.py) adds unused declarations and edits to those examples.
Here `source()` represents submitted input and `sink(value)` represents an output receiving it.
The checker follows possible connections; a reported connection does not prove that execution takes that path.

The [earlier process comparison](python-process-cost-results.md) found lower costs with an installed Python client kept running.
This experiment uses that arrangement for both approaches:

| Approach | What happens for each pair of questions |
| --- | --- |
| Separate, the baseline | Find both functions, then request two fresh answers |
| Together | Find both functions, then use the existing `analyze_together` operation |

The Python client, also called the SDK, starts the compiled `fr` program and validates its replies.
Both approaches use the same installed client and Python interpreter.
Neither saves complete answers or checked Python preparation between requests.
Both use the ordinary project cache, which saves facts extracted from source files.
Each sequence starts with its own empty cache, and its first fill counts toward the measured cost.

## The work limits differ

An **analysis step** is a unit of work counted by the checker.
Each separate answer receives 4,096 steps and up to 1,048,576 response bytes.
The combined request shares 4,096 steps and that response allowance between both answers.
It does not receive 8,192 steps or twice the output space.
One MiB means 1,048,576 bytes.

The exhausted-work example permits one step instead of 4,096.
Separate requests can each use one step.
Together, the first answer can use that step and leave zero for the second.
The measurement preserves both unfinished answers and their different recorded work.
Their elapsed times cannot establish equal-answer savings.

All questions explicitly select depth eight, context `html` and function summaries.
Depth bounds nested calls. Summaries describe how arguments pass through helpers.
The context selects the kind of output for cleaning rules; these examples define no cleaning functions.
Only the imported-helper example enables import checking.
Its limits stay at 16 files, 256 KiB of source and 128 searches; one KiB means 1,024 bytes.

The timed examples use a large output allowance, so they do not measure behavior near the output limit.
The existing [SDK test](../sdk/python/tests/test_flow_together.py) separately checks that a combined reply exceeding its allowance fails visibly.

## Which questions repeat?

Startup is code Python executes while loading a file, before you call the selected helper.
That code can replace a helper or change a value it uses.
When the checker cannot understand a startup call, its answer must stay unfinished.

Each Python process answers seven pairs, rather than the seven individual questions in the earlier experiment:

1. Ask about both helpers with their original definitions.
2. Repeat the pair without an edit.
3. Ask in reverse order.
4. Edit `discard` to return its argument, then ask both questions again.
5. Add an unknown startup call, `opaque()`. Both answers must stay unfinished.
6. Ask that unfinished pair in reverse order.
7. Restore the original definitions. `discard` must stop the input again.

The four examples have local helpers, imported helpers, 200 unused declarations or an exhausted step allowance.
Five repetitions of both approaches produce 40 measured Python processes, 280 pairs and 560 function answers.
Both approaches find both functions for every pair.
Separate requests make four native calls per pair; together makes three.
A **native call** here means starting the compiled `fr` program once.
That produces 980 native calls in total, without treating fewer calls as proof of lower elapsed time.
The starting approach alternates between repetitions; five repetitions leave three starts in one order and two in the other.
Every sample retains its position and timing.

The collector also executes these examples in ordinary Python in separate correctness checks.
Original definitions return `[17, 0]`; the edited definitions return `[17, 17]`.
The unknown startup raises `NameError`.
These runtime checks occur outside the timed sequences and do not compare Python execution time with analysis time.

## Which costs count?

The recurring total adds the first process start through its ready reply, all seven submitted pairs through parsed replies, and shutdown through process exit.
It includes client loading, both function searches, native startup, analysis, client validation and measurement code.
Writing and reading replies in JSON, a structured text format, also counts.
Each process stays alive for its whole sequence in both approaches.
The controller changes the example files, checks loaded module paths and saves progress between those intervals.
A separate session wall-time record includes that controller work.
The controller's own startup, installation inventory and ordinary-Python correctness checks run outside every session's timers.

The installation record lists building a wheel, installing it and compiling checked-hash bytecode separately.
A **wheel** is a Python package archive.
**Bytecode** is compiled Python instructions saved for later starts.
Checked-hash bytecode lets Python verify source contents before accepting those saved instructions.
The collector checks every client file's source and saved bytecode before and after the experiment.
Each process records which client files it loaded.
Runner preparation, interpreter setup, installing build tools and compiling `fr` remain outside these installation timers.
Both approaches use isolated Python with `-I -B`: user import paths are isolated, new bytecode writes are disabled, and existing bytecode can still load.
Operating-system file caches and standard-library bytecode remain uncontrolled.

Native-call timers locate work inside each pair; they do not replace the complete process total.
The byte counts measure compact JSON reconstructed from parsed native reports, including searches.
They do not measure raw pipe traffic, agent-visible text or memory.

## Run and inspect the experiment

Use the **Two Python questions together or separately** GitHub workflow on the revision under review.
Full builds and collection run on GitHub under this project's workstation policy.
The workflow runs corruption controls, builds ordinary optimized `fr`, verifies the downloaded binary and runs:

```sh
python3 -B tools/python-together-cost.py --fr target/release/fr \
  --repetitions 5 --output "$RUNNER_TEMP/python-together-cost"
python3 -B tools/python-together-cost.py \
  --audit "$RUNNER_TEMP/python-together-cost/result.json"
```

`result.json` retains all reports and sample timings, file identities, settings, hardware and installation costs.
The artifact also contains progress records, the built wheel and any failure trace.
The independent audit checks every stage, selected function, source edit, unfinished reason, work allocation, cost sum and native report byte count.
With enough work available, full answers must agree between approaches and repetitions.
Only records of how each command ran (`execution`) and its project identity (`context_basis`) leave that comparison.
The one-step case compares the first answer across approaches and keeps the second answers separate because their work differs.

These are disclosed examples used during development.
Results cannot establish support for unfamiliar packages, savings on complete agent tasks, memory savings or the execution overhead of translated programs.
