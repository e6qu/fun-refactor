# Ordered scalar assignments

Opt-in Python summary analysis admits local-name assignment, chained assignment,
and exact literal tuple/list unpacking. All scalar right-hand expressions run
left-to-right before any target changes. Chained target groups and repeated names
then bind left-to-right, using those retained values.

```python
a, b = source(), 0
a, b = b, a
sink(b)                 # retains the original source
left = right = source() # one RHS evaluation
left, left = source(), 0
sink(left)              # the final write contains no source
```

Nested literal sequences can match nested tuple or list targets. Parentheses around
one name do not introduce unpacking unless they contain a comma. Empty literal
sequences can match empty targets. This follows the admitted portion of
[Python assignment semantics](https://docs.python.org/3/reference/simple_stmts.html#assignment-statements).

The analysis keeps sequence structure only while binding an assignment. It does not
introduce container-valued variables, heap aliases or arbitrary iteration. Starred
unpacking, mismatched shapes, attributes, subscripts and annotations remain explicit
cutoffs. Unsupported shapes do not establish absence of propagation. Implicit
unpacking exceptions and overloaded protocols remain outside the contract.

An assignment admits at most 64 RHS syntax nodes, 64 target-name writes in total,
64 chained target groups and 16 nested shape levels. Parentheses count toward the
shape budget. The existing global transfer and disclosure budgets still apply.
A helper that explicitly raises stops later RHS expressions and prevents target
writes on that normal path. Unknown control alternatives retain the existing may-flow
semantics; witnesses do not prove path feasibility.

Local binding discovery includes every unpacked and chained name, including names
assigned later in a function. A shadowed external rule cannot become a known call
merely because its local assignment has not executed. Definition occurrences use
the exact target spans; swaps preserve RHS origins while linking each target write.

`python-scalar-summaries-5` reports carry `fr-scalar-assignment-1` in both public
metadata and canonical inputs. The SDK exposes `FunctionSummaries.assignment_control`
as an `AssignmentControl`. It verifies the full contract and rejects invented
semantics, non-Boolean flags and disagreements with input metadata. Versions one
through four remain readable without assignment claims.

The analyzer identity includes the assignment implementation. Changes to assignment
sources invalidate cached results; unrelated edits renew exact occurrences. Native
reuse refuses altered assignment metadata even when a caller recomputes the report
digest. Existing join and overwrite kernels remain applicable. Parsing, lowering,
shape matching and source correspondence remain trusted implementation boundaries.

The [pinned task](../tests/agent-eval/scalar-assignments/task.json) finds a leaking
helper through a swap and chained assignment. Its independent CPython oracle checks
repair, a sink-free preview, fresh-process resumption, receiver replay and reversal.
The [runtime corpus](../sdk/python/tests/test_scalar_assignments.py) covers ordering,
locations, refusals, budgets, contract tampering and clean/cache agreement.
The [retained acceptance](../tests/agent-eval/results/2026-09-30-assignments-scalar-assignments/result.json)
binds the fixture, implementation and delivery checks. The baseline distinguishes
the installed local binary from the pinned source revision. These are finite
deterministic cases, with no live-agent or token-saving claim.

Existing function edits use guarded `fr` previews, saved plans and history applications
in the [dogfood record](../tests/agent-eval/results/2026-09-30-assignments-dogfood/manifest.json).
Full builds, gates and evidence regeneration run on GitHub.
