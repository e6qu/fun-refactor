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

## Import an unused value beside a helper

```python
# helpers.py
LIMIT = 10

def relay(value):
    return value

# app.py
from helpers import LIMIT, relay

def entry():
    return send(relay(read_input()))
```

The unused `LIMIT` import no longer blocks this trace. The report identifies it as a value
and retains its defining file and declaration position. This covers previously supported
scalar and bounded collection literals, plain classes, and instances created by a plain
class without a custom constructor. Aliased imports, static re-exports and literal export
lists can carry these values too. Names must be ASCII; special metadata imports remain unchecked.

Importing a value does not add a model for reading or calling it. For example,
`send(LIMIT)` and `LIMIT(read_input())` still leave the answer incomplete.
An unused module function such as `def unused(key=LIMIT): ...` can read a previously available value
without running its body. The [default-name example](#read-a-known-name-in-an-unused-functions-default)
explains this declaration rule. Calling a function with a named default remains unfinished.
Computed setup, missing imports, conflicting bindings
and package cycles also remain unfinished. A declared package value takes precedence over
a same-named child file; it cannot be treated as that child to obtain a callable helper.

## Read a known name in an unused function's default

Python reads a default when it defines the function. Reading an already-created object does not
construct it again or run the function body:

```python
class EmptyType:
    pass

EMPTY = EmptyType()

def unused(key=EMPTY):
    return unknown_operation(key)

def relay(value):
    return value

def entry():
    return send(relay(read_input()))
```

The unused function no longer blocks the supported entry trace. Its default reads the earlier `EMPTY`
binding. This covers earlier checked literals, collections, plain classes and supported instances,
including static value imports before the definition. Names must be ASCII and exclude special metadata.
Parentheses, fixed parameter kinds and unused-function variadics follow their existing rules.

Moving `EMPTY = EmptyType()` below the definition is different: Python raises `NameError` while defining
the function. Deleted, conflicting and unknown bindings remain unchecked. Unsupported setup clears
the known bindings available to later defaults. A later declaration cannot repair an earlier read.

Calling `unused()`, even with an explicit replacement argument, remains outside the literal-default
call model because `EMPTY` is an object marker. The rule defers an uncalled body; it does not model shared object defaults or global reads
inside analyzed bodies. `key=EmptyType()`, attributes, subscripts and other computed defaults remain unchecked.
Decorators, class-method defaults and unsupported annotation evaluation keep their separate boundaries.

## Call a helper with an earlier scalar default

You can ask whether supplied input reaches `send` through this helper:

```python
LIMIT = 10

def choose(value=LIMIT):
    return value

def entry():
    send(choose())              # the captured default has no input origin
    return send(choose(read_input()))  # the supplied input reaches send
```

The analyzer accepts an earlier immutable scalar literal assigned in the function's own module.
This includes `None`, Booleans, numbers, ordinary strings and bytes. Parentheses around the name are allowed.
The function may itself be imported or reexported. Its default still belongs to the defining file.
Omitted arguments have no source origins; explicit arguments keep their own origins and effects.

Python captures `LIMIT` when it defines `choose`. Changing or deleting `LIMIT` afterward does not
replace the captured default. Such module changes remain unchecked, so the overall answer stays incomplete.
A forward reference, an earlier rebinding or unknown setup cannot establish a supported scalar capture.
Imported scalar names, aliases of values, collections and object markers remain unsupported for calls,
even when an explicit argument replaces the default. Ordinary global reads in the body also stay unchecked.
The analysis does not specialize branches using the default's concrete value.

## Keep declarations separate from reads

For example, an inert declaration does not mean the model can read its global value:

```python
metadata = 0

def entry():
    return send(metadata)
```

Python can read this value. The current analyzer still reports an unknown binding in the function,
so the trace answer remains incomplete. Supporting initialization and supporting global reads are
separate pieces of work.

A library may update a function's help text after defining it:

```python
# helpers.py
def relay(value):
    return value

relay.__doc__ = "Return the supplied value."
```

Your `send(relay(read_input()))` trace can still follow the original helper.
Python changes the help text; this write does not change the supported scalar call.
The allowance covers a literal text string or `None` assigned to `__doc__` on a plain,
undecorated function already defined in that file, with a supported signature.
Repeated documentation writes are allowed. Replacing the function, importing after its
definition, or executing unchecked code removes this allowance for that function.

Writing through an alias or to an imported function stays unchecked.
So do computed values, formatted strings, byte strings, template strings, annotated or
chained assignments, and other attributes. For example, `relay.__doc__ = read_input()`
executes a call, while `relay.__defaults__ = (0,)` changes omitted arguments.
Both keep the answer incomplete. Reading `relay.__doc__` inside the selected function
also remains unsupported; admitting a write does not add a model of metadata reads.

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
The method body stays deferred. The supported class has no base or one direct, unshadowed built-in
base: `object`, `dict` or `Exception`. Parentheses and comments around that base are allowed.
Decorators, type parameters, multiple bases, unpacking and explicit metaclasses remain unchecked.
Its body contains comments, literal strings, `pass`, scalar attributes or supported method declarations with inert defaults.
Default instance construction requires no base or `object`; constructing a `dict` or `Exception`
subclass stays unchecked. The call must name an unchanged class defined earlier in the same file
and pass no arguments.

For example, a helper module can declare an exception and a cache type before the function you need:

```python
class SkipMemory(Exception):
    pass

class CacheMemory(dict):
    def __init__(self, timeout):
        self.timeout = timeout

def relay(value):
    return value
```

The declarations no longer block a supported `send(relay(read_input()))` trace. Neither constructor
runs merely because its class is defined. An imported class value keeps its defining file and
declaration position. Required base names must be unshadowed in that module; explicit `__builtins__`
access disables this base allowance. A custom base can run `__init_subclass__`, so
`class CacheMemory(CustomBase): ...` remains unchecked even when it looks similar.

For example, adding `def __init__(self): read_input()` makes `EMPTY = Marker()` execute custom code.
That initialization remains unchecked, as do instances with custom `__new__` or `__del__` methods,
descriptors and other evaluated class-body statements.
Python's [class creation and construction rules](https://docs.python.org/3.14/reference/datamodel.html#creating-the-class-object)
explain these calls. Imported classes, special class metadata and rebinding remain incomplete.
Reading `EMPTY` in a function also remains incomplete: the scalar model does not trace global object values.

## Follow a helper through another function name

Suppose a module exposes its helper under a shorter name:

```python
# helpers.py
def relay(value):
    return value

public = relay
```

With summaries enabled, fr can follow `send(public(read_input()))` to `relay`.
The same applies when `relay` was imported from a supported local module before the assignment.
Imports, re-exports and literal export lists can expose `public` to other modules.
The trace retains the original helper's parameter and return locations.
Changing `relay` to return zero removes the connection to that output.

The assignment only binds a new name; it does not call the function.
See Python's [assignment rules](https://docs.python.org/3.14/reference/simple_stmts.html#assignment-statements).
Both sides must be plain ASCII names, with the original name already bound in that module.
The target name must be new, and the source must resolve to a function within the existing sixteen-hop limit.
Import and alias hops share that limit. A single-file alias chain counts its final function too.
Each analysis admits at most 128 function-alias declarations, without changing the import lookup limit.
Dependency records retain each admitted assignment and reject missing or inconsistent alias hops.

For example, `public = relay` before defining or importing `relay` remains unchecked.
So do `public = relay()`, attribute reads, annotated or chained assignments, and rebinding either name.
Calls through stored built-ins, classes, module objects and global values remain unsupported.
Reading the function object in an expression also remains incomplete; supported alias calls are a separate case.
Local shadowing, explicit global declarations, function attribute changes and shared mutable objects still need separate handling.

## Trace a helper beside an unused built-in reference

Suppose a module stores Python's sorting function for another part of the program:

```python
# helpers.py
sorter = sorted
map = map

def relay(value):
    return value
```

With summaries enabled, fr can still follow `send(relay(read_input()))`.
These assignments store functions without calling them. Changing `relay` to return zero
removes the connection to the output. Supported local imports behave the same way.
Calling `sorter`, or reading that function object as a value, still leaves the answer incomplete.

Both sides must be plain ASCII names, and the destination must be new.
The source must be an unshadowed name from Python's [built-in function and type list](https://docs.python.org/3.14/library/functions.html).
`help` is excluded because the `site` module supplies it. Special destination names are excluded too.
For example, `sorted = 0; sorter = sorted` does not qualify as a built-in reference.
A previously defined local function named `sorted` instead follows the ordinary function-alias rules.
Calls, attributes, parentheses, annotations, chained assignments and rebinding remain unchecked.
Any explicit `__builtins__` reference in the module prevents this admission, including a deferred function parameter.
The result assumes ordinary, unchanged built-ins; it does not establish behavior under custom loading or monkey patching.
Python's [name-resolution rules](https://docs.python.org/3.14/reference/executionmodel.html#resolution-of-names)
explain why a module binding takes precedence over a built-in with the same name.

## Keep a built-in type map during import

A library may prepare a map of built-in types before defining the helper you want to trace:

```python
FACTORIES = {
    type(object.__dict__): dict,
    type({}.keys()): list,
    type({}.values()): list,
    type({}.items()): list,
}

def relay(value):
    return value
```

That setup no longer blocks a supported `send(relay(read_input()))` trace.
The map stores known type objects; it does not construct any of its values.
It also preserves earlier known markers for an unused declaration such as `def unused(key=EMPTY): ...`.

Keys may be direct built-in type names or `type(...)` applied to a supported literal, `object.__dict__`,
or an empty dictionary's argument-free `keys()`, `values()` or `items()` call.
Values must be direct built-in type references. The admitted names are `bool`, `bytes`, `complex`,
`dict`, `float`, `frozenset`, `int`, `list`, `object`, `set`, `str`, `tuple` and `type`.
Parentheses and comments are allowed. The whole initializer still fits within 128 named syntax nodes.

The destination must be a new ASCII module name outside special metadata. Required built-in names
must be unshadowed; any explicit `__builtins__` reference disables this allowance.
Custom objects, arbitrary attributes, computed arguments, constructor calls, unpacking and comprehensions remain unchecked.
For example, `type(custom.keys())` may execute user code, while `type({}.keys())` uses an empty built-in dictionary.
An imported map retains its defining file, declaration position and distinct `registry` value kind.
Import aliases, reexports and explicit star exports preserve that record.
Reading, calling, changing or rebinding the map still leaves the answer incomplete.
The rule describes supported startup behavior; it does not establish a working full-package trace.

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
| Imports | Resolve static local modules, packages, namespaces and re-exports under one root. Preserve each helper's defining file. Distinct scalar and bounded collection literal bindings stay inert. Literal help text writes to prior unchanged plain functions preserve scalar calls. | No arbitrary search paths, import hooks, evaluated initialization or global value reads. Rebinding, other mutations and special attribute writes, cycles and unresolved imports stay incomplete. |
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
