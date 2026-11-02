# Write a proof with feedback after each step

Keep the theorem (claim), model, assumptions and program fixed. Tactics are proof
steps. Copy `spec goals specs`’s Lean `target` as `<LEAN_TARGET>`:

```sh
fr --json spec proof-task '<LEAN_TARGET>' --token-limit 4096
```

Read `claim.parameters` and `claim.parts`: assumptions and conclusions. Parameters may already be in scope: if the goal shows `x : Bool`,
use `x`; do not `intro x` again. `intro` is for an implication or quantified input
still to the right of `⊢`.

## The interactive loop

Write `skip` in `.fr/proof-tactics` without `by`. It leaves the goal open.
Keep indentation for nested steps.

```sh
fr --json spec proof-state '<LEAN_TARGET>' --from .fr/proof-tactics --at 1:5 --token-limit 4096 --timeout-seconds 30
```

`--at` is a one-based line and character column in your draft. Column 5 is just
after `skip`. Count Unicode characters, not bytes. Query the end of the step
you just wrote; before it you may see the previous state. Each query reads the draft. `goal_id` names the obligation; `proof_digest` identifies draft content.
Keep the source, imports and full draft together. `theorem` retains the original objective.

In each `goals` entry, `h : P` means a named proof of statement `P` is available;
`⊢ Q` means you must prove `Q`. Read all entries after a split. For `P ∧ Q`,
`constructor` creates two goals. A bullet `·` focuses a branch. Inspect the next branch after solving the first.

- `goals`: Lean's goals and available assumptions at that cursor, without clipping.
- `closed-at-position`: this branch has no goals there; another branch may be open.
- `no-tactic-at-position`: move the cursor into a tactic or run a complete check.

All have `acceptance: "not-checked"`. Insufficient response space fails explicitly. `--token-limit` measures UTF-8 response bytes, not model
tokens; choose it and the shared dependency/editor deadline explicitly.

Inspect each edited step. Read [Algorithms](lean-algorithms.md) for recursion or
[Arithmetic](lean-arithmetic.md) for comparisons and division. For a missing fact,
run `lean --print-prefix` from `specs/` to select its toolchain. Under that prefix,
choose a folder in `src/lean/Init/Data` as `<LEAN_LIBRARY_DIR>` and a name fragment
as `<LIBRARY_QUERY>`. Return to the project root for proof commands.

```sh
fr -C '<LEAN_LIBRARY_DIR>' project find '<LIBRARY_QUERY>' --contains --signature --limit 4
```

Read argument types and order; an empty search with gaps cannot establish absence.

## Check the complete draft before saving

```sh
fr --json spec proof-check '<LEAN_TARGET>' --from .fr/proof-tactics --token-limit 4096
```

`--timeout-seconds` belongs to `proof-state`, not `proof-check` or `prove`.
`prove` has no `--token-limit`. Read the complete response; `tail` can hide errors.
Inspect `passed`, not process exit. `draft_location` points into your tactics;
`checked_location` can include generated lines. Diagnostic `context` retains `⊢`;
`context_omitted_lines` counts omissions. Fix errors, warnings and all open goals.
Only `passed: true` supplies a checked receipt. Then save:

```sh
fr --json spec prove '<LEAN_TARGET>' --from .fr/proof-tactics --write
```

`status: written` saved a checked proof. Keep its `transaction` (saved edit ID).
`already-present` needs no repeated save. Preserve `.fr/checks.json`.
Run the package check, review the diff, then from the project root write
`git diff --binary HEAD > change.patch` and finish. Keep failed diagnostics.

For a missing model or theorem, read [Create a claim](lean-claims.md).
