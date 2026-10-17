# Trace Python calls with default arguments

If `choose(value=0)` returns its argument, `choose()` returns the default while `choose(user_input())` uses the supplied input.
`fr` distinguishes those calls for supported literals and earlier local or imported scalar defaults. A default that calls another function needs separate handling because Python evaluates it when defining the function.

A default supplies a parameter when the caller omits its argument.
A literal writes a value directly, such as `0`, `None` or `"ready"`.
In Python parameters, `/` makes earlier slots positional-only; `*` makes later slots keyword-only.
A normal `value` parameter accepts either `choose(17)` or `choose(value=17)`.
For `choose(value=0, /, *, spare=None)`, call the first slot by position and the second by `spare=...`.
The examples use `source()` and `sink()` as configured external input/output calls.

## Commands, results and limits

With `--summaries`, the tool can follow supported helper calls with literal or captured scalar defaults.
This works for all three parameter kinds described above.
An explicit argument replaces its default and keeps its own input connection and effects.
An omitted argument uses the stored scalar, which introduces no tracked input.
Omitting a required argument leaves the analysis incomplete.

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
The [shared-default lesson](python-contract.md#share-a-default-value-between-files) supplies all three files for `from settings import LIMIT` followed by `def choose(value=LIMIT): ...`.
It explains how the default captures an earlier scalar and how changing that scalar into a list keeps calls incomplete.
An earlier checked scalar can also have another name, as in `BASE = 0; LIMIT = BASE`.
The [alias lesson](python-contract.md#give-a-shared-setting-another-name) follows that value across three files and shows which changes invalidate the answer.
Aliasing a list or object keeps its original kind; it does not make calls using that default supported.

The analysis leaves calls incomplete for defaults such as `value=[]`, `value=(0,)`, `value=lambda: 0`,
`value=f'{name}'` or `value=make_default()`.
Those examples store collections or callables, format a string or execute a call.
Unknown names and other computed expressions also need separate handling.
Function annotations require the documented
[future-annotation rule](python-contract.md#trace-a-helper-that-has-type-annotations).
A call stays incomplete for an unsupported default even when it supplies an explicit argument.
Python evaluates defaults during function definition; replacing an argument does not remove those earlier effects.
Referenced variadic values, extra arguments through `*args` or `**kwargs`, unpacking, dynamic dispatch (choosing a callable from runtime values) and implicit
exceptions remain outside this contract. Unused variadic declarations admit fixed-slot calls.

An uncalled module function can read an earlier checked value without running its body.
For example, after `EMPTY = EmptyType()`, `def unused(key=EMPTY): ...` reads the existing marker during definition.
The [declaration rule](python-contract.md#read-a-known-name-in-an-unused-functions-default) checks binding order and static value imports.
Calling a helper with an object or collection default still remains outside this call contract.

Selecting `choose` itself as the analysis entry asks about values supplied to its parameters.
Its default does not restrict that question to `choose()` with no argument.
The model also keeps both sides of an unknown branch, even when the default happens to be `0`.
Use the [complete lesson](python-contract.md#share-a-default-value-between-files) for runnable files, rules and commands; the snippets on this page illustrate call behavior.

Each checked helper's `signature` lists its parameters in declaration order.
For `def choose(value=(0)): ...`, the parameter's `site` points to `value` and its `default` points to `(0)`.
The report labels these source locations `summary-parameter` and `parameter-default`.
Separators such as `/` and `*` create no parameter entries.
A helper defined in `helpers.py` keeps those locations even when `app.py` imports and calls it.

The Python interface exposes that list as `FunctionSummary.signature`.
Each `SummaryParameter` has `required`, `site` and `default` fields; `required` is false for `value=(0)`.
The reader checks names, parameter kinds, source roles and the recorded revision.
These checks validate the reported locations and format. They do not establish every Python runtime behavior.

Current reports identify this behavior with `python-scalar-summaries` and `fr-call-binding`.
The reader requires the complete current signature and call rules. It rejects missing or obsolete fields.
Before reusing a saved answer, `fr` checks the source inputs and analysis settings.
Changing a default, parameter kind, required slot or helper invalidates the affected answer.
An unrelated edit can preserve the analysis result while renewing its source references.
The response byte limit includes signature details; leaving out results keeps the answer incomplete.

The [pinned task](../tests/agent-eval/literal-defaults/task.json) discovers a leaking helper through
an omitted keyword-only default. It requires reviewed repair, sink-free preview delivery,
resumption in a new process, independent patch replay and exact reversal. The
[retained run](../tests/agent-eval/results/2026-10-17-value-aliases-literal-defaults/result.json) passes those
checks and binds the current source. The baseline records the installed local binary separately from
the source revision.
The [runtime corpus](../sdk/python/tests/test_literal_defaults.py) compares binding and public results
with CPython, checks exact syntax-tree source positions, and exercises invalidation and forged contract refusals.

All compilation, full test gates and evidence regeneration run on GitHub. Actual implementation
edits use guarded `fr` previews, saved plans and history applications recorded in the
[dogfood manifest](../tests/agent-eval/results/2026-09-29-defaults-dogfood/manifest.json).
