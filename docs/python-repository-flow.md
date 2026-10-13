# Can Python tracing help in a real project?

Suppose your application imports a utility library and sends a value through one of its helpers.
You want to know whether user input can reach the output, including what the import does.

This is an illustration of that question:

```python
from helper_library import add

def handle():
    value = read_input()
    return send(add(value, 1))
```

Finding `add` is useful, but it does not settle the question. Importing a package can run other
code, change bindings or reach imports the analyzer does not understand. A result with no trace
must still say the answer is incomplete when those parts remain unchecked.

## Where the tests leave us

The current tests use Toolz, funcy and pydash. They check helpers through their real packages,
then compare some helpers in small files containing the copied function and a declared control.

The real-package answers still remain incomplete or refuse the deliberately invalid output budget.
Some isolated helper examples complete. That shows a supported helper can work in a simpler setting;
it does not show that the original package is understood. Full real-project tracing remains open.

We also rename helpers and move files to check whether answers depend on the code rather than
familiar names. Python execution checks the stated inputs separately from the analyzer’s answer.
The counts and original attempts below remain available for review.

## An import must select the object Python actually uses

Imagine `helpers/__init__.py` defines a class named `Formatter`, while
`helpers/Formatter.py` also exists. For `from helpers import Formatter`, Python selects the
existing class. Reading the separate file instead can describe a different function and result.

The package resolver now recognizes direct class definitions, decorated definitions and
assignment targets before considering a same-named child file. It keeps their unsupported values
unavailable for scalar calls. An annotation such as `Formatter: object` without a value does not
create that binding; an annotation with a value does.
If an ordinary function definition later replaces an unsupported binding, fr can select that
function. The earlier unchecked startup code still leaves the overall answer incomplete.

The [runtime comparisons](../sdk/python/tests/test_package_bindings.py) check these choices with
and without the child file. They also check saved answers after a binding appears or disappears.
Explicitly importing the child file can replace an earlier package attribute; fr records that
conflict and keeps the answer incomplete. This correction does not execute class bodies,
decorators or arbitrary package startup code, and does not close the real-project tracing gap.

## Recorded runs and exact test setup

Literal `__all__` lists and tuples now give fr the names to follow through `from helpers import *`.
For example, `__all__ = ['inc']` lets the resolver follow an imported `inc` function to its defining file.
An empty list still records the import dependency, and an existing package object keeps precedence over a child file.
Computed or directly mutated export sets remain incomplete. These are general language rules;
the full libraries still contain other initialization and import boundaries.

The class correction covers a familiar marker declaration in `funcy.primitives`:
`EmptyType` has a deferred `__repr__` method, followed by `EMPTY = EmptyType()`.
Plain declarations and default empty construction can stay inert while scalar helpers run.
Custom constructors, finalizers, bases, decorators and other executed class-body code remain unchecked.
The collector now also copies the complete original primitive module unchanged and tests `inc` and `dec` through it.
This removes package parents from that separate test context. It cannot establish full-package support.

The first cohort uses pinned Toolz, funcy and pydash snapshots with the same native analyzer,
frozen at `aad6dc34cb48ff27c01ad964a425c94167053347`. The
[selection record](../tests/agent-eval/python-repository-flow/cohort.json) declares revisions,
source/sink rules, resource budgets and the first held-out opening. These are three independently
maintained utility libraries, not a representative sample of Python applications.

Each authored adapter imports the upstream identity helper through its real package. One entry
passes the selected value through two helper calls to a sink. Another calls the helper, overwrites
the value and sends zero through the helper to the same sink. Independent AST inspection checks
the actual runtime implementation; isolated CPython executions use source values 17 and 31.
The alternate adapter renames its entry and import alias and moves into another package without
changing upstream code or analysis rules. Unknown-call, one-step-budget and invalid-byte-budget
controls run in both forms.

| Result | Count |
| --- | ---: |
| Independently maintained repositories | 3 |
| Original and renamed/relocated cells | 30 |
| Runtime executions per collection | 60 |
| Complete native analyses | 0 |
| Incomplete native analyses | 24 |
| Deliberate CLI refusals | 6 |

Runtime execution agrees with independent inspection. Renaming and relocation preserve the
reported conclusions. The analyzer remains incomplete even for the positive and overwritten
entries in these packages. An empty witness list therefore does not establish no path.
Unknown calls and exhausted budgets stay incomplete; invalid response budgets refuse analysis.
**B.repository-flow remains open**, and roadmap status remains 9/18 demonstrated acceptance items.
These results establish neither general library support nor agent efficiency.

