# Which argument reaches a Python helper?

Suppose `choose(left, right)` returns `left`. Passing user input as `right` should not make that input reach the result.
`fr` checks how positional and named arguments match parameters in supported Python functions. The examples below show what it can answer and when it must leave the answer incomplete.

## Commands, results and limits

Opt-in summary analysis binds explicit positional and named arguments to Python parameters
with ASCII names, including the [immutable literal defaults](literal-defaults.md) subset. Unicode identifier normalization remains outside this contract.
Function declarations and identifiers in analyzed bodies must already be in NFKC form.
Names such as `café` remain supported; spellings that normalize to another name leave analysis
incomplete. This prevents function and local bindings from silently shadowing another spelling.
It supports positional-only parameters before `/`, positional-or-keyword parameters, and
keyword-only parameters after a bare `*`. Calls can reach same-file helpers, recursive summaries,
static package functions, re-exports and module aliases.

```python
def choose(left, /, *, right):
    return left

choose(source(), right=0)
```

Argument values are evaluated in source order, once per expression transfer. Binding then places
those flows in declaration order for independent summary substitution. Keyword order does not
change which parameter receives an origin. Comments and parameter separators create no origins.
An entry function gives each actual parameter a symbolic origin, including keyword-only parameters.

An explicit raise in an argument stops evaluation of later arguments and the callee. Unknown
short-circuit alternatives still join their normal continuations. Arguments of an invalid call can
have effects before binding fails. Missing, excess, duplicate and unknown arguments produce an
`invalid-call-binding` cutoff. Passing a positional-only parameter by keyword or a keyword-only
parameter positionally also produces that cutoff. The report is incomplete; it does not model the
implicit `TypeError` or establish absence of later behavior.

Lexical checks reject repeated keyword names and invalid argument ordering before expression
transfer, including skipped branches and arguments that never return. Generator argument syntax
is outside this contract. These checks are bounded subset validation, not a general Python compiler.

Evaluated defaults, referenced variadic values and argument unpacking remain outside the
supported call contract. Annotations require the documented
[future-annotation directive](python-contract.md#trace-a-helper-that-has-type-annotations).
Other annotation evaluation remains unchecked. External
source, sink, propagator and sanitizer rules have no declared parameter names, so keyword calls to
those rules report `keyword-rule-contract-unchecked`. A caller can use an admitted local wrapper
with explicit parameters and a positional rule call. Dynamic dispatch, implicit exceptions and
heap effects remain outside scalar summary analysis.

The binding order follows the [Python call reference](https://docs.python.org/3/reference/expressions.html#calls).
The finite runtime corpus compares independent CPython observations with origin propagation,
argument effects, parameter kinds and invalid binding. It does not prove source correspondence or
general path feasibility.

Current reports use `python-scalar-summaries` and retain `fr-call-binding` in both canonical inputs and
the public report. Native reuse validates both identities, even if a modified report has a freshly
computed digest. Analyzer identities include the binding implementation.
The response budget includes both contract copies. If even metadata cannot fit, the command refuses;
omitting analysis sections yields an incomplete report, never a complete absence claim. The SDK exposes
`FunctionSummaries.call_binding` and validates its complete required contract. Other report formats
fail explicitly; the reader provides no compatibility adapter.

The required `CallBinding.definitions` field separates declaration safety from call support.
Unannotated functions with ASCII parameters and immutable literal defaults defer their bodies,
including referenced variadics. An uncalled declaration therefore does not introduce a module-effect
cutoff solely because its body or call binding lies outside the scalar model. Calling that function
still requires the fixed-slot contract. An unsupported selected entry remains incomplete.
Decorators, evaluated defaults and annotations outside the documented future rule remain unchecked during initialization.

The required `CallBinding.initialization` field describes which declarations may coexist with a trace.
The [current Python examples](python-contract.md) cover unused literals, collections, metadata, plain classes,
function aliases and built-in references. They also state the collision and evaluation limits.
For example, `registry = {}` can initialize without blocking a helper, but reading that global registry
still leaves the answer unfinished. A literal package member cannot select a same-named child module.

For an unused built-in reference, the left-hand name must be new to the module.
Python's existing built-in with that name does not count as a module binding.
For example, `sorted = sorted` can coexist with a supported helper, while calling that stored function
still leaves the answer unfinished.

Coroutine and generator calls do not execute their bodies as ordinary scalar calls. The analyzer
refuses those executions before transferring body effects. A yield inside a skipped expression still
makes its enclosing function a generator. A nested function's deferred body belongs to its own scope;
yield expressions in nested defaults still belong to the enclosing scope.

The [pinned task](../tests/agent-eval/call-binding/task.json) discovers a leaking helper through
keyword calls and parameter separators. It requires a reviewed repair, a sink-free preview feature,
fresh-process resumption and independent receiver patch replay and reversal. Its baseline records
the installed 0.35.0 binary separately from the pinned source revision; no local rebuild was used.

The [runtime tests](../sdk/python/tests/test_call_binding.py) also compare retained and clean results
after keyword, parameter-order, separator and helper changes. Unrelated source edits renew
occurrences without invalidating the defining-file analysis. Whole-analysis invalidation remains
conservative; finer summary reuse is a separate remaining gate.


The earlier required-parameter [retained acceptance](../tests/agent-eval/results/2026-09-29-calls-call-binding/result.json)
passes the repair, preview and independent receiver outcomes. Ordinary discovery reads 444 source
bytes across six files; native discovery returns 117,022 bytes across eight calls with no source-body
reveals. These scripted measurements establish no general efficiency or token-saving benefit.
The [authoring record](../tests/agent-eval/results/2026-09-29-calls-dogfood/manifest.json)
retains 100 receipts from guarded `fr` edits on the required-parameter implementation.
