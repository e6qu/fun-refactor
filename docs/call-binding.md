# Which argument reaches a Python helper?

Suppose `choose(left, right)` returns `left`. Passing user input as `right` should not make that input reach the result.
`fr` checks how positional and named arguments match parameters in supported Python functions. The examples below show what it can answer and when it must leave the answer incomplete.

A parameter is a named slot in a function definition. An argument is the value supplied by a call.
Binding matches arguments to those slots: `choose(right=7, left=0)` supplies zero to `left` despite writing it second.
A summary records which slots can contribute to the function's result.
The examples use `source()` and `sink()` as configured external input and output calls, not built-in Python functions.

## Commands, results and limits

With `--summaries`, the analysis matches supplied arguments to supported parameters,
including [literal and earlier local or imported scalar defaults](literal-defaults.md).
Parameters use ordinary ASCII letters, digits and underscores.
Function and body-variable names must already use Python's standardized Unicode spelling, called NFKC.
For example, Python treats the Kelvin sign `K` as `K`; this tool leaves the unconverted spelling incomplete.
`café` already has the required spelling and can name a function or body variable here.
These checks prevent two spellings from silently hiding the same binding.
It supports positional-only parameters (slots that calls must supply by position) before `/`, positional-or-keyword parameters, and
keyword-only parameters (slots that calls must supply by name) after a bare `*`. Calls can reach same-file helpers, recursive summaries,
static package functions, re-exports (making an imported name available from another module) and module aliases.

```python
def choose(left, /, *, right):
    return left

choose(source(), right=0)
choose(0, right=source())
```

Python evaluates explicit arguments in the order they appear in the call.
In `choose(0, right=source())`, the input call still runs, but the returned `left` argument is zero.
The analysis keeps that evaluation order and matches each value to its parameter.
Comments and separators such as `/` add no input connection.
Selecting a function itself as the entry asks about an independent possible input for each parameter.
For complete runnable files and input/output rules, use the [Python trace walkthrough](python-contract.md).

If an argument's helper raises an error, later arguments and the called helper do not run.
When the model cannot decide a Boolean condition, it keeps both possible paths.
Arguments can have effects before Python rejects a call.
For example, `choose(source())` runs `source()` before Python reports the missing `right` argument.
Missing, excess, duplicate and unknown arguments produce an
`invalid-call-binding` cutoff. Passing a positional-only parameter by keyword or a keyword-only
parameter positionally also produces that cutoff. The report is incomplete; it does not model the
implicit `TypeError` or establish absence of later behavior.

The call checker rejects repeated keyword names and invalid argument ordering before following argument expressions.
It applies this check even in skipped branches and arguments that never return.
Generator argument syntax stays outside this contract. These limited checks do not replace Python's compiler.

