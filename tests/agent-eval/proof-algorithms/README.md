# Prove an algorithm, then measure the guidance

Start with Euclid's algorithm for the greatest common divisor (GCD).
The GCD is the largest whole number that divides both inputs without a remainder.
For example, the GCD of 48 and 18 is 6.
The standard convention gives GCD zero when both inputs are zero.
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

**Current status:** the [checked build](https://github.com/e6qu/fun-refactor/actions/runs/41383408769)
passed twelve patch controls and six scripted native-client exchanges. Its exact Mac
build also passed a focused local proof-state and termination check.
The [first live batch](interrupted-results/README.md) stopped in its fifth slot at
the workstation memory limit. It retained three timeouts and one Kimi submission.
The investigation found missing detached-server accounting, lost buffered feedback
and a reader rejection of induction over two inputs. Their repairs need fresh
hosted qualification before the complete comparisons run. No guidance advantage is claimed.

The [memory investigation](memory-controls.json) reproduced the correctness-goal
limit without a model. Fr now removes the redundant Lake parent and Lean watchdog
from editor inspection, but the complete OpenCode/fr/Lean path with `import Std`
still exceeds the unchanged inner limit. A low reading from the inner monitor was
misleading: CPU throttling paused that monitor while a new worker ran.

Fresh attempts use an observer outside the throttled workload. All actual work
still runs inside the required workstation guard. The observer counts the guard,
controller, CLI and workers through preparation and accounting; it uses the same
150-second, 768-MiB and 75-CPU-second limits. This stricter scope differs from the
older native-client-only readings. The observer is low priority and cannot run two
guarded workloads at once.

The fresh task imports `Init`, which provides the required natural-number facts,
instead of the unused full `Std` library. The executable function and theorem
statements are unchanged. A complete scripted local check fits the memory limit;
hosted reference and rejection controls must qualify this exact task before calls.
The [original task](interrupted-results/Euclid.lean), all failures and misleading
readings remain recorded separately. No diagnostic is an agent outcome or a
measurement of fr overhead against ordinary tools.
The [generated reader](https://github.com/e6qu/fun-refactor/actions/runs/41390732896)
now preserves both induction inputs and the named induction rule.

The fixed [reserved list-reversal implementation](reserved/Reverse.lean) and its
reference proof are also recorded before development attempts. A small guarded Lean
check accepts the reference. The Euclid task archive contains neither the reserved
task nor either reference answer. Transfer-specific agent guidance and grading still
need qualification before any transfer call.

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
Before live calls, check the actual Mac CLI, private server, fr and Lean together
on the correctness goal. A standalone inspector check cannot establish that the
combined workload fits its limit. Scripted loopback replies can test this without
contacting a model provider.
Run each frozen algorithm slot through the independent observer:

```sh
python3 -B -m agent_eval.selected_guarded --plan /absolute/path/plan.json --id ATTEMPT_ID
```

This coordinator launches `fr-local-guard.py` itself. Keep the coordinator outside
the guard so its sampling continues while the guard pauses the workload.
It preserves the controller's original report and adds a separate whole-attempt receipt.
The public summary requires that receipt; a successful inner command cannot hide a
stopped or late complete attempt. The 768-MiB budget is a memory limit, and the
75-CPU-second budget counts sampled processor work rather than time spent waiting.
Two successes versus zero in two repetitions justify a transfer test, not a general
claim that one method is better. Mixed results remain inconclusive.

Raw local transcripts stay private. Publish reviewed summaries and submitted
patches only. Preserve all earlier results and keep M3 open until its original
requirements pass. The earlier [division-fact plan](../proof-division-facts/README.md)
is retained as an unrun supporting diagnostic; it is not the algorithm experiment.
