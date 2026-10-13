# Can this Python input reach that output?

Suppose your application reads a user value and passes it to a helper. You want to know whether
that value can reach an output, or whether the code replaces it first.

```python
def choose(left, right):
    return left

def selected():
    return send(choose(read_input(), 0))

def blocked():
    return send(choose(0, read_input()))
```

Tell `fr` which external calls supply input and consume output, then ask for a trace:

```sh
fr --json project dataflow '<HANDLE>' --summaries --rules rules.json
```

`<HANDLE>` is the selected function reference returned by a project query. `rules.json` declares
your assumptions about external input and output calls. The analyzer follows supported code;
it does not execute Python or infer the behavior of every external function.

## Read the answer before relying on it

A trace, called a witness in the report, records a possible connection in the supported model.
An empty trace list answers the question only when the model completes, with its assumptions and
limits still in view. If `complete` is false, read `cutoffs`: the tool could not finish answering.
[Result meanings](understanding-results.md) explains these cases with concrete examples.

Even `complete: true` does not establish every Python runtime behavior. For example, the model
can follow `value - value` back to the input although ordinary numeric execution produces zero.
It also does not account for every exception Python may raise. Use runtime checks for the actual
behavior and keep a model result separate from a safety claim.

## Common questions and limits

| Your question | A supported example | Where the answer remains limited |
| --- | --- | --- |
| “Does overwriting remove the original input?” | `value = read_input(); value = 0; send(value)` | Shared mutable objects and attribute writes need other handling. |
| “Which argument does this helper return?” | `choose(right=read_input(), left=0)` | Unpacking, extra variadic arguments and referenced variadic values remain incomplete. |
| “What if the caller omits an argument?” | `def relay(value=0): ...` called as `relay()` uses the literal default. | A default such as `value=read_input()` executes when Python defines the function and remains unchecked. |
| “Does this expression skip a call?” | `False and send(read_input())` skips both calls. | The model does not decide the truth of arbitrary variables, comparisons or returned values. |
| “Does an unused function run its body?” | An uncalled plain function can contain unsupported body code without executing it during import. | Decorators and evaluated defaults execute during definition. Use the supported future directive for annotations. |
| “Can a plain module constant block the trace?” | Distinct `metadata = 0` and literal package metadata bindings are inert during initialization. | Global reads, rebinding, import controls and evaluated initialization remain unsupported. |
| “Can I follow a helper in another file?” | A supported static local import can retain the helper’s defining file and argument trace. | Missing or ambiguous imports, import cycles and unsupported initialization keep the answer incomplete. |
| “What if the helper raises?” | An explicit supported `raise` ends that modeled path. | Exception types, implicit exceptions and handlers exceed this model. |

For example, an inert declaration does not mean the model can read its global value:

```python
metadata = 0

def entry():
    return send(metadata)
```

Python can read this value. The current analyzer still reports an unknown binding in the function,
so the trace answer remains incomplete. Supporting initialization and supporting global reads are
separate pieces of work.

An unused module can also prepare a literal collection while defining a helper:

```python
# helpers.py
registry = {}
modules = ("calc", "colls", "tree")

def relay(value):
    return value
```

These declarations no longer block an otherwise supported `send(relay(read_input()))` trace.
Module lists, tuples, dictionaries and sets can contain scalar literals and nested literal collections.
Each initializer must fit within 128 named syntax nodes, including wrappers, strings and comments.
Dictionary keys and set members must contain only hashable scalar literals or tuples of those values.
Python's [collection display rules](https://docs.python.org/3.14/reference/expressions.html#displays-for-lists-sets-and-dictionaries)
explain which expressions run while creating a collection.

For example, `registry = {"input": read_input()}` executes a call during import and remains unchecked.
Unpacking, comprehensions, names, formatted or template strings and computed values remain unsupported.
Reading or mutating `registry` also leaves the answer incomplete. The allowance applies to distinct
ASCII module names; class attributes, function defaults and special metadata keep their existing rules.

A helper package can also declare ordinary release and author information:

```python
# helpers.py
__version__ = "8.1.0"
__author__ = "Example contributors"

def relay(value):
    return value
```

