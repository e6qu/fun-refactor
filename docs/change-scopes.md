# Find what a proposed change could affect

Before changing the arguments of `subtotal`, find the callers that may need updating and the tests that may cover them.
`fr` lists relationships it can identify and explains where the search stops. Review those limits before treating the list as complete.

A consumer is code that uses the function being changed. A change scope lists the consumers and related tests found by the search.
For `subtotal(price)`, adding a required `tax` argument can require changes at every caller.
A handle selects the function; a basis connects the later edit to the scope you reviewed.
Missing relationships or unfinished pages mean the caller list cannot yet be treated as complete.

## Commands, results and limits

`fr project change-scope HANDLE...` lists code that may need attention when the selected functions
change. It follows references to each function, then references to the declarations containing those
uses. Each result includes the use location, the containing declaration and the tool's confidence.
Related tests appear as candidates. You still choose checks that exercise the requested behavior.

```sh
fr --json project find subtotal
fr --json project change-scope '<HANDLE>' --depth 4 --nodes 128 --references 512 --bytes 65536
```

Illustration: `checkout` calls `subtotal`, and a test calls `checkout`. The search can lead from
`subtotal` to both declarations. If it stops at a limit, read `cutoffs` before changing the arguments.
A caller in another repository or a call selected at runtime may remain outside this search.

The selected handles must identify distinct declarations. Resumption selectors retain the exact file,
name, declaration kind and identifier byte range. For example, `selected` at the top of a Rust file
and `selected` inside a module retain different selectors. A changed declaration invalidates the
saved scope; resumption cannot silently choose the other one.
Textual mentions appear separately and do not create consumer edges. Their files also require check
associations. For example, a README showing `subtotal(2, 3)` needs review during a rename even though
it does not execute that call. A text match alone does not authorize rewriting an executable string.
Unresolved same-name references remain separate candidates. Weak resolved references keep their confidence
and prevent review readiness. Module-level references retain their file even without a containing declaration.

Depth bounds expansion into new consumers. References from frontier declarations still appear, but an
unvisited consumer reports a depth cutoff. Node, reference and response budgets also produce explicit
cutoffs. Output truncation removes whole records and never preserves a complete or ready claim.
The scan coverage describes the selected root, including skipped files and syntax gaps.
Unsupported files remain reported outside the indexed relation; an exported patch does not make
indexed discovery incomplete. Skipped supported files, symlinks and parser gaps still prevent readiness.

## Declared check associations

Put exact workspace-relative file associations in `.fr/check-scopes.json`:

```json
{
  "schema": 1,
  "checks": [
    {"name": "api-behavior", "paths": ["pricing.py", "api.py", "test_api.py"]}
  ]
}
```

Every name must already exist in `.fr/checks.json`. Paths are exact, normalized file names; globbing,
directory expansion and command inference do not occur. Missing or unindexed declared paths remain
visible. The map permits 32 check names, 64 paths per check and 64 KiB in total. Symlink traversal refuses.

A check becomes a candidate when its declared paths intersect affected files. The report lists uncovered
files and missing associations. `review_ready` requires complete indexed discovery, no unresolved or weak
references, mapped affected files and at least one declared check. It does not prove test execution or
runtime coverage. External consumers, reflection and dynamic dispatch (choosing a callable from runtime values) remain outside this relation.

## Scope-bound changes and resumable plans

```python
from fr_ir.change_scope import ChangeScope

scope = ChangeScope.inspect(client, [handle])
review = client.review(scope.bind(change))
# Inspect the mutation, selected checks and scope before execution.
result = client.execute(review)
```

The optional task-change binding makes native preview and execution revalidate the complete scope.
Every edit target must belong to the discovered declaration set. Every declared candidate check must
appear in the task's lifecycle or acceptance selection. Extra checks remain permitted.
Use the scope binding when delivery must recheck this caller search before applying the edit.

`scope.dependency` captures the whole selected workspace, declaration selectors, query limits,
check declarations, association-map bytes and analyzer rules. Adding a consumer, changing configuration,
deleting a target or introducing ambiguity invalidates it. This is conservative whole-workspace validation;
it does not provide incremental consumer analysis.

Use that dependency in a `TaskStep`, explicitly start the step, then call
`scope.observe(plan, client, step_id)` to retain the discovery observation. The helper revalidates inputs
and refuses incomplete reports or stale steps. Persist and restore reports through `scope.persist(store)`
and `ChangeScope.restore(store, digest)`. Verified Merkle records detect altered stored bytes; they do not
authenticate an untrusted producer.

Scope discovery and checked behavior are separate evidence. Use [checked outcomes](checked-outcomes.md)
to complete the delivery step. A repair changes the discovery inputs: explicitly rediscover and refresh
a prerequisite when a post-change plan requires it. Successful delivery cannot renew a stale discovery step.

The Rust/Lean readiness comparison covers all 16 Boolean admission cases. Its theorems concern that
small predicate; they do not prove the index, consumer closure, test catalog or source behavior.

The [pinned evaluation](../tests/agent-eval/results/2026-10-27-m3-show-fixed-change-scope/manifest.json)
compares independent Python inspection of the parsed syntax tree with native discovery, then checks two-file delivery,
resumption in a new process, added-consumer invalidation and independent patch replay.
The [SDK tests](../sdk/python/tests/test_change_scope.py) exercise budgets, ambiguity, coordinates,
mapping drift and refusal boundaries. This is deterministic acceptance, not a live-agent trial.
