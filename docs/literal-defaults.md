# Trace Python calls with default arguments

If `choose(value=0)` returns its argument, `choose()` returns the default while `choose(user_input())` uses the supplied input.
`fr` distinguishes those calls for supported literals and earlier local scalar defaults. A default that calls another function needs separate handling because Python evaluates it when defining the function.

## Commands, results and limits

Opt-in Python scalar summaries admit immutable literal and captured local scalar defaults for positional-only,
positional-or-keyword and keyword-only parameters. Explicit values retain their source-order effects
and override defaults. After binding explicit values, omitted optional slots receive an empty origin
set. A missing required slot still leaves analysis incomplete.

```python
def choose(value=0, /, *, spare=None):
    return value

sink(choose())          # no source origin
sink(choose(source()))  # possible source propagation
```

Admitted defaults are `None`, Booleans, numeric literals, single ordinary string or bytes literals, numeric
literals with one leading sign, and parentheses around admitted literals. Comments do not change
binding. Positional defaults must form a trailing suffix, including across `/`. Keyword-only
parameters can mix required and defaulted slots.

After an earlier `LIMIT = 10` assignment, `def choose(value=LIMIT): ...` also works.
Python captures the scalar when it defines the function. The [worked example](python-contract.md#call-a-helper-with-an-earlier-scalar-default)
shows omitted and explicit arguments, import support and the remaining binding limits.

Analyzed calls refuse defaults containing calls, other names, operators other than the admitted numeric sign, containers,
comprehensions, lambdas and formatted strings in default values. Function annotations require the documented
[future-annotation rule](python-contract.md#trace-a-helper-that-has-type-annotations). It also refuses an
unsupported default when a call supplies an explicit argument: Python evaluates defaults during
function definition, and overriding an argument does not remove those earlier effects.
Referenced variadic values, extra variadic arguments, unpacking, dynamic dispatch and implicit
exceptions remain outside this contract. Unused variadic declarations admit fixed-slot calls.

An uncalled module function can read an earlier checked value without running its body.
For example, after `EMPTY = EmptyType()`, `def unused(key=EMPTY): ...` reads the existing marker during definition.
The [declaration rule](python-contract.md#read-a-known-name-in-an-unused-functions-default) checks binding order and static value imports.
Calling a helper with an object or collection default still remains outside this call contract.

An analyzed entry function still represents all explicit inputs through symbolic parameter origins.
Its default does not force the analysis to represent only a zero-argument invocation. Literal defaults
also do not specialize truth tests on variables or call results. Summary substitution retains the
existing conservative control-flow semantics.

Each evaluated function summary includes `signature`, in declaration order. Every entry contains
its ASCII name, parameter kind, exact `summary-parameter` occurrence and optional `parameter-default`
occurrence. Separators create no parameter entries. Default locations include surrounding parentheses
when those belong to the syntax node. Imported signatures point into the callee's file.

The SDK exposes `FunctionSummary.signature` as typed `SummaryParameter` entries, with `required`,
`site` and `default` fields. It checks names, kinds, order, origin roles and snapshot consistency.
These are bounded syntax locations, not a proof that arbitrary Python behavior matches the model.

Current reports use `python-scalar-summaries` and `fr-call-binding` in public and canonical input records.
The reader requires signatures and the current default contract. Other formats fail explicitly.
Native reuse requires current analysis identities. A change to a default, parameter kind,
required slot or helper invalidates affected analysis; unrelated edits renew every signature origin.
The response budget includes signature metadata, and omitted results remain incomplete.

The [pinned task](../tests/agent-eval/literal-defaults/task.json) discovers a leaking helper through
an omitted keyword-only default. It requires reviewed repair, sink-free preview delivery,
fresh-process resumption, independent patch replay and exact reversal. The
[retained run](../tests/agent-eval/results/2026-10-14-named-defaults-literal-defaults/result.json) passes those
checks and binds the current source. The baseline records the installed local binary separately from
the source revision.
The [runtime corpus](../sdk/python/tests/test_literal_defaults.py) compares binding and public results
with CPython, checks exact AST coordinates, and exercises invalidation and forged contract refusals.

All compilation, full test gates and evidence regeneration run on GitHub. Actual implementation
edits use guarded `fr` previews, saved plans and history applications recorded in the
[dogfood manifest](../tests/agent-eval/results/2026-09-29-defaults-dogfood/manifest.json).
