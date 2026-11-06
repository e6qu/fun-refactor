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
invalid. `start` creates `skip` and shows the actual goal; it never overwrites.
To resume a draft after changing a step:

```sh
fr --json proof state '<TARGET>' --draft .fr/proof-tactics --at end --bytes 8192 --timeout-seconds 30
```

`end` selects the last nonempty draft line. `4:end` selects line four's end;
`LINE:COLUMN` uses one-based Unicode character positions. No column counting
is needed for an end position. `--bytes` bounds the UTF-8 JSON response.
`--timeout-seconds` shares one deadline across dependencies and checking.

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
For `P ∧ Q`, `constructor` creates two goals. Bullets `·` focus branches.
For `h : P ∧ Q`, `rcases h with ⟨hp, hq⟩` names two proofs.
An implication `h : P → Q` instead needs a proof of `P` to yield `Q`.
See [Arithmetic](lean-arithmetic.md) and [Algorithms](lean-algorithms.md).

Read the model's imports. From `specs/`, `lean --print-prefix` prints `<PREFIX>`
for its toolchain. For `import Init.Data.Int.DivMod.Lemmas`, search that file:

```sh
fr -C '<PREFIX>/src/lean/Init/Data/Int/DivMod/Lemmas.lean' project find '<NAME>' --contains --signature --limit 4
```

`-C` bounds input files; `--limit` bounds output rows.
A whole-library scan exceeded 768 MiB. Start with the imported file.

Text labels results. JSON uses `columns` and `rows`, or the SDK decoder.
If results were omitted, an empty page cannot prove no match.

## Check, save, deliver

```sh
fr --json proof check '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30
fr --json proof save '<TARGET>' --draft .fr/proof-tactics --bytes 8192 --timeout-seconds 30
```

A rejected check exits 1 with `passed: false` and diagnostics. Inspect
`draft_location`, `context`, warnings and omission counts; `tail` can hide errors.
`save` checks and writes in one call; a separate final check is unnecessary.
`written` supplies the saved edit's `transaction` ID as `<EDIT_ID>`.
`already-present` creates no new edit. Neither accepts other unfinished proofs.

Run the supplied package check once and export the selected saved edit:

```sh
lake -d specs build --wfail
fr history patch '<EDIT_ID>' --output change.patch
```

For two saved proofs, pass both IDs in save order. Review the complete patch.
Preserve `.fr/checks.json`. A passing draft, a saved proof, a passing package
and a delivered patch are separate stages. Combine known serial commands in one shell call; stop on failure.
For claims, read [Create a claim](lean-claims.md).
