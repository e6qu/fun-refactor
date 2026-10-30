# Repair arithmetic proof steps

Try a small draft and read its returned goal before searching for more facts.
Keep the program, theorem and assumptions fixed.
`example` states a claim; `by` starts its proof. `import Init` loads Lean's standard library.
`Nat` means nonnegative whole numbers; `Int` also includes negative numbers.

## Boolean comparisons

`Bool` has values `true` and `false`. `Prop` holds statements such as `a ≤ b`;
a proof of that statement lets arithmetic tactics use it.
`decide (a ≤ b)` computes a Boolean. Convert an assumption about that Boolean
into inequalities before asking `omega` to solve arithmetic:

```lean
import Init
example (a b : Nat)
    (h : (decide (a ≤ b) && decide (b ≤ a)) = true) : a = b := by
  simp only [Bool.and_eq_true, decide_eq_true_eq] at h
  rcases h with ⟨hab, hba⟩
  omega
```

`simp only` uses the listed simplification rules. Here they turn `h` into
`a ≤ b ∧ b ≤ a`. `rcases` names its two facts; `omega` proves their arithmetic consequence.

## Assumptions joined by “and” or “or”

`p ∧ q` supplies both facts. `p ∨ q` supplies one of two alternatives.
Angle brackets unpack both facts; a vertical bar creates separate cases:

```lean
import Init
example (p q : Prop) (h : p ∧ q) : q ∧ p := by
  rcases h with ⟨hp, hq⟩
  exact ⟨hq, hp⟩

example (p q : Prop) (h : p ∨ q) : q ∨ p := by
  rcases h with hp | hq
  · exact Or.inr hp
  · exact Or.inl hq
```

`exact` supplies a proof. `Or.inl` chooses the left alternative; `Or.inr` chooses the right.

## Library arguments can be proofs

Read a fact's declared type before applying it. `Int.tdiv` rounds integer division toward zero. These
arguments are proofs that the inputs are nonnegative:

```lean
import Init
example (a b : Int) (ha : 0 ≤ a) (hb : 0 ≤ b) : 0 ≤ Int.tdiv a b := by
  exact Int.tdiv_nonneg ha hb
```

`ha` and `hb` prove inequalities; `a` and `b` are integers. Their types determine
the argument order. `omega` handles addition, subtraction, order and multiplication by fixed numbers.
Products of two changing values can need extra facts. Check each step before another search.

## Ask Lean for a missing fact

`exact?` searches available theorems for the current goal. `Std` includes this tactic:

```lean
import Std
example (a b : Int) (ha : 0 ≤ a) (hb : 1 ≤ b) : a ≤ b * a := by
  have h : 1 * a ≤ b * a := by
    exact?
  simpa using h
```

Lean suggests `exact Int.mul_le_mul_of_nonneg_right hb ha` for `h`.
`simpa using h` reduces `1 * a` to `a`. Replace the search with the suggested step.
Search a specific missing fact; `exact?` does not solve every goal.
