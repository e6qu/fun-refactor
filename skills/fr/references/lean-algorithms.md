# Prove the code that actually runs

Read the task's fixed implementation and named claims before editing. A termination
claim proves that recursive calls eventually stop. A correctness claim describes
the returned answer for every admitted input. They are separate obligations.

Euclid replaces `(48, 18)` with `(18, 12)` until the second input is zero.
Prove that input decreases and that the answer stays the same, including the zero case.

Read the target's parameters and actual goals through the [interactive loop](lean.md).
For recursion, identify what becomes smaller and which inputs change at the next
call. An induction hypothesis is a fact about smaller calls. If both inputs change,
the hypothesis must still cover the new first input; fixing it too early can leave
an unusable hypothesis. Unfold one recursive step and inspect both cases before
choosing library facts. Read each fact's argument types rather than guessing them.
Here `b` can fall from 18 to 12. A proof that only knows case 17 may not help.
Strong induction lets you reuse the proof for every smaller input.

Simplification repeatedly rewrites using equalities. `simp [euclid]` can loop by
unfolding more recursive calls. Use `rw [euclid]` to unfold one call and inspect
the cases. Keep the recursion limit.

Save one checked draft before changing another proof. Lean checks the whole file:
an invalid neighboring proof can reject a valid draft. A null `draft_location`
and a `checked_location` elsewhere mean inspect that source. Keep drafts separate;
do not erase failed work. A target check can pass while another claim is unfinished.
Finish both claims and the package check before delivering the patch.
For recorded edits, export their returned IDs together:
`fr history patch <FIRST_TX> <SECOND_TX> --output change.patch`.
This uses saved snapshots; include every requested edit and review the result.

Tactics depend on the imports. With only the GCD library, `by_cases` may be
unavailable. `cases b` separates zero from successor; unfolding an `if` and using
`split` separates its branches. Read the actual goals. Do not change fixed imports.

A checklist or graph can record unfinished work. Neither proves a tactic is valid.
After a branch change, retain the complete draft, source and available assumptions.
Identical displayed goals can have different hidden context; do not merge states
or restore a draft based only on goal text. Require a fresh Lean check after edits.

A proof about executable Lean applies to that function. A model of Rust or Python
needs a separate proof connecting it to the source. Matching examples cannot
establish that connection.
