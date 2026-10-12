# What Python behavior does fr model?

`fr project dataflow HANDLE --summaries --rules RULES.json` models possible scalar value
propagation. It can explain how a configured source might reach a configured sink through
assignments, expressions and direct helpers. It does not execute Python or prove that a path
can run. `complete: true` means no cutoff occurred within the declared model and budget.
It does not mean that every Python runtime effect was modeled.

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
| Calls | Evaluate explicit argument values in source order. Bind positional and exact keyword arguments to fixed declaration parameters. Uncalled inert declarations defer their bodies. | No dynamic dispatch, referenced variadic values, extra variadic arguments, unpacking, annotations, evaluated defaults or coroutine/generator execution. Invalid bindings remain incomplete. |
| Defaults | Immutable literal defaults have no source origins; explicit arguments replace them. | Definition-time effects and mutable defaults are refused. |
| Returns | Direct helper summaries substitute parameter origins. Returns and explicit raises terminate their paths. | Exception types, implicit exceptions, handlers, causes and bare raises are outside the contract. |
| Recursion | Direct and mutual recursion grow scalar summaries to a fixed point. Each call substitutes its own arguments. | No modeled normal return does not prove runtime divergence. Stack exhaustion is an implicit exception. |
| Imports | Resolve static local modules, packages, namespaces and re-exports under one root. Preserve each helper's defining file. | No arbitrary search paths, import hooks or module initialization effects. Cycles and unresolved imports stay incomplete. |
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
| Cross-file evidence | Keyword binding, defaults, equal helper names in different files, explicit raise, initialization effects, cycles and missing modules. |
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