Defaults that execute a call remain outside this call support.
A function can declare unused extra-argument slots, such as `*items` and `**options`, while calls still fit its ordinary fixed parameters.
Reading those slots, passing extra arguments, or unpacking values into a call with `*items` or `**options` remains unsupported.
Annotations require the documented
[future-annotation directive](python-contract.md#trace-a-helper-that-has-type-annotations).
Other annotation evaluation remains unchecked. External
source, sink, propagator and sanitizer rules have no declared parameter names, so keyword calls to
those rules report `keyword-rule-contract-unchecked`. A caller can use an admitted local wrapper
with explicit parameters and a positional rule call. Dynamic dispatch, implicit exceptions (errors an operation can raise without an explicit `raise`) and
changes to shared mutable objects remain outside scalar summary analysis.

The binding order follows the [Python call reference](https://docs.python.org/3/reference/expressions.html#calls).
The finite runtime corpus compares independent CPython observations with origin propagation,
argument effects, parameter kinds and invalid binding. It does not prove the connection between source and its model or
general path feasibility.

Reports label this mode `python-scalar-summaries` and store the call rules under `fr-call-binding`.
The rules appear in both the public report and the input details used to verify a saved answer.
Before reuse, `fr` checks both copies against its current rules and implementation.
It rejects missing or obsolete rules.
The response budget includes both contract copies. If even metadata cannot fit, the command refuses;
omitting analysis sections yields an incomplete report, never a complete absence claim. The SDK exposes
`FunctionSummaries.call_binding` and validates its complete required contract. Other report formats
fail explicitly; the reader provides no compatibility adapter.

The required `CallBinding.definitions` field separates declaration safety from call support.
Module functions with ASCII parameters and literal or earlier known value defaults defer their bodies,
including referenced variadics. An uncalled declaration therefore does not introduce a module-effect
cutoff solely because its body or call binding lies outside the scalar model. Calling that function
still requires the fixed-slot contract. An unsupported selected entry remains incomplete.
The [named-default example](python-contract.md#read-a-known-name-in-an-unused-functions-default) checks whether a value exists before the declaration.
Calls can use [earlier scalar defaults](python-contract.md#call-a-helper-with-an-earlier-scalar-default), including [values imported from checked local files](python-contract.md#share-a-default-value-between-files).
Giving a checked scalar another name, such as `LIMIT = BASE`, also preserves that default.
The [alias lesson](python-contract.md#give-a-shared-setting-another-name) explains the files, call results and remaining limits.
Collections, object markers and function objects still prevent calls, even with explicit replacement arguments.
Decorators, computed defaults, class-method defaults and annotations outside the documented future rule remain unchecked during initialization.

To find where a shared default came from, the Python interface exposes `FlowDependencies.value_aliases` when imports are enabled.
Each `ValueAlias` records `path`, `name`, `target` and `offset`.
For `LIMIT = BASE` in `settings.py`, `name` is `LIMIT` and `target` is the earlier name `BASE`.
`offset` counts UTF-8 bytes from the start of that file, rather than Python string characters.
For example, `café` has four characters but occupies five bytes in UTF-8.
The import's `binding_chain` follows those names back to the original declaration.
Aliasing a list does not make it a scalar.
The reader requires the current inventory, including an empty list when there are no value aliases.
It rejects missing or contradictory records before using saved answers.

The required `CallBinding.initialization` field describes which declarations may coexist with a trace.
It includes [bounded maps of built-in types](python-contract.md#keep-a-built-in-type-map-during-import).
The [current Python examples](python-contract.md) cover unused literals, collections, metadata, plain classes,
function and value aliases, and built-in references. They also state the collision and evaluation limits.
For example, `registry = {}` can initialize without blocking a helper, but reading that global registry
still leaves the answer unfinished. A literal package member cannot select a same-named child module.

For an unused built-in reference, the left-hand name must be new to the module.
Python's existing built-in with that name does not count as a module binding.
For example, `sorted = sorted` can coexist with a supported helper, while calling that stored function
still leaves the answer unfinished.

Coroutine functions use `async def`; generator functions contain `yield`.
Their execution rules need separate handling, so these calls leave the analysis incomplete.
Even `if False: yield value` makes the enclosing function a generator.
A nested function's body belongs to that nested function; its defaults still belong to the function that defines it.

The [pinned task](../tests/agent-eval/call-binding/task.json) discovers a leaking helper through
keyword calls and parameter separators. It requires a reviewed repair, a sink-free preview feature,
resumption in a new process and independent receiver patch replay and reversal. Its baseline records
the installed 0.35.0 binary separately from the pinned source revision; no local rebuild was used.

The [runtime tests](../sdk/python/tests/test_call_binding.py) also compare retained and clean results
after keyword, parameter-order, separator and helper changes. Unrelated source edits renew
source references without invalidating the defining-file analysis.
The current cache can recheck more functions than an individual edit requires.
Reusing each unaffected helper separately remains open work.


The earlier required-parameter [retained acceptance](../tests/agent-eval/results/2026-09-29-calls-call-binding/result.json)
passes the repair, preview and independent receiver outcomes.
Its Python comparison script reads and parses the six known fixture files, totaling 444 source bytes.
Fr's discovery commands produce 117,022 bytes of serialized JSON report data across eight calls and reveal no source bodies.
Those counters measure different payloads; internal response bytes also do not establish what an agent reads or pays for.
The timers cover different work too: the Python arm includes edits and two runtime checks, while the fr discovery timer stops before those stages.
This record therefore supplies no overall time-overhead percentage or general efficiency claim.
The [authoring record](../tests/agent-eval/results/2026-09-29-calls-dogfood/manifest.json)
retains 100 receipts from guarded `fr` edits on the required-parameter implementation.
