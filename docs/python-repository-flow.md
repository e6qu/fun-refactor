# Can Python tracing help in a real project?

Suppose your application imports a utility library and sends a value through one of its helpers.
You want to know whether user input can reach the output, including what the import does.

A library import can execute a package's `__init__.py` before making its helper available.
That startup code can change names or shared objects, so reading only the helper's body can miss relevant behavior.
A source is the configured input call; a sink is the configured output call.
The examples use `read_input()` and `send()` to illustrate those roles; the test setup supplies their implementations.
A cutoff records why analysis could not finish. Read it before interpreting an empty trace.

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

This page's recorded tests use Toolz, funcy and pydash. They check helpers through their real packages,
then compare some helpers in small files containing the copied function and a declared control.

The real-package answers still remain incomplete or refuse the deliberately invalid output budget.
Some isolated helper examples complete. That shows a supported helper can work in a simpler setting;
it does not show that the original package is understood.
Later [original-package checks](package-milestones.md) finish selected Boltons and dateutil
questions. Broader project coverage remains a separate requirement.

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
Custom bases, instances with custom constructors or finalizers, decorators and other executed class-body code remain unchecked.
Unused classes can now declare a single unshadowed `object`, `dict` or `Exception` base. The
[Python guide](python-contract.md) shows why defining a cache type differs from running its constructor.
The collector now also copies the complete original primitive module unchanged and tests `inc` and `dec` through it.
This removes package parents from that separate test context. It cannot establish full-package support.

The first cohort uses pinned Toolz, funcy and pydash snapshots with the same native analyzer,
frozen at `aad6dc34cb48ff27c01ad964a425c94167053347`. The
[selection record](../tests/agent-eval/python-repository-flow/cohort.json) declares revisions,
source/sink rules, resource budgets and the first held-out opening. These are three independently
maintained utility libraries, not a representative sample of Python applications.

Each authored adapter imports the upstream identity helper through its real package. One entry
passes the selected value through two helper calls to a sink. Another calls the helper, overwrites
the value and sends zero through the helper to the same sink. Independent inspection of the parsed syntax tree checks
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
**B.repository-flow remains open.** The [current status](roadmap-status.md) owns the overall requirement counts.
These results establish neither general library support nor agent efficiency.

Retained cutoffs include unchecked module effects, cyclic initialization, unsupported import
forms and unresolved standard-library members. Pydash also hits the import-lookup budget and
an unsupported variadic parameter contract. These boundaries need general solutions; discarding
imports or treating an unresolved call as harmless would not establish a complete answer.

Pydash source first opened after coverage passed on the same collector revision. Its first
inspection failed because the harness treated type-only overload declarations as runtime
implementations. General inspector and runtime-dependency corrections followed; the native
analyzer never changed. The record preserves that first opening. Pydash cannot serve as a fresh
project kept unread during development for a later analyzer implementation.

Collection also exposed two harness failures: an invalid requested call depth and an evidence
directory restored by the compiler cache. Preflight validates public CLI budgets, and collection
now writes to a unique `RUNNER_TEMP` directory. The exact failed report and snapshots remain
historical evidence; a pre-download failure records its run and reason. Every timeout stays unchanged.

The hosted retainer verifies archive digests, producer attempts and recorded input fingerprints, replays
runtime observations, and checks altered records after rehashing the modified receipts. Snapshots
retain full source inventories and upstream licenses. The
[fixture guide](../tests/agent-eval/python-repository-flow/README.md) describes the collector.

The fixed-slot correction uses `python-scalar-summaries`. It admits unused, unannotated
variadic declarations only for calls that bind fixed slots. Referenced variadic values and extra
arguments remain incomplete. The [current regression collection](../tests/agent-eval/results/2026-10-17-value-aliases-fixed-arity-probes/result.json)
replays these thirty already-seen cells and requires each upstream identity helper to enter the
summary solver. Pydash's parameter rejection disappears. All 24 emitted reports remain incomplete,
and the six deliberate CLI refusals remain refusals. The remaining import and initialization
boundaries still prevent the real-project requirement. The original baseline bytes remain intact;
its historical audit uses a pinned snapshot of the original source and collector.

The expanded collector also checks Funcy's `inc` and `dec`, Pydash's `add` and Toolz's `return_none`.
Each helper runs through its original package and through an explicitly isolated declaration module.
The latter copies the exact runtime function and an uncalled variadic companion from the pinned source.
It also adds the declared authored binding `probe_metadata = 0` to exercise inert scalar initialization.
That binding is a control, not an upstream source fragment.
Independent inspection of the parsed syntax tree checks the scalar operation, signature and both source fragments.
Two runtime inputs, renamed/relocated adapters, unknown calls and budget controls cover both contexts.
The collector keeps package outcomes separate from isolated declaration results. Isolated success cannot
establish the real-project requirement or erase unchecked initialization effects in the original packages.

