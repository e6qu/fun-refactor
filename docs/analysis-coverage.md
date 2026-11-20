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
| Conversion errors and caught values | Base-ten conversion of known strings; builtin handlers with local error names | [Independent Python checks](../sdk/python/tests/test_graph_analysis.py) |
| Shared lists | Local, returned and nested identities; signed fixed-index reads/writes; append, extend, clear, insert, pop, reverse; helpers with checked argument binding | [Independent Python checks](../sdk/python/tests/test_graph_analysis.py), [contract examples](../tools/python_contract_cases.py), [effect examples](../tools/python-effects.py) |
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
Calls with missing, duplicate or invalid arguments remain incomplete.
Known helpers can return a list or allocate a new one. Nested lists retain the identity of each contained list;
mutating an inner list reaches its aliases, while replacing an outer slot leaves old aliases intact.
Cleanup can mutate a returned list; rebinding the helper's local name does not replace the pending return.
Cycles are traversed within the same object budget.
Unknown indices, possible out-of-range access, mixed list/scalar receivers, repeated allocation at one
call path, unpacking and recursive list helpers leave the answer incomplete.
A helper's mutation must affect every alias of the same list. Rebinding an alias must leave the original list intact.

## Follow a conversion failure

Use `--standard-library builtins` when tracing supported builtin operations.
`int(str(value))` and `int(str(value), 10)` include a possible `ValueError` path.
A matching `except ValueError as error` can read the caught value; Python deletes that local error name after the handler.
Explicit-base conversions with unknown input types remain incomplete.
One-argument numeric conversion retains the existing scalar value-flow model and discloses unchecked exception paths when its input type is unknown.
A finished trace does not establish general runtime exception safety.

Read [what a result establishes](understanding-results.md) before using an empty graph or trace to justify a change.
