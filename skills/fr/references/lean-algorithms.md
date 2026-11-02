# Prove the code that actually runs

Read the task's fixed implementation and named claims before editing. A termination
claim proves that recursive calls eventually stop. A correctness claim describes
the returned answer for every admitted input. They are separate obligations.

For example, Euclid's GCD function replaces `(48, 18)` with `(18, 12)` until the
second input is zero. Its termination argument concerns the decreasing second
input. Its correctness argument concerns the answer, including the zero case.
Keep the implementation fixed while proving these claims.

Read the target's parameters and actual goals through the [interactive loop](lean.md).
For recursion, identify what becomes smaller and which inputs change at the next
call. An induction hypothesis is a fact about smaller calls. If both inputs change,
the hypothesis must still cover the new first input; fixing it too early can leave
an unusable hypothesis. Unfold one recursive step and inspect both cases before
choosing library facts. Read each fact's argument types rather than guessing them.
Here `b` can fall from 18 to 12. A proof that only knows case 17 may not help.
Strong induction lets you reuse the proof for every smaller input.

Simplification repeatedly rewrites expressions using known equalities. For a recursive
function, the same pattern can reappear. If Lean reports a looping simplification after
`simp [f]`, unfold one call instead and inspect the resulting cases. Do not increase
the recursion limit to force that loop through.

When one claim uses another unfinished claim, Lean may check that draft while
retaining an assumption elsewhere. A passing `proof-check` applies to its target.
Finish every required region and require the complete package check to pass with
warnings treated as errors. Preserve the supplied `.fr/checks.json`. Once it
passes, deliver the requested patch and finish; repeating the same check consumes
the remaining attempt time.

A checklist or graph can record unfinished work. Neither proves a tactic is valid.
After a branch change, retain the complete draft, source and available assumptions.
Identical displayed goals can have different hidden context; do not merge states
or restore a draft based only on goal text. Require a fresh Lean check after edits.

State precisely what was proved. A theorem about an executable Lean function
applies to that function. A theorem about a Lean model of a Rust or Python program
needs a separate source-to-model proof before it establishes the original program.
Matching a few executions does not establish that connection.