Retained cutoffs include unchecked module effects, cyclic initialization, unsupported import
forms and unresolved standard-library members. Pydash also hits the import-lookup budget and
an unsupported variadic parameter contract. These boundaries need general solutions; discarding
imports or treating an unresolved call as harmless would not establish a complete answer.

Pydash source first opened after coverage passed on the same collector revision. Its first
inspection failed because the harness treated type-only overload declarations as runtime
implementations. General inspector and runtime-dependency corrections followed; the native
analyzer never changed. The record preserves that first opening. Pydash cannot serve as a fresh
held-out project for a later analyzer implementation.

Collection also exposed two harness failures: an invalid requested call depth and an evidence
directory restored by the compiler cache. Preflight validates public CLI budgets, and collection
now writes to a unique `RUNNER_TEMP` directory. The exact failed report and snapshots remain
historical evidence; a pre-download failure records its run and reason. Every timeout stays unchanged.

The hosted retainer verifies archive digests, producer attempts and source bindings, replays
runtime observations, and checks corruptions after rehashing the modified receipts. Snapshots
retain full source inventories and upstream licenses. The
[fixture guide](../tests/agent-eval/python-repository-flow/README.md) describes the collector.

The fixed-slot correction introduces `python-scalar-summaries`. It admits unused, unannotated
variadic declarations only for calls that bind fixed slots. Referenced variadic values and extra
arguments remain incomplete. The [current regression collection](../tests/agent-eval/results/2026-10-13-builtin-bindings-fixed-arity-probes/result.json)
replays these thirty already-seen cells and requires each upstream identity helper to enter the
summary solver. Pydash's parameter rejection disappears. All 24 emitted reports remain incomplete,
and the six deliberate CLI refusals remain refusals. The remaining import and initialization
boundaries still prevent repository acceptance. The original baseline bytes remain intact;
its historical audit uses a pinned snapshot of the original source and collector.

The expanded collector also checks Funcy's `inc` and `dec`, Pydash's `add` and Toolz's `return_none`.
Each helper runs through its original package and through an explicitly isolated declaration module.
The latter copies the exact runtime function and an uncalled variadic companion from the pinned source.
It also adds the declared authored binding `probe_metadata = 0` to exercise inert scalar initialization.
That binding is a control, not an upstream source fragment.
Independent AST inspection checks the scalar operation, signature and both source fragments.
Two runtime inputs, renamed/relocated adapters, unknown calls and budget controls cover both contexts.
The collector keeps package outcomes separate from isolated declaration results. Isolated success cannot
establish repository acceptance or erase unchecked initialization effects in the original packages.

| Expanded context | Cells | Complete | Incomplete | CLI refusals |
| --- | ---: | ---: | ---: | ---: |
| Four additional helpers through their original packages | 40 | 0 | 32 | 8 |
| Exact helper/companion declarations in isolated modules | 40 | 16 | 16 | 8 |
| Two helpers through the complete unchanged primitive module without package parents | 20 | 8 | 8 | 4 |

These one hundred cells add 200 runtime executions to the original thirty cells and sixty executions.
All contexts preserve renamed/relocated conclusions. Isolated declarations and the standalone primitive module yield some complete answers.
Unknown calls, exhausted budgets and full package initialization still prevent complete answers.
The package boundaries and the unopened holdout keep B.repository-flow open.

Next, address the remaining import and initialization boundaries
through general language rules. Reserve an unopened holdout before further analyzer edits and
freeze broader coverage before opening it. The
[next reservation](../tests/agent-eval/python-repository-flow/replacement-holdout.json) pins jaraco.functools.

To inspect an incomplete answer, use the initialization locations described in the
[Python guide](python-contract.md#find-the-statement-that-prevents-an-answer).
They point to loaded source statements, such as an evaluated assignment or unsupported class declaration.
The list is bounded and counts omitted locations. It does not establish package success or explain
code in modules the import search could not load.
The familiar pydash result reaches the 128 import-lookup limit before its annotation directives
are checked. The focused postponed-annotation examples pass; that package still needs a solution
for the other imports and executed initialization. Literal package metadata such as
`__version__ = "8.1.0"` can now stay inert without admitting import controls or global reads.
The withdrawn Boltons reservation overlooked earlier repository-delivery coverage; preserve that
mistake rather than treating Boltons as new. Keep unknown effects and exhausted budgets incomplete.
