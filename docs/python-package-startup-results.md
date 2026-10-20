# What package startup prevents us from answering

The [startup investigation](python-package-startup.md) asks why simple helpers still produce incomplete package answers.
Ordinary Python ran all four selected helpers successfully through their original package imports.
The observation does not complete the analyzer's answers.
It shows different obstacles that another small change to function defaults would leave unresolved.

## What the first capture observed

The [first hosted run](https://github.com/e6qu/fun-refactor/actions/runs/39073655766) passed thirteen control tests and completed all sixteen planned processes.
All forty-eight helper calls matched their independently stated integer results.
Ordinary and instrumented runs loaded the same package modules and returned the same results for these inputs.
There were no resource stops or collection errors.
The [raw artifact](../tests/agent-eval/python-startup/2026-10-18-first-run.zip) and [receipt](../tests/agent-eval/python-startup/first-run.json) preserve that first capture.
The receipt identifies its original collector; later audit improvements require a fresh capture rather than rewriting this one.

The [checked repeat](https://github.com/e6qu/fun-refactor/actions/runs/39073951823) also passed thirteen controls and all sixteen processes without stops or collection errors.
It repeats the package file counts, source sizes and forty-eight expected integer results below.
Its separate audit also checks producer metadata, raw-output fingerprints, transcript sizes and sampled measurements against the unchanged limits.
The [second raw artifact](../tests/agent-eval/python-startup/2026-10-18-checked-run.zip) and [receipt](../tests/agent-eval/python-startup/checked-run.json) preserve that capture.
It used an AMD EPYC 7763 processor; the timing table continues to describe the first runner only.
Review then found that both captures' internal helper-call timer included copying the module inventory.
Their outer elapsed measurements include that work as declared, so the table below remains valid for those complete observed processes.
Do not use their internal call durations as measurements of the helper alone.
The corrected collector starts the call timer after copying file names at import completion.
It also stops after saving the first failed process, preserving upload time within the unchanged job deadline.

The [corrected capture](https://github.com/e6qu/fun-refactor/actions/runs/39074486759) passed sixteen controls, including separate timing and first-failure preservation.
All sixteen real processes and forty-eight integer calls completed without resource stops or collection errors.
The package file counts and source sizes agree with both earlier captures.
Its [raw artifact](../tests/agent-eval/python-startup/2026-10-18-corrected-timing.zip) and [current receipt](../tests/agent-eval/python-startup/current-run.json) preserve the corrected measurements.
That runner used an Intel Xeon Platinum 8370C processor.
The timing table still describes the first capture; results from different processors do not establish a timing improvement.

| Selected helper | Files read by the saved analyzer answer | Package source files observed in Python | Whole-file source bytes observed |
| --- | ---: | ---: | ---: |
| Toolz `return_none` | 14 | 13 | 103,897 |
| Funcy `inc` | 16 | 16 | 74,739 |
| Funcy `dec` | 16 | 16 | 74,739 |
| Pydash `add` | 8 | 15 | 430,220 |

The analyzer column includes one small calling script, `repository_probe.py`, outside the original package.
The Python column counts distinct source files inside the pinned package, including its startup file.
These counts exclude standard-library and separately installed dependency files.
The observer calls each helper directly on the listed integers; it does not run the saved analyzer report's input/output calling script.
The comparison concerns package startup and selected helper behavior, rather than identical complete application executions.
Bytes count each observed source file's entire contents, including function bodies that never ran.
They do not measure the minimum code necessary to answer the question.

The saved analyzer answers still report 82 unchecked startup statements for Toolz, 76 for each Funcy question and 420 for Pydash.
The new observation leaves those answers unchanged.
They remain subject to unsupported imports, startup behavior and the existing search limits.

## Why the remaining work differs

**Toolz needs startup effects understood.** The analyzer already read every package source file observed in this run, plus the calling script.
Python still called `create_signature_registry` during package import.
That function builds a table describing which arguments other functions accept.
Its `expand_sig` helper ran 237 times before the selected `return_none` call.
Only `toolz/functoolz.py` ran package code during the later helper-call phase.
The difference explains where to investigate; it does not prove the earlier setup cannot affect the helper.

For example, the [complete small-package lesson](python-package-startup.md#try-a-package-that-changes-its-helper) replaces a helper during import.
Skipping apparently unrelated startup code would miss that change.
Toolz's table builder also calls imported introspection functions, which examine other functions' arguments.
Those calls need checked behavior before the analyzer can declare startup understood.

**Funcy needs work split without losing dependencies.** Python observed sixteen package source files.
The separate calling script makes seventeen files for this complete observed execution, above the existing sixteen-file limit.
The analyzer read fifteen of those package files and left `funcy/_inspect.py` unread.
Both later integer helpers ran only `funcy/primitives.py` in the package.
The small helper body does not remove the package startup that preceded it.

**Pydash needs more than a repeated-search fix.** Python observed fifteen package source files totaling 430,220 bytes, about 420 KiB.
KiB means 1,024 bytes. The existing source limit is 256 KiB, or 262,144 bytes.
Eight observed package files were absent from the analyzer's loaded files, including `pydash/helpers.py` and `pydash/types.py`.
The selected integer calls later ran only `pydash/numerical.py` in the package.
Reducing duplicate searches alone cannot fit all this observed source into the existing single-analysis source budget.

## What to implement next

The next useful question is: can the checker understand a package's startup once and safely reuse that result for a helper question?
A reusable result would need to record the source and imports it depends on, which names or objects startup changes, and what remains unknown.
Changing any of those inputs must invalidate the saved result.
The result must preserve unknown calls and writes instead of assuming they have no effect.

The [first startup-call implementation](check-python-startup.md) checks a small registration function and keeps a helper-replacement control unfinished.
A registration function records values or functions for later use, like Toolz's argument table.
Tests compare both outcomes with ordinary Python before using the supported case to make an answer complete.
The checker now handles bounded copies from an unchanged literal dictionary, but Toolz's actual builder needs more than that loop rule.
The familiar Toolz source at revision `451af60dec590a6010e2babdbf391ea8f815122f` makes the next missing pieces concrete.
Its table builder accepts dictionaries as default arguments, visits `module_info.items()`, and visits each module's function entries.
The checker now handles dictionary arguments and defaults in small registration functions.
For example, `def register(inputs=labels, output=table): ...` stores references to the two earlier dictionaries.
An explicit call can replace either reference. Two parameters pointing to one dictionary cannot hide a write during iteration.
Changed source contents and rebinding a stored default's name stay unfinished.
For a module named by a string, it imports that module before looking up its functions.
It then calls `expand_sig`, which inspects the arguments accepted by a function and constructs new signature values.
A signature describes those accepted arguments.
The dictionary and bounded copy rules cannot establish the behavior of those imports, inspections or computed values.
Some input tables also exceed the existing per-initializer syntax limit and use module objects as keys.
Completing this package question needs checked connections across these steps; recognizing a `for` statement alone does not finish it.
Keep existing file, source, search and execution limits throughout that work.
Splitting a large question into smaller checked steps requires an explicit dependency connection between those steps.
Running more unchecked source elsewhere would not solve the correctness problem.

The [fresh library replay](../tests/agent-eval/results/2026-10-19-registration-arguments-fixed-arity-probes/result.json) checked 100 helper cases and 200 Python executions.
All 40 cases using the original full packages stayed unfinished or refused.
Sixteen cases completed with isolated declarations and eight with a whole helper module copied outside its package.
Those smaller contexts do not establish original-package support.
The [Python receipt](../tests/agent-eval/results/2026-10-19-registration-arguments-python-contract/manifest.json) records the dictionary arguments, defaults, loops and unfinished controls.
The focused interface suite passed 172 cases: 96 for arguments and stored references, and 76 for loops.
The broader Python suite passed 1,656 cases.
Both the function's syntax and its repeated work keep the existing 128-node limit.
No separate measurement establishes this rule's time or memory cost.

These observations identify a design question, not an implemented package-summary capability.
They do not establish a safe minimal dependency set or general support for Toolz, Funcy or Pydash.
The later [package milestones](package-milestones.md) record Jaraco.functools' first
unsuccessful admission and the next unfamiliar-project question. It is no longer an unused reservation.

## What the timings mean

The first runner used Python 3.14.8 on an AMD EPYC 9V45 processor.
Outer elapsed times included a fresh interpreter, the package import, three helper calls, source inspection, output and monitoring.

| Helper | Ordinary Python elapsed range, seconds | Instrumented Python elapsed range, seconds |
| --- | ---: | ---: |
| Toolz `return_none` | 0.069500–0.120129 | 0.072614–0.130088 |
| Funcy `inc` | 0.072975–0.125304 | 0.078976–0.131090 |
| Funcy `dec` | 0.070721–0.081507 | 0.079910–0.084375 |
| Pydash `add` | 0.124876–0.127654 | 0.134245–0.136398 |

Each range contains only two observations.
The baseline performs the same imports and calls without the tracing callback.
Both methods perform the later source inspection and output.
The [method](python-package-startup.md#understand-the-comparison-costs) explains included costs, excluded setup, sampled-resource limits and uncontrolled caches.
This small capture cannot establish a stable tracing slowdown, analyzer overhead or agent savings.
