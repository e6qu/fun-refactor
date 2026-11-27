# Find code, follow calls and trace values

Use graphs to find related code and analysis to understand what that code does.
For example, before changing a price calculation, find its callers, follow the supplied amount,
and check which tests cover the affected files.

## Feature coverage

`fr capabilities` lists support by operation and language. `fr --json audit` describes the available workflows and their limits.
Parsing a file does not imply support for every analysis or edit in that language.
The table below connects the analysis features to their implementation checks.

| Feature | Scope and interfaces | Checks |
| --- | --- | --- |
| Symbols, references and imports | Language readers and resolvers; CLI/project queries and browser workspace | [Capability matrix](../tests/capability_matrix.rs), [qualified calls](../tests/qualified_calls.rs) |
| Code and call graphs | Callers, callees, file-scope calls, dispatch candidates and unresolved calls; CLI JSON/DOT and browser graph details | [Graph exports](../tests/graph_export.rs), [analysis boundaries](../tests/analysis_boundaries.rs) |
| Graph reuse after edits | Index and call-graph caches must reflect changed source and resolution inputs | [Call-graph cache](../tests/call_graph_cache.rs) |
| Source-level flow and provenance | Forward/backward value origins for supported programming constructs; configuration readers for other supported formats | [Forward flow](../tests/flow_forward.rs), [provenance](../tests/provenance.rs) |
| Entry points and reachability | Declared entry points, dispatch candidates and potentially unused code | [Entry-point conventions](../tests/entrypoints_conventions.rs), [hierarchy reachability](../tests/hierarchy_reachability.rs) |
| Cross-language links | Declared configuration readers, file links and supported framework relationships | [Stitching languages](../tests/stitch_languages.rs), [cross-language guide](../CROSS_LANGUAGE.md) |
| Duplicate code and unused declarations | Structural duplicates and reachability from declared entry points; CLI and browser | [Duplicates](../tests/duplicates.rs), [hierarchy reachability](../tests/hierarchy_reachability.rs) |
| Declared types | Source declarations and supported type information; CLI and browser | [Types](../tests/types.rs), [data types](../tests/types_data.rs) |
| HTTP contracts | Supported framework routes and declared HTTP contracts; CLI and project queries | [Routes](../tests/routes.rs), [project queries](../tests/project_cli.rs) |
| Flags and paths | Flag definitions, conditions and path relationships; CLI | [Flags](../tests/flags.rs), [paths](../tests/paths.rs) |
| Affected consumers and checks | Traverse references and relate affected files to declared checks; CLI and Python | [Change scopes](change-scopes.md) |
| Control flow | Supported Python branches, loops, returns, raises, handlers and cleanup | [Fixed-point analysis](../tests/flow_fixed_point.rs), [cleanup tests](../sdk/python/tests/test_finally_flow.py) |
| Value flow across helpers | Parameter binding, recursion, local/imported helpers and scalar summaries; CLI and Python | [Summary tests](../tests/flow_summaries.rs), [imported flow](../tests/imported_flow.rs) |
| Package initialization | Original imports, aliases, re-exports, startup effects and missing dependencies | [Package flow](../tests/package_flow.rs), [Python contract](python-contract.md) |
| List and conversion errors | Known list bounds and slice failures, base-ten conversion of known strings, builtin handlers and cleanup | [List error checks](../sdk/python/tests/test_list_exceptions.py), [conversion checks](../sdk/python/tests/test_graph_analysis.py) |
| Shared lists | Local, returned and nested identities; signed fixed-index reads/writes; append, extend, clear, insert, pop, reverse, copy; slices, concatenation and deletion; helpers with checked argument binding | [Independent Python checks](../sdk/python/tests/test_graph_analysis.py), [contract examples](../tools/python_contract_cases.py), [sequence checks](../sdk/python/tests/test_list_sequences.py), [effect examples](../tools/python-effects.py) |
| Dictionaries and variadic calls | Shared mapping identities, literal keys, mutation, keyword expansion and packed helper arguments | [Python comparisons](../sdk/python/tests/test_mapping_calls.py) |
| Saved answers | Reuse only when source, imports, settings and analyzer inputs still match | [Saved flow checks](../sdk/python/tests/test_flow.py), [effect checks](../sdk/python/tests/test_runtime_effects.py) |

