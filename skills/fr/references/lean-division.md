# Prove facts about division and remainder

Keep the program, theorem and assumptions fixed. `Int.tdiv a b` divides and rounds
toward zero; `Int.tmod a b` is its remainder. For `/` and `%`, use the matching
`Int.ediv` and `Int.emod` facts the same way.

## Give omega the facts it lacks

`omega` treats `Int.tdiv a b`, `Int.tmod a b` and `b * Int.tdiv a b` as unknown
numbers. It proves nothing about them until `have` lines state their relations.
Take those facts from `library_facts` in the `fr proof start` result:

- the identity `Int.tmod_add_mul_tdiv a b : tmod a b + b * a.tdiv b = a`;
- the remainder bounds `Int.tmod_nonneg` and `Int.tmod_lt_of_pos`;
- the quotient bounds `Int.tdiv_nonneg` and `Int.tdiv_le_self`.

A product of two changing values, such as `b * q`, needs one more fact. Write
it as `1 * q ≤ b * q`; `exact?` finds that form, not `q ≤ b * q`.

```lean
import Init
example (a b : Int) (ha : 0 ≤ a) (hb : 1 ≤ b) (hr : Int.tmod a b ≠ 0) :
    Int.tdiv a b + 1 ≤ a := by
  have hsum := Int.tmod_add_mul_tdiv a b
  have hr0 := Int.tmod_nonneg b ha
  have hq0 : 0 ≤ Int.tdiv a b := Int.tdiv_nonneg ha (by omega)
  have hprod : 1 * Int.tdiv a b ≤ b * Int.tdiv a b :=
    Int.mul_le_mul_of_nonneg_right hb hq0
  omega
```

`hsum` gives `a = tmod a b + b * q`, and `hr` with `hr0` makes the remainder at
least 1. `hprod` bounds the product, so `omega` finishes.

## Conditions in the model

`simp [MODEL, h]` unfolds the model and uses `h` to decide its `if`. That works
for conditions written with `||`, `&&` or `==` as well:

```lean
import Init
example (a b : Int) (h : a < 0 ∨ b ≤ 0) :
    (if (a < 0 || b ≤ 0) then (-1 : Int) else 0) = -1 := by
  simp [h]
```

Split the remaining `if` with `by_cases hz : Int.tmod a b = 0` and simplify
each branch. Check the draft after each branch instead of reading library source.
