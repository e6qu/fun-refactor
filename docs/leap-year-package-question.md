# Does the year reach a leap-year answer?

Suppose an application asks whether a user-supplied year has February 29.
JDcal documents `is_leap(2000)` returning `True` and `is_leap(2100)` returning
`False`. These are boolean values: yes and no.

The question uses the original package import:

```python
from jdcal import is_leap

def entry():
    return record_value(is_leap(emit_value()))
```

Here `emit_value()` supplies the selected input, and `record_value()` receives
the answer. The comparison collector supplies both functions and places the pinned
package in a prepared directory before running this caller.
The analyzer follows a possible value route through the calculation.
Separately, ordinary Python executes both examples and checks their stated results.
Overwriting the year with 2000 before calling `is_leap` must remove the original
input's value route. Python must then return `True` for either original input.

We chose the function name and examples from the
[public README at the pinned revision](https://github.com/phn/jdcal/blob/c82c3cfb7f814b2ab79fd597e3f8aa87c3390fbb/README.rst).
The implementation file `jdcal.py` and tests `test_jdcal.py` stayed unread during
development. We recorded the analyzer, client library, question plan and collector
before the first evaluation, which passed. A failure must stay a failure;
choosing a simpler function after opening the code would change the test.

Inspection after the freeze found that `is_leap` computes the year's remainders
after division by 4, 100 and 400. Its return expression uses those remainders.
For 2000 all three are zero, giving `True`. For 2100 the remainder after division by 400 is 100, giving `False`.
The input therefore affects the selected return expression. Replacing it with 2000 removes that connection.
These two examples return booleans; they do not establish the return type for every year.

## Check imported-module behavior first

A Python file can contain a block intended only for running that file directly:

```python
if __name__ == '__main__':
    run_demonstration()
```

Python supplies `__name__` when loading a module. During an ordinary import it
contains the module's name, so this block is inactive. The analyzer now recognizes
this exact guard in an imported dependency when the file contains no other
`__name__` use and the guard has no alternative branch. The selected entry file
does not receive this allowance: its execution mode is unknown.
The [Python module guide](https://docs.python.org/3.14/library/__main__.html)
describes the distinction between importing a module and running it directly.

Rebinding `__name__`, importing it from another file, using a different comparison
or adding `else` keeps the question unfinished. Ordinary Python tests also check
that an inactive block cannot replace the selected helper.

The checker also admits explicitly supported Python 3 future directives, such as
`from __future__ import division`. These request behavior already mandatory in
the declared Python 3 runtime. They do not enable postponed annotations.
The checker handles `annotations` separately. A competing local `__future__` file still
prevents trusting the directive.
The [Python future-statement reference](https://docs.python.org/3.14/library/__future__.html)
records which behavior is mandatory in the selected runtime and explains that
the statement still performs a module import.

The question explicitly selects the `builtins` and `math` standard-library models.
A model is an explicit description of operations the checker knows how to handle.
These models use the assumptions in the
[number and date guide](original-package-questions.md).
An unused imported name such as `math.modf` may be recognized without supporting
calls to it. Calling an unmodeled operation still leaves the answer unfinished.

## What counts as finishing

The passing comparison used one unchanged analyzer for Boltons, dateutil and JDcal.
Check the original and renamed or moved callers, the overwritten input, unknown
calls and exhausted work budgets. Keep the original package files unchanged.
The familiar-project saved-answer checks must agree with fresh analysis after
helper and import changes.

The [milestone record](package-milestones.md) owns the outcome and original run
receipts. This finite comparison establishes three-project coverage for these questions.
It does not prove the calendar algorithms correct, support all Python packages or establish lower agent costs.
