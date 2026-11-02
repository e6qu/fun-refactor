# Prove an algorithm, then measure the guidance

Start with Euclid's algorithm for the greatest common divisor (GCD).
The GCD is the largest whole number that divides both inputs without a remainder.
For example, the GCD of 48 and 18 is 6.
The fixed implementation repeatedly replaces `(a, b)` with `(b, a % b)`;
`%` means the remainder. Its execution is `(48, 18) → (18, 12) → (12, 6) → (6, 0)`.
When the second input reaches zero, it returns the first input.

The agent must fill two proof regions in [Euclid.lean](fixtures/Euclid.lean):

- **Termination:** when `b` is nonzero, `a % b` is smaller than `b`.
  This proves that recursive calls cannot continue forever.
- **Correctness:** for every pair of nonnegative whole numbers, the fixed function
  returns `Nat.gcd a b`, Lean's standard GCD specification.

These are proofs of the actual executable Lean function. There is no separate
Rust or Python model and no claim about translating a program from either language.
Lean's kernel, standard library and execution tools are trusted components.
Finite execution examples provide an additional check; they do not prove the general theorem.

## What must pass before we call an agent

The [hosted qualification](../../../.github/workflows/proof-algorithms.yml) builds
the unfinished package, discloses both goals and checks supplied reference tactics
through `fr spec proof-state`, `proof-check` and `prove`.
It replays the delivered patch in a clean checkout, builds with warnings treated as
errors, audits the executable function and both proofs for assumed axioms, and
compares twelve executions with independently calculated answers.
The reference tactics live in `controls/`, outside the exported agent project.

Negative controls must reject unfinished termination, unfinished correctness,
wrong tactics, a weaker claim, a changed implementation, replacing the function
with library GCD, an extra axiom, changed package settings, and missing or invalid patches.
A proof-check receipt for one region cannot establish that the whole algorithm is proved.

**Current status:** task and evaluator implementation in progress. A small guarded
Lean check accepts the reference proof; hosted qualification and all live comparisons
are still pending. No improved agent result is claimed.

## The comparisons, in order

1. **Relevant facts:** GLM Flash and non-Pro Kimi, two repetitions with and without
   supplied library declarations: eight fresh sessions on Euclid.
   Both receive the same task, tools and shared skills. Only the declaration handout changes.
2. **Presentation:** Luna Fast, GLM Flash and non-Pro Kimi, two repetitions with a
   checklist and two with a graph: twelve fresh sessions on Euclid.
   Both presentations must contain exactly the same goals, facts, dependencies,
   case advice and delivery instructions. A diagram is not an automatic proof search engine.
3. **Transfer:** reserve accumulator-based list reversal until the development
   decisions above are fixed. For example, reversing `[1, 2, 3]` into `[9]` must
   produce `[3, 2, 1, 9]`. The theorem must cover all lists and accumulators.
4. **Backtracking:** test a controller that revisits an earlier draft only if the
   observed failures justify it. Keep this separate from the presentation comparison.

Freeze inputs, order, model settings, controls, checking tools and decision rules
before live sessions. Each complete attempt has 150 seconds, including preparation,
tool use, checks and delivery. Retain every failed or timed-out slot.
Two successes versus zero in two repetitions justify a transfer test, not a general
claim that one method is better. Mixed results remain inconclusive.

Raw local transcripts stay private. Publish reviewed summaries and submitted
patches only. Preserve all earlier results and keep M3 open until its original
requirements pass. The earlier [division-fact plan](../proof-division-facts/README.md)
is retained as an unrun supporting diagnostic; it is not the algorithm experiment.