These links identify checks, not a claim that they passed on every revision.
The combined [graph-and-analysis PR #621](https://github.com/e6qu/fun-refactor/pull/621) passed its hosted checks and evidence audits before merge.
Refactoring, translation and public-interface completion have their own PRs in that plan.

The maintained language matrix comes from [the capability registry](../src/capabilities.rs), available as `fr capabilities --json`.
It distinguishes implemented operations, refused operations and operations that do not apply to a language.
[Capability drivers](../tests/capability_claims.rs) exercise each advertised language/operation pair;
the [coverage report](../tools/capability-report.py) rejects unexercised claims in the complete native gate.
Those drivers establish example coverage. The behavioral tests above check resolution, mutation and invalidation in more depth.

This guide covers symbols, calls, impact, flow, provenance, entry points, stitching, duplicates, dead code, declared types and HTTP-contract analysis.
The [refactoring guide](refactoring-coverage.md) covers rename, deletion, restructuring, extraction, inlining, signatures, rewrites, imports, flag removal, moves and translation.
Both sets remain product requirements. Per-language refusals document current limits; they do not cancel roadmap promises.

## Inspect a graph

```sh
fr --json graph
fr graph --dot
fr project find calculate
fr project calls '<HANDLE>' --direction incoming --limit 8
```

Replace `<HANDLE>` with the full handle returned by `project find`.
The graph JSON includes declaration spans and each call's file, byte offset, line and column.
`source_digests` binds those locations to the indexed source contents; a missing snapshot has a null digest and a coverage gap.
Two calls on the same line remain separate sites. `unresolved` lists calls without a known target;
`file_scope` lists calls outside functions. `coverage.gaps` reports skipped or unsupported files and analysis failures.

A dispatch candidate is a possible target. Source analysis does not establish which target a running program selects.
`all_visible_calls_resolved` concerns the indexed workspace; external callers and runtime dispatch remain open.
The browser's `graph()` result includes the same export under `details` beside its summary counts.

## Trace a shared list

```python
def replace_last(items, *, value=0):
    items[-1] = value

def entry():
    values = [read_input()]
    alias = values
    replace_last(items=alias)
    send(values.pop())
```

With `read_input` and `send` declared as source and sink, the trace should show that the replacement removes the original input connection.
Changing the helper to append the input instead should preserve that connection when the appended item reaches `send`.
Use the [Python guide](python-contract.md) to declare these rules and run `project dataflow --summaries --imports`.

The list model allows up to 64 objects and 64 items per object within the existing query budgets.
It tracks aliases of local lists, signed constant indices and checked positional/keyword calls to known helpers.
Negative indices follow each possible list length after a branch. Lists with one known identity and length support
`insert`, `pop` and `reverse`; `pop` carries the removed item’s flow into its result.
Helpers preserve positional-only and keyword-only rules and checked scalar defaults.
Expanded and variadic calls raise `TypeError` for missing, duplicate or invalid arguments. Other invalid calls remain incomplete.
Known helpers can return a list or allocate a new one. Nested lists retain the identity of each contained list;
mutating an inner list reaches its aliases, while replacing an outer slot leaves old aliases intact.
Cleanup can mutate a returned list; rebinding the helper's local name does not replace the pending return.
Cycles are traversed within the same object budget.

For one known list identity and length, slices accept omitted bounds, `None`, and signed integer
constants, including negative steps. Bounds clip to the list; a zero step raises `ValueError`.
`items[:]`, `items.copy()` and `left + right` allocate new outer lists while sharing nested lists.
Replacing an item in the copy leaves the original outer list intact; mutating an inner list reaches both.
Slice assignment snapshots the replacement after evaluating the target, so self-replacement is safe.
A step-one slice can grow or shrink; extended slices require matching replacement lengths.
`del items[index]` and `del items[slice]` update every alias of the outer list. Multiple deletion targets
run left to right. Mismatched extended slices raise `ValueError` before replacing any items.
Unknown bounds, arbitrary replacement iterables and
ambiguous identities or lengths remain incomplete. The same 64-object and 64-item limits apply.
Unknown indices, mixed list/scalar receivers, repeated allocation at one
call path and recursive list helpers leave the answer incomplete.
A helper's mutation must affect every alias of the same list. Rebinding an alias must leave the original list intact.

## Unpack lists and tuples

With summary analysis, a known list or tuple can supply flat, nested and starred local targets:

```python
def entry():
    inner = [read_input()]
    values = (0, inner, 0)
    first, *rest, last = values
    rest[0][0] = 0
    send(inner[0])
```

`rest` is a new list, but its item still refers to `inner`. This example therefore sends zero.
A scalar captured by unpacking keeps its value when the original list slot changes.
The right side is evaluated once, then targets are written from left to right.
A length mismatch raises `ValueError`; a known noniterable literal raises `TypeError`.
A failed nested pattern preserves earlier target writes. Handlers and `finally` see those writes.

`[*items, extra]`, `(*items, extra)` and `helper(*items)` expand bounded lists and tuples.
Helper calls keep positional-only and keyword-only rules, defaults and shared list identities.
Python evaluates positional expressions before keywords. A sole `*items` is expanded after
keyword evaluation; with other positional arguments or expansions, it is expanded earlier.
This matters when a keyword expression mutates `items`.

The existing limits remain: 64 objects, 64 items per sequence, 64 expanded arguments and
64 target writes, with pattern nesting up to 16. Ambiguous lengths, arbitrary iterators,
custom mapping expansion, arbitrary iterators and tuple mutation remain incomplete.
Module setup accepts nested literal patterns and literal expansion within its existing
128-node limit; starred module targets and evaluated setup remain unchecked.
[Runtime and cache tests](../sdk/python/tests/test_sequence_unpacking.py) check these distinctions.

## Trace dictionaries and variadic calls

Known dictionaries share the collection heap with lists and tuples. Reading an item
captures its current value; mutating a nested collection reaches every alias of that
collection. `copy()` and `{**table}` create new outer dictionaries with shared inner objects.
Reads, writes, deletion, `get`, `pop`, `setdefault`, `update` and `clear` support known
literal string, integer, Boolean and `None` keys. Missing reads, deletions and `pop`
without a default raise `KeyError`; handlers and cleanup see earlier mutations.

```python
def clear_input(**options):
    options["items"][0] = 0

def entry():
    items = [read_input()]
    clear_input(**{"items": items})
    send(items[0])
```

This sends zero: each call creates a fresh keyword dictionary, but its values keep their
nested identities. `*args` similarly creates a tuple. Helpers can read, return and forward
these values. Duplicate keyword merges and invalid binding preserve Python's evaluation
order and raise `TypeError`. Builtin `dict` construction requires the selected builtin model.

The same limits apply across collections: 64 objects, 64 items and 64 expanded arguments.
Unknown keys, ambiguous dictionary shapes, custom mappings, mapping iteration and recursive
collection helpers remain incomplete. [Independent checks](../sdk/python/tests/test_mapping_calls.py)
compare runtime behavior, imported-helper cache invalidation and limit boundaries.

`left | right` combines two known dictionaries into a new outer dictionary.
Right-hand values replace equal keys without moving their insertion positions.
Nested objects remain shared. Both operands run before the model reads their
contents, so a helper's mutation during right-hand evaluation remains visible.

For a local name, `table |= other` updates the existing dictionary and every
alias. Like `update`, it accepts a known dictionary or bounded lists/tuples of
two-item lists/tuples. Selected builtin `dict` construction accepts those pairs
too. Pair keys must have known string, integer, Boolean or `None` values.
Known unhashable list/dictionary keys raise `TypeError`; malformed pair lengths
raise `ValueError`. Earlier updates survive either failure, including through
helpers and cleanup. Keyword updates run only after the positional input succeeds.

The existing 64-object and 64-item limits remain. Tuple keys, arbitrary iterators,
custom protocols and indexed or attribute targets for `|=` remain incomplete.
Binary `|` requires dictionaries; it does not accept a list of pairs.
[Runtime comparisons](../sdk/python/tests/test_mapping_operations.py) cover copying,
aliases, evaluation order, partial updates and these boundaries.

## Follow a failed list operation

With `--summaries --imports --standard-library builtins`, known out-of-range reads,
writes, deletions and `pop` calls raise `IndexError`. A handler for `LookupError`,
`Exception` or `BaseException` can catch it too. If an index fits only some possible
list lengths, the model retains both successful and exceptional paths.

```python
def clear_then_fail(items):
    items[0] = 0
    items.pop(9)

def entry():
    values = [read_input()]
    alias = values
    try:
        clear_then_fail(values)
    except IndexError:
        send(alias[0])
```

The handler receives the list after its element was replaced, so this example no
longer connects the input to `send`. Mutations before a failure remain visible;
work after it does not run. A failed assignment evaluates its replacement before
its target. Multiple deletion targets run in order and keep earlier deletions.
`finally` runs before the caller handles the error and can replace a pending return
or error. The report locates each modeled list failure in `events`.

List error messages do not inherit the stored elements' origins. Unknown index
types, ambiguous list mutations and arbitrary user-defined operations remain
outside this bounded model. Exception paths require summary analysis; no general
runtime safety claim follows from a complete result.

## Follow a conversion failure

Use `--standard-library builtins` when tracing supported builtin operations.
`int(str(value))` and `int(str(value), 10)` include a possible `ValueError` path.
A matching `except ValueError as error` can read the caught value; Python deletes that local error name after the handler.
Explicit-base conversions with unknown input types remain incomplete.
One-argument numeric conversion retains the existing scalar value-flow model and discloses unchecked exception paths when its input type is unknown.
A finished trace does not establish general runtime exception safety.

Read [what a result establishes](understanding-results.md) before using an empty graph or trace to justify a change.
