# Follow an input through an original Python package

Suppose your code uses Boltons to keep a number between zero and fifty:

```python
from boltons.mathutils import clamp

def entry():
    return record_value(clamp(emit_value(), 0, 50))
```

Here `emit_value()` supplies an input and `record_value()` receives the output.
The question is whether that input can affect the output. For example, Python
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

These models assume standard CPython behavior, scalar inputs and no replacement
import hooks or monkey patches. Scalar inputs here mean ordinary numbers,
strings and booleans rather than objects with custom operator methods. The
answer records these assumptions. It also records competing project files,
including stubs, bytecode and native extensions. A local `math.py` prevents
treating that import as the trusted standard-library module.

An empty selection, `standard_library=()`, requests no standard-library models.
The Python SDK requires this setting explicitly. It does not infer models from
an imported module's name.

## Read the result

In the JSON answer, `complete: true` means the question finished within the
recorded rules and assumptions. A witness is a recorded possible connection
between the chosen input and output. A finished answer without a witness means
the analysis found no connection within those rules.

`complete: false` means some part remains unchecked. Its `cutoffs` explain why.
An unfinished answer with no witness does not establish that the input is
blocked. For example, an unknown call might transform or replace a value in a
way the analyzer cannot check.

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