| Expanded context | Cells | Complete | Incomplete | CLI refusals |
| --- | ---: | ---: | ---: | ---: |
| Four additional helpers through their original packages | 40 | 0 | 32 | 8 |
| Exact helper/companion declarations in isolated modules | 40 | 16 | 16 | 8 |
| Two helpers through the complete unchanged primitive module without package parents | 20 | 8 | 8 | 4 |

These one hundred cells add 200 runtime executions to the original thirty cells and sixty executions.
All contexts preserve renamed/relocated conclusions. Isolated declarations and the standalone primitive module yield some complete answers.
Unknown calls, exhausted budgets and full package initialization still prevent complete answers.
The package boundaries and the unopened holdout keep B.repository-flow open.

Sharing wildcard provider searches lets the current replay reach later imports without raising a limit.
Funcy's package initializer now needs 14 searches instead of 126. Its imported `EMPTY` marker resolves,
and `funcmakers.py` enters the loaded code. Sharing names in an ordinary `from` statement then
reduces the representative `inc` case from 120 searches to 75; the 16-file limit still applies.
For example, `from helpers import add, subtract` searches for `helpers` once and retains both bindings.
Toolz also clears its import-search cutoff. The current Pydash replay uses 47 searches
and reaches the separate 256 KiB source limit. Its unchecked startup statements fall from 510 to 420
across the same eight files.
The full-package outcomes in the table stay unchanged. The
[roadmap](../PLAN.md#r2-follow-shared-lists-through-original-imports) names the full-package questions and independent-project check still required.

The callback-table change was also replayed against these same familiar libraries.
The [registry replay](../tests/agent-eval/results/2026-10-15-registry-startup-fixed-arity-probes/result.json)
keeps the full-package totals at 32 incomplete cases and eight deliberate budget refusals.
Unchecked startup counts stay at 92 for Toolz, 80 for Funcy and 420 for Pydash, across the same loaded files.
Small callback tables now work, but this change has not removed the libraries' remaining blockers.

The [import-order replay](../tests/agent-eval/results/2026-10-16-import-order-fixed-arity-probes/result.json)
checks the same familiar source snapshots after allowing imports to return to a file already loading.
A second file can use a function the first file has already defined. Reading that function before its definition remains incomplete.
The tests check that requested files load before imported names become available in the caller.
For `from helpers import *`, the `__all__` list supplies the names and their order. The tests preserve that order.
The [guide's example](python-contract.md#follow-imports-that-return-to-a-file-already-loading) shows why changing the order can make Python raise `ImportError`.

All 40 full-package cases remain unfinished: 32 incomplete analyses and eight deliberate invalid-budget refusals.
The isolated declarations still have 16 complete answers and the standalone module has eight, across the same 100 cases and 200 runtime executions.
Import-order support improves small programs; it does not remove the packages' other startup and import boundaries.

The [startup-diagnostics replay](../tests/agent-eval/results/2026-10-16-startup-diagnostics-fixed-arity-probes/result.json)
keeps the same 100 expanded cases and 200 runtime executions, with the same outcomes in the table above.
Its reports also count unchecked statements in each loaded file, including statements beyond the sixteen displayed locations.

The [function-default replay](../tests/agent-eval/results/2026-10-16-function-defaults-fixed-arity-probes/result.json)
checks the same libraries after allowing unused functions to store earlier functions or built-in objects as defaults.
For example, `factory=dict` stores Python's dictionary type; `factory=dict()` would create a dictionary during definition.
Toolz has nine such `factory=dict` declarations in `dicttoolz.py` and one `map=map` declaration in `sandbox/parallel.py`.
Those ten declarations no longer count as unchecked startup code. This does not execute their bodies or admit calls through their callback arguments.
All full-package outcomes stay unchanged: 32 incomplete cases and eight deliberate invalid-budget refusals.

The [parallel-assignment replay](../tests/agent-eval/results/2026-10-16-parallel-bindings-fixed-arity-probes/result.json)
checks these familiar libraries after allowing several existing functions to be saved in one assignment.
For example, `_map, _filter = map, filter` stores Python's built-in functions without calling them.
All values on the right are read before any new name on the left is written.
The [two-file lesson](python-contract.md#save-several-helper-names-in-one-assignment) explains the rule and shows why swapping the stored helpers can change an input trace.

An identity helper returns its input unchanged. For the three original identity-helper questions,
that replay exposed these groups of unchecked statements:

| Library | Total unchecked statements in loaded files | Example loaded file | Unchecked statements in that file |
| --- | ---: | --- | ---: |
| Toolz | 82 | `toolz/curried/__init__.py` | 45 |
| Funcy | 94 | `funcy/seqs.py` | 18 |
| Pydash | 461 | `pydash/collections.py` | 159 |

Compared with the function-default replay, Funcy's total falls from 98 to 94, with all four removed statements in `seqs.py`.
Toolz and Pydash keep their previous totals. This reduces one source of uncertainty; it does not finish any full-package answer.
The expanded 40 full-package cases still contain 32 incomplete analyses and eight deliberate invalid-budget refusals.
The isolated declarations and standalone primitive module keep the same results in the table above.

The identity-helper queries are separate from the expanded `inc` and `add` questions.
The new `inc` replay reduces Funcy's unchecked count from 80 to 76 across the same sixteen files and 75 import searches.
It still reaches the sixteen-file limit. Pydash's `add` stays at 420 across eight files and 47 searches, with the separate 256 KiB source limit still reached.
Toolz's `return_none` stays at 82 across fourteen files. These limits are unchanged.
Keep the selected helper and loaded-file list fixed when comparing counts.
A decorator is code applied while Python defines a function or class.
Pydash's example file has 155 unchecked decorated declarations and four startup calls.
Toolz's example file has 43 startup calls and two writes.
Funcy's has fifteen unchecked function declarations, two assignments and one statement that controls which code runs.
These counts describe unchecked syntax, not observed executions or complete package support.

The [shared-default replay](../tests/agent-eval/results/2026-10-16-shared-defaults-fixed-arity-probes/result.json)
checks the same libraries after allowing calls to use scalar defaults imported from checked local files.
For example, a helper can import `LIMIT = 0` from a settings file and store it in `value=LIMIT`.
The [three-file lesson](python-contract.md#share-a-default-value-between-files) explains that capture and its limits before showing the commands.
The replay keeps the same completion totals in both tables above.
This general rule helps the new small programs; it does not finish these libraries' remaining startup operations and imports.

The [value-alias replay](../tests/agent-eval/results/2026-10-17-value-aliases-fixed-arity-probes/result.json)
keeps the same completion totals in both tables after allowing checked values to have another name.
For example, `BASE = 0; LIMIT = BASE` can now supply a scalar default across files.
The [complete lesson](python-contract.md#give-a-shared-setting-another-name) introduces aliases before showing the files and commands.
The remaining `concat = chain` in Funcy reads an imported native object whose behavior is still unresolved here.
`cat = chain.from_iterable` also needs attribute access checked. Those operations remain separate work.

The [default-replacement replay](../tests/agent-eval/results/2026-10-17-default-replacements-ci-fixed-arity-probes/result.json)
checks the same familiar libraries after allowing explicit arguments to replace checked object defaults.
Its 100 cases and 200 Python executions keep the preceding completion totals.
All 40 full-package cases remain unfinished: 32 incomplete answers and eight deliberate budget refusals.
The copied declarations keep sixteen complete answers, and the standalone module keeps eight.
These smaller contexts have different import behavior from their original packages.

Compared with the value-alias replay, the same selected-helper questions keep these unchecked startup counts:

| Library and selected helper | Unchecked startup statements |
| --- | ---: |
| Toolz `return_none` | 82 |
| Funcy `inc` | 76 |
| Pydash `add` | 420 |

The file, source-size and import-search limits stay unchanged.
The [complete replacement lesson](replace-python-defaults.md) shows the supported smaller call and the unfinished omitted-object case.
The full packages still need import and startup behavior outside current support.

The [startup-function replay](../tests/agent-eval/results/2026-10-18-startup-registration-ci-fixed-arity-probes/result.json)
keeps the same 100-case outcomes: no complete full-package answers, sixteen complete copied-declaration answers and eight complete standalone-module answers.
The [new lesson](check-python-startup.md) checks a smaller setup function that fills a dictionary created in its own file.
Toolz's actual table builder still uses loops, imported introspection and computed values outside that rule.
The replay establishes neither complete library support nor lower agent cost.

The [shared-startup comparison](https://github.com/e6qu/fun-refactor/actions/runs/39110449189)
keeps those same outcomes. You can now ask about several functions in one file with one startup check within that request.
The [complete lesson](check-python-together.md) follows submitted input through a forwarding helper and a helper returning zero, then runs both in Python.
The questions share the existing work and output limits; unknown startup effects still leave them unfinished.
In that recorded version, a later request checks startup again. The later
[saved-startup lesson](save-python-startup.md) checks reuse between separate questions.
This change has no timing measurements and does not finish the original packages' import checks.

The remaining import and startup boundaries require general language rules.
The [earlier reservation](../tests/agent-eval/python-repository-flow/replacement-holdout.json)
pinned jaraco.functools; its first admission failed. The [package milestones](package-milestones.md)
own that outcome and the subsequent passing JDcal comparison, selected before opening its implementation.

To inspect an incomplete answer, use the initialization locations described in the
[Python guide](python-contract.md#find-the-statement-that-prevents-an-answer).
They point to loaded source statements, such as an evaluated assignment or unsupported class declaration.
The list is bounded and counts omitted locations. It does not establish package success or explain
code in modules the import search could not load.
Earlier Pydash records reached the 128-search limit before annotation directives could be checked.
The current replay clears that obstacle and reaches the separate 256 KiB source limit instead.
The focused postponed-annotation examples pass; the package still needs other imports and startup behavior checked.
Literal package metadata such as
`__version__ = "8.1.0"` can now stay inert without admitting import controls or global reads.
The withdrawn Boltons reservation overlooked earlier repository-delivery coverage; preserve that
mistake rather than treating Boltons as new. Keep unknown effects and exhausted budgets incomplete.
