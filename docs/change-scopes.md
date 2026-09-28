# Consumer scope before delivery

`fr project change-scope HANDLE...` discovers indexed consumers before a reviewed change.
It follows references to each selected declaration and then references to their lexical containing
declarations. Cycles terminate. The report retains every admitted reference's exact occurrence,
confidence and consumer, plus built-in test candidates among the reached declarations.

```sh
fr --json project find subtotal
fr --json project change-scope '<HANDLE>' --depth 4 --nodes 128 --references 512 --bytes 65536
```

The selected handles must identify distinct declarations. Stable resumption selectors use exact file,
name and declaration kind. Multiple matching declarations refuse; this route does not select an overload
or same-named class method by position. Textual mentions do not create consumer edges.
Unresolved same-name references remain separate candidates. Weak resolved references keep their confidence
and prevent review readiness. Module-level references retain their file even without a containing declaration.

Depth bounds expansion into new consumers. References from frontier declarations still appear, but an
unvisited consumer reports a depth cutoff. Node, reference and response budgets also produce explicit
cutoffs. Output truncation removes whole records and never preserves a complete or ready claim.
The scan coverage describes the selected root, including skipped files and syntax gaps.

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
runtime coverage. External consumers, reflection and dynamic dispatch remain outside this relation.

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
An absent binding preserves the previous explicit task-change workflow.

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
