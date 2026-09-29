# Literal defaults and signature evidence

Opt-in Python scalar summaries admit immutable literal defaults for positional-only,
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

Analysis refuses calls, names, operators other than the admitted numeric sign, containers,
comprehensions, lambdas, formatted strings and annotations in default contracts. It also refuses an
unsupported default when a call supplies an explicit argument: Python evaluates defaults during
function definition, and overriding an argument does not remove those earlier effects.
Variadics, unpacking, dynamic dispatch and implicit exceptions remain outside this contract.

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

Reports use `python-scalar-summaries-4` and `fr-call-binding-2` in public and canonical input records.
Version-three reports retain the earlier required-parameter contract; their SDK signatures and
literal-default contracts are `None`. Versions one and two remain readable without call-binding
claims. Native reuse requires current analysis identities. A change to a default, parameter kind,
required slot or helper invalidates affected analysis; unrelated edits renew every signature origin.
The response budget includes signature metadata, and omitted results remain incomplete.

The [pinned task](../tests/agent-eval/literal-defaults/task.json) discovers a leaking helper through
an omitted keyword-only default. It requires reviewed repair, sink-free preview delivery,
fresh-process resumption, independent patch replay and exact reversal. The
[retained run](../tests/agent-eval/results/2026-09-29-defaults-literal-defaults/result.json) passes those
checks and binds the current source. The baseline records the installed local binary separately from
the source revision.
The [runtime corpus](../sdk/python/tests/test_literal_defaults.py) compares binding and public results
with CPython, checks exact AST coordinates, and exercises invalidation and forged contract refusals.

All compilation, full test gates and evidence regeneration run on GitHub. Actual implementation
edits use guarded `fr` previews, saved plans and history applications recorded in the
[dogfood manifest](../tests/agent-eval/results/2026-09-29-defaults-dogfood/manifest.json).
