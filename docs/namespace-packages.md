# Static namespace packages

Opt-in Python imported summaries now admit namespace directories inside one workspace root.
A directory can supply a package component without an `__init__.py` file. Nested namespace and
regular packages can mix, and selected source entries can live inside either kind.

```python
# space/pkg/leaf.py, without either parent initializer
def identity(value):
    return value

# app.py
from space.pkg import leaf as helper

def render():
    return sink(helper.identity(source()))
```

Analysis follows direct dotted imports, explicit aliases, relative imports and from-import child
fallback. Namespace aliases can pass through explicit re-exports. Function identities and occurrences
still refer to real source files. Merely importing a namespace does not load all its children;
implicit child-attribute traversal remains outside this analysis contract.

The declared import environment contains one workspace root. Python can merge namespace portions
from several search-path entries; this analysis does not model that environment, path changes,
installed packages, native extensions, cached external modules or import hooks. See the
[Python import specification](https://docs.python.org/3/reference/import.html#namespace-packages)
for the broader runtime behavior.

`fr-flow-modules-5` retains five candidates per module component: its directory, `.py`, `.pyi`,
`/__init__.py` and `/__init__.pyi`. A namespace component requires a directory and four absent file
candidates. A final `.py` module may coexist with a namespace directory; the source module wins.
Competing regular-package and source-module candidates remain an explicit ambiguity refusal.
Stubs, ignored sources, symlinks, invalid parents and unresolved imports keep analysis incomplete.
Regular initializers still need the admitted inert module contract.

Namespace paths live in a separate `namespaces` list. They carry no invented source digest or source
origin. Real source modules and namespace directories share the existing 16-module budget; source
bytes, import lookups, component counts and binding-chain budgets retain their previous limits.
Empty namespace directories are explicit filesystem dependencies. No recursive directory listing or
implicit child discovery enters this contract.

The SDK exposes `FlowDependencies.namespaces`, `ImportResolution.namespaces` and
`ImportLookup.namespaces`. It independently derives namespace admission from candidate statuses and
checks closure coverage, module budgets, child fallback, terminal identity and re-export chains.
Legacy dependency schemas one through four remain readable without namespace claims.

Creating or deleting an initializer changes the next input identity. So do relevant missing-file,
stub, competing-module and directory changes. Cached results agree with clean analysis after these
mutations. Unrelated sources and unloaded siblings can retain the same analysis inputs; reuse renews
all source occurrences against the current revision. The dependency also enters persisted task plans.

The [pinned task](../tests/agent-eval/namespace-packages/task.json) starts from an unknown leaking helper
inside a namespace package. Its independent CPython oracle checks public results and AST coordinates.
The delivery evaluator requires discovered repair, a sink-free preview feature, fresh-process
resumption, stale-review refusal, independent patch replay and exact reversal.
The [retained run](../tests/agent-eval/results/2026-09-29-namespaces-namespace-packages/result.json)
passes these checks and binds the current implementation and fixture.
The [runtime tests](../sdk/python/tests/test_namespace_packages.py) cover mixed package trees,
namespace entries, aliases, filesystem boundaries, budgets and forged dependency metadata.
These finite tests do not establish general import equivalence or source correspondence.

Implementation edits use reviewed `fr` previews, saved plans and history applications. The
[dogfood manifest](../tests/agent-eval/results/2026-09-29-namespaces-dogfood/manifest.json) records those
receipts and direct-edit boundaries. Compilation, complete gates and evidence regeneration run on
GitHub; local commands use the resource guard and the existing binary.
