# Follow an input through an original Python package

Suppose your code uses Boltons to keep a number between zero and fifty:

```python
from boltons.mathutils import clamp

def entry():
    return record_value(clamp(emit_value(), 0, 50))
```

Here `emit_value()` supplies an input and `record_value()` receives the output.
The check follows that input through supported operations to the output. Python
returns 17 for input 17 and 31 for input 31. This checks a possible connection;
it does not establish that every input produces a different output.

Overwriting the input changes the question:

```python
from boltons.mathutils import clamp

def entry():
    value = emit_value()
    value = 0
    return record_value(clamp(value, 0, 50))
```

Both example inputs now produce zero. A finished analysis should report that
the selected input no longer reaches the output.

The original package matters. Python runs package startup code before calling
`clamp`. That code can change the meaning of imported names. Checking a copied
`clamp` function would answer a smaller question. This experiment keeps the
package files and original import unchanged.

## State the standard-library assumptions

The source analyzer does not run Python startup to discover what names mean.
For selected standard-library operations, it uses an explicit description of
their behavior. We call that description a **model**.

For this Boltons question, select `builtins,math,bisect,binascii`. Boltons imports
the last three modules; its helper uses built-in numeric operations. Selecting
a module does not support every call in it. For example, an unmodeled `bisect`
call still leaves an unfinished answer.

In the command line, use `--summaries --imports --standard-library builtins,math,bisect,binascii`
alongside the selected function, caller rules and explicit search budgets. In the
Python client library, supply the same names as the `standard_library` tuple to
`FlowCache.analyze`.
With `builtins` selected, definition-time defaults can include `float()` and the
checked literal infinity or NaN forms, such as `float('-inf')`. Other evaluated
defaults remain unchecked. A default supplies a value when a caller leaves out
an argument. For example, a package may define `def scan(lower=float('-inf')):`
to use negative infinity as its lower bound. NaN means "not a number," such as
the result represented by `float('nan')`.

These models describe CPython, the Python implementation used to run the
independent examples. They assume scalar inputs. Program code must not replace
the selected standard-library operations or change how Python
loads them. For example, assigning a custom function to `math.floor` falls
outside these assumptions. Scalar inputs here mean ordinary numbers,
strings and booleans rather than objects with custom operator methods. The
answer records these assumptions. It also records competing project files,
including type descriptions (`.pyi` files), compiled Python files (`.pyc`) and
modules compiled from other languages (`.so` or `.pyd`). A local `math.py`
prevents treating that import as the trusted standard-library module.

An empty selection, `standard_library=()`, requests no standard-library models.
The Python client requires this setting explicitly. It does not infer models from
an imported module's name.

## Read the result

In the JSON answer, `complete: true` means the question finished within the
recorded rules and assumptions. A witness records a route through the checker's
rules from the chosen input to the output. A finished answer without a witness
means the analysis found no such value route within those rules.

`complete: false` means some part remains unchecked. Its `cutoffs` explain why.
An unfinished answer with no witness does not establish that the input is
blocked. For example, an unknown call might transform or replace a value in a
way the analyzer cannot check.

A recorded route does not prove that changing the input changes the output.
For an integer `value`, `value - value` always produces zero, but the checker
can still record a route through the subtraction.

The check also leaves out the effect of a condition choosing between constants:

```python
def entry():
    return record_value(1 if emit_value() > 0 else 0)
```

Inputs -1 and 1 produce different outputs, zero and one. The checker does not
count that choice as carrying the input's value into the output. A finished
answer without a witness therefore stays within these value-tracking rules;
it does not establish that the output is independent of the input in every way.

This is also separate from numeric correctness. Knowing that the input reaches
`clamp` does not prove that its bounds are in the right order. That requires a
different question and checks.

## Check dates independently too

The second development example calls the original `dateutil.easter.easter`
with a year and the explicit Western Easter method, `3`:

```python
from dateutil.easter import easter

def entry():
    return record_value(easter(emit_value(), 3))
```

The independently stated Python results are March 31 for 2024 and April 20 for
2025. Overwriting the year with 2000 produces April 23, 2000 for either input.
The selected models are `builtins,datetime,sys`. The date constructor produces
an immutable date: its contents cannot change after construction.

The collector runs ordinary Python separately and compares the results with
these stated examples. The analyzer's answer concerns the input's connection
to the date, rather than proving the Easter algorithm correct.

## Follow a customer label through Stringcase

Stringcase is a text utility library. Its `lowercase` helper changes uppercase letters to lowercase:

```python
from stringcase import lowercase

def entry():
    return record_value(lowercase(emit_value()))
```

The independently stated results are `foobarbaz` for `FooBarBaz` and `customer_label` for `CUSTOMER_LABEL`.
Overwriting the input with `'PUBLIC'` produces `public` for either input and removes the original value connection.
The checker keeps the original module and import unchanged.

Select `builtins,re` explicitly. The `re` module supplies Python's regular-expression tools, which match text patterns.
Stringcase imports that module, so the checker needs to recognize its references.
This selection keeps calls to those tools unchecked; this question needs only string conversion and lowercase conversion.
The [text lesson](check-language-behavior.md#follow-customer-text-through-string-operations) introduces those operations and the receiver checks.

We chose the question and expected results from the README before opening implementation or tests.
After fixing the checker and procedure, its first evaluation passed.
That same executable also passed the earlier Boltons, dateutil and JDcal questions, which are familiar projects now.
The [current record](../tests/agent-eval/results/2026-10-24-pr573-python-text-package/result.json)
retains all four questions, renamed and moved callers, unknown-call refusals, exhausted budgets and saved-answer controls.

The reserved question allows at most 16 Python modules and 256 KiB of source.
A module is Python code loaded by an import; one KiB means 1,024 bytes.
The report can use at most 256 KiB. Work stops after 4,096 analysis steps or a helper-call chain deeper than eight.
An earlier run stopped during its build when we found that a reused test worker requested a larger report.
GitHub confirms that run skipped source inspection. The fixed worker receives the declared report limit explicitly.
The frozen record preserves that stop alongside the successful first inspection.

These results concern the selected questions. Full Toolz, Funcy and Pydash imports still leave incomplete answers.
The comparison checks correctness; it measures no speed or agent savings.

## What the milestone checks must demonstrate

The [combined milestones](package-milestones.md) require original imports,
complete reaching and blocked answers, caller renaming and movement, honest
unfinished answers, and saved-answer invalidation after relevant changes.
Invalidation means refusing to reuse an answer whose supporting source or
import choice changed; it must agree with a fresh analysis.

The same frozen implementation must then work across at least three independent
projects, including one whose implementation stayed unread until that freeze.
The collector preserves failed attempts too. Development examples and unit
tests alone do not complete these milestones.

This work does not measure agent savings. It compares analysis answers with
independently stated Python results. A future performance comparison must count
the analyzer's startup, indexing, queries, output and reuse checks as overhead,
alongside the ordinary tools used to answer the same question.
