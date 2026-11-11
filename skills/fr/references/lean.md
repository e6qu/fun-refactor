# Write and deliver a checked proof

Keep the program, definitions, claim, assumptions and imports fixed.
Tactics are proof steps. Proving this model does not prove its source connection.

## Start with an actual goal

```sh
fr proof list specs --limit 8 --bytes 4096
fr proof show '<TARGET>' --bytes 8192
fr --json proof start '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30
```

Copy `<TARGET>` from the list: `FILE.lean::THEOREM`. A theorem name alone is
invalid. Each listed goal's `start` field is the next command to run. `start` creates `skip` and shows the actual goal; it never overwrites.
To resume a draft after changing a step:

```sh
fr --json proof state '<TARGET>' --draft .fr/proof-tactics --at end --bytes 8192 --timeout-seconds 30
```

`end` selects the last nonempty draft line. `4:end` selects line four's end;
`LINE:COLUMN` uses one-based Unicode character positions. No column counting
is needed for an end position. `--bytes` (1024..16384) bounds the JSON response.
`--timeout-seconds` (1..60) shares one deadline across dependencies and checking.

Each `goals` item has two text fields:

```json
{"context":"n : Int\nhn : 0 ≤ n", "goal":"0 ≤ n + 1"}
```

Here `n` is a number, `hn` an available proof, and `0 ≤ n + 1` the objective.
Read `item["context"]` and `item["goal"]`; the items are not strings.
Read every goal after a split. `closed-at-position` closes only this branch;
`no-tactic-at-position` means the cursor selected no tactic.
All inspection states retain `acceptance: "not-checked"`.

Write tactics without leading `by`, preserving nested indentation.
Parameters in `context` already exist. Use `intro` for an implication or
quantifier still in the goal, not for a variable already in scope.
For `P ∧ Q`, `constructor` creates two goals; for `P ∧ Q ∧ R`, use
`refine ⟨?_, ?_, ?_⟩`. Bullets `·` focus branches.
`omega`, `simp` and `decide` work in every model file, even after `prelude`.
For `h : P ∧ Q`, `rcases h with ⟨hp, hq⟩` names two proofs.
An implication `h : P → Q` instead needs a proof of `P` to yield `Q`.
See [Arithmetic](lean-arithmetic.md) and [Algorithms](lean-algorithms.md).

For a missing fact, read `library_facts` in the `fr proof start` result. It
lists library theorems about the model's operations with exact names and
statements. For any other step, try `exact?` (see Arithmetic). Searching Lean's
own source is slower; check a draft instead.

## Check, save, deliver

```sh
fr --json proof check '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30
fr --json proof save '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30 --deliver change.patch
```

A rejected check exits 1 with `passed: false` and diagnostics. Inspect
`draft_location`, `context`, warnings and omission counts; `tail` can hide errors.
A rejected `save` returns the same report and writes nothing.
`save` checks and writes; `--deliver` then runs `.fr/checks.json` and writes all
saved proofs as one patch. `delivery.delivered: true` means done. A failing check
exits 1 and writes no patch; fix it and save again. `already-present` creates no edit.
Without checks, omit `--deliver`, run the package check, then
`fr history patch '<EDIT_ID>' --output change.patch` with the `transaction` ID.
Preserve `.fr/checks.json`.
For claims, read [Create a claim](lean-claims.md).