When your application calls `send(relay(read_input()))`, these unused literal declarations
no longer prevent an otherwise supported trace. The exception covers `__version__`, `__author__`,
`__license__` and `__copyright__` at module scope, using the same immutable scalar literals as
ordinary module constants. It does not admit evaluated expressions, mutable values, annotated
assignments, repeated bindings or class metadata. Reading `__version__` inside a function still
leaves the answer incomplete.

Assignments to import controls such as `__path__`, `__package__` and `__spec__` remain unchecked.
Python uses these attributes when loading modules; see its
[import rules](https://docs.python.org/3.14/reference/import.html#module-spec).
Other special names remain outside this metadata exception.

A library may create an unused marker while it defines a helper:

```python
class Marker:
    def __repr__(self):
        return 'marker'

EMPTY = Marker()

def relay(value):
    return value
```

This plain declaration and default empty construction do not prevent an otherwise supported answer.
The method body stays deferred. The supported class has no bases, decorators or type parameters.
Its body contains comments, literal strings, `pass`, scalar attributes or supported method declarations with inert defaults.
The call must name an unchanged class defined earlier in the same file and pass no arguments.

For example, adding `def __init__(self): read_input()` makes `EMPTY = Marker()` execute custom code.
That initialization remains unchecked, as do custom `__new__` or `__del__` methods, descriptors and other evaluated class-body statements.
Python's [class creation and construction rules](https://docs.python.org/3.14/reference/datamodel.html#creating-the-class-object)
explain these calls. Imported classes, special class metadata and rebinding remain incomplete.
Reading `EMPTY` in a function also remains incomplete: the scalar model does not trace global object values.

## Trace a helper that has type annotations

Suppose your helper declares types, but your question is whether it forwards an input:

```python
# helpers.py
from __future__ import annotations

def relay(value: MissingType) -> MissingType:
    return value
```

With summaries and imports enabled, fr can follow `send(relay(read_input()))` through this helper.
Python postpones these annotations, so the missing type name does not prevent the module from loading.
The directive must appear at the beginning of the defining module, after any module docstring or comments.
A directive in the caller does not apply to its imported helpers.
See Python's [future statement rules](https://docs.python.org/3.14/reference/simple_stmts.html#future-statements).
Python 3.14 also defers ordinary annotations, but this analyzer supports the explicit directive only.
The [Python 3.14 changes](https://docs.python.org/3.14/whatsnew/3.14.html#pep-649-pep-749-deferred-evaluation-of-annotations)
describe that broader behavior.

The import still matters. fr checks for a local `__future__.py`, package, stub or directory.
It assumes the standard CPython feature only when all five local candidates are missing.
Each lookup retains those missing paths; adding a local shadow changes the saved answer's inputs.
A local shadow remains subject to the ordinary import and initialization checks.

Defaults still execute: `def relay(value: MissingType = read_input()): ...` leaves the answer unfinished.
Decorators, type parameters, annotation inspection and annotated class attributes also remain unchecked.
The supported directive imports only `annotations`, without aliases or other future features.
Single-file analysis cannot accept it because that mode does not check the import candidates.
Annotations containing `await`, `yield`, assignment expressions or type unpacking remain unchecked.
Each supported annotation contains at most 128 syntax nodes.

## Find the statement that prevents an answer

Suppose a helper's module reads a value while Python imports it:

```python
# helpers.py
metadata = read_input()

def relay(value):
    return value
```

Your selected function may only call `send(relay(0))`. An empty trace for that function does not
check the earlier `read_input()` call. The report stays incomplete and points to
`metadata = read_input()` in `helpers.py` as unsupported initialization.

The Python SDK exposes these locations through `analysis.initialization.sites`.
Each site has an exact source occurrence, reasons and plain-language explanations.
For example, use `site.occurrence.path` and `site.occurrence.location.range.start.line`
to find the file and line. `site.occurrence.text(source, revision=analysis.report.at('/revision'))`
reads the corresponding statement from that file's source.

The report shows at most sixteen blocking statements in file and source order.
`analysis.initialization.omitted` counts additional statements or locations omitted to fit the response budget.
An omitted location does not mean the statement was checked. Import resolution errors remain in
the dependency records and cutoffs; this list describes initialization in files the analyzer loaded.
These locations explain a model limitation and do not assert that the statement executed.

## Rules and executable examples

The rule registry and independently written programs live in
[`python_contract_cases.py`](../tools/python_contract_cases.py). Each program has an explicit
CPython outcome and a separate expected analysis result. Every program also runs with renamed
identifiers, extra Unicode text and a different file location. Only source syntax and declared
caller contracts select analyzer behavior; repository names and expected answers do not.

| Area | Contract | Explicit limit |
| --- | --- | --- |
| Operands | Evaluate scalar operands in source order; join their origins. Ignore comments. | Arithmetic values are not interpreted. `x - x` retains the origins of `x`. |
| Assignments | Evaluate RHS leaves once before writes. Match literal tuple/list shapes; write targets left to right. | No starred targets, arbitrary iterables, container bindings or attribute/subscript writes. |
| Expression selectors | `and`, `or`, `not` and conditional expressions preserve evaluation order. Boolean and None literals can select a path. | Variables, numeric values and call results have unknown truth. |
| Comparisons | Transfer operands left to right; later operands may be skipped. | Comparison results remain unknown. Overloaded protocols are outside the model. |
| Statements | Join `if`/`elif`/`else` paths and loop fixed points. Preserve loop `else`, `break` and `continue`. | Statement conditions retain both CFG edges; joins lose correlations. |
| Calls | Evaluate explicit argument values in source order. Bind positional and exact keyword arguments to fixed declaration parameters. Uncalled inert declarations defer their bodies. | No dynamic dispatch, referenced variadic values, extra variadic arguments, unpacking, unpostponed annotations, type parameters, evaluated defaults or coroutine/generator execution. Invalid bindings remain incomplete. |
| Defaults | Immutable literal defaults have no source origins; explicit arguments replace them. | Definition-time effects and mutable defaults are refused. |
| Returns | Direct helper summaries substitute parameter origins. Returns and explicit raises terminate their paths. | Exception types, implicit exceptions, handlers, causes and bare raises are outside the contract. |
| Recursion | Direct and mutual recursion grow scalar summaries to a fixed point. Each call substitutes its own arguments. | No modeled normal return does not prove runtime divergence. Stack exhaustion is an implicit exception. |
| Imports | Resolve static local modules, packages, namespaces and re-exports under one root. Preserve each helper's defining file. Distinct scalar and bounded collection literal bindings stay inert. | No arbitrary search paths, import hooks, evaluated initialization or global value reads. Rebinding, mutations, special attributes, cycles and unresolved imports stay incomplete. |
| Caller rules | Named sources introduce origins; sinks report them; sanitizers remove matching contexts; propagators forward them. | These are user-supplied assumptions, not facts inferred about external functions. |
| Names | Local bindings and shadowing prevent guessed external calls and unbound value claims. | Call binding uses the documented ASCII parameter subset. Python name normalization is not universally supported. |
| Locations | Source revisions and exact UTF-8 byte spans accompany witnesses. Lines and Unicode columns are one-based. | A derivation trace is not an executable path or mutation authority. |
| Budgets and refusals | Unsupported constructs and exhausted searches carry cutoffs and remain incomplete. Invalid entry syntax, invalid budgets and conflicting caller rules refuse analysis. | A CLI error has no completeness or witness answer. Absence of witnesses in an incomplete report establishes no absence of flow. |

The Python reference defines [expression evaluation order](https://docs.python.org/3.14/reference/expressions.html#evaluation-order),
[assignment semantics](https://docs.python.org/3.14/reference/simple_stmts.html#assignment-statements) and
[name resolution](https://docs.python.org/3.14/reference/executionmodel.html#resolution-of-names).
The table describes the smaller `fr` contract. Detailed limits remain in
[scalar assignments](scalar-assignments.md), [expression control](expression-control.md),
[call binding](call-binding.md) and [literal defaults](literal-defaults.md).

## How to interpret differences

For a helper imported with `from helpers import *`, fr can read a fixed export declaration:

```python
# helpers.py
__all__ = ['relay']

def relay(value):
    return value
```

With `--imports`, a call through `relay` keeps its defining file and can carry the input to an output.
A literal tuple works too; an empty list imports no names but still records the imported file.
A listed name beginning with `_` still counts as an export. Python defines these rules in its
[import reference](https://docs.python.org/3.14/reference/simple_stmts.html#the-import-statement).

The supported declaration uses plain quoted ASCII names in one list or tuple.
Computed export sets, direct mutations, aliases to the export list and missing declarations remain incomplete.
Colliding imports or local bindings also remain incomplete. A listed package child must preserve an
existing package object before considering a file. Other import-time effects remain unchecked.

The corpus records deliberate differences rather than hiding them as successful execution
predictions. `3 < 2 < sink(source())` does not call the sink in Python. The analysis may retain
that call because it does not evaluate comparison truth. Likewise, arithmetic cancellation
can leave an origin in the model even when Python returns zero.

An implicit exception is a stronger boundary: `1 // 0` stops Python before the following
statement, while the current scalar model can retain that statement with `complete: true`.
Do not use that result as an exception-safety claim. Explicit raise tests check path termination
and argument ordering; they do not establish exception-type correspondence.

An unresolved import can stop Python before entry while `fr` retains argument origins through
the unknown call. Such a witness is incomplete and cannot establish that the call can execute.
Imported sink traces can end with caller records after the sink. The audit checks the exact
sink site and permits only AST-backed summary calls after it.

Static module resolution follows the [package-flow contract](agent-investigations.md#static-local-imports).
This corpus does not establish arbitrary imports, heap aliases, async behavior or whole-language
correctness. The repository-flow and effects roadmap obligations remain separate.

The authored corpus pairs every program with a renamed/relocated case across fourteen rules.
The generated roadmap status owns the active source-bound inventory counts.
Import cases rename module paths as well as identifiers. Separate Python processes prevent
module caches from leaking between programs. Runtime results distinguish definition failures
from exceptions after calling the entry function. External source and sink hooks remain assumptions.

| Coverage | Examples |
| --- | --- |
| Binding | Selected and discarded arguments for positional-only, ordinary and keyword-only parameters, including mixed separators; seven invalid bindings retain argument effects. |
| Defaults | Omitted and overridden defaults, required/default mixtures, seven literal forms, evaluated defaults and definition failure. |
| Recursion | Direct and mutual return, constant return, independent callers, keyword substitution, explicit raise, no normal return and step exhaustion. |
| Imports | Absolute, relative, dotted, aliased and continued imports; regular, namespace and mixed trees; re-exports, child fallback and module alias chains. Each has positive and negative helper bodies. |
| Cross-file evidence | Keyword binding, defaults, equal helper names in different files, explicit raise, inert scalar bindings, initialization effects, cycles and missing modules. |
| Refusals | Entry syntax errors, local and imported rule conflicts, competing entry stubs, and invalid step, depth and byte budgets. Runtime definition failures and entry exceptions stay separate. |
| Discovery | Two-hop helper discovery with depths two and three; a stopped search has no absence authority. The summary-function count also has an explicit boundary example. |
| Import boundaries | Competing modules/packages and stubs; parameter, assignment and global shadowing; conflicting bindings; module-count, source-byte, lookup-count, component and binding-chain limits. Positive and negative arguments remain independent expectations. |

Hosted collection and corruption controls validate the full inventory, including deliberate
CLI refusals and emitted incomplete reports. B.contract requires every independently reviewed
assertion in the active report. The earlier 252-case report remains historical evidence.
The active source-bound report and acceptance assertions are selected by the
[generated roadmap status](roadmap-status.md#bcontract-specify-reusable-python-behavior).
Repository-general flow and exception/alias semantics remain separate acceptance items.

## Evidence and regression checks

`tools/python-contract.py` collects CPython outcomes and bounded `fr` reports on GitHub runners.
Its offline audit checks the full case inventory, source bindings, caller rules, completeness,
cutoffs, declared model differences and exact AST locations in every witness. It also checks
module inventories, candidate file states, callee identities, parameter kinds and default locations.
Budget-limited reports must retain the exact independently declared admitted file subset; candidate
facts still correspond to the entire authored workspace. A declared binding-chain cutoff checks the
entry lookup's resolution and sixteen-hop length. Renaming preserves both `.py` and `.pyi` suffixes.
The version-three report retains exit codes, stderr and structured CLI errors separately from
successful reports. The audit checks commands and runtime outcomes even when analysis is refused.
The audit memoizes authored runtime executions; retained outcomes are compared on every audit.
Source and binary
identities are retained. Output digests detect changes; they do not attest that execution occurred.

Focused regressions check comments between operands and explicit continuations inside module names.
Both LF and CRLF imports must preserve binding, runtime results and witness locations.
Full builds and evidence collection stay on hosted runners to protect local interactivity.
