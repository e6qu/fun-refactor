# Finish and deliver two already-correct proofs

An agent can find a correct proof and still fail the task: it might save only one
of two proofs, export just its last edit, put the patch in the wrong directory,
or keep working until its deadline expires. The
[Euclid review](../proof-algorithms/local-results/review-context.md) includes a
correct diff written outside the project. It remains a failed attempt.

This diagnostic asks a narrower question: **when both correct proofs are supplied,
can the agent save and deliver them within the complete attempt limit?** It does
not test the ability to discover a proof. It cannot close the missing page-count
proofs in [M3](../../../PLAN.md#m3-help-the-agents-write-repair-and-maintain-lean-proofs).

Euclid's algorithm computes the greatest common divisor: for example, `gcd(48, 18)`
returns `6`. The existing task asks Lean to check two claims: every recursive call
gets closer to stopping, and the answer equals the standard GCD for every admitted
input. A tactic is a proof step that Lean checks. Both correct tactic files are
copied into this diagnostic's project; that assistance must remain explicit.
The program, claims, imports and checking settings stay fixed.

## What the agent must do

The project contains `.fr/euclid_decreases.tactics` and
`.fr/euclid_correct.tactics`. The unfinished claims are in
`specs/FrSpecs/Euclid.lean`.

1. Check each supplied draft with `fr spec proof-check`, then save it with
   `fr spec prove --write`. Require the JSON result's `passed: true` for a check.
   A save with `status: written` returns the ID of that saved edit. A save with
   `status: already-present` means that draft is already in the source.
2. Run the package command supplied in `.fr/checks.json`. A target proof check
   covers one claim; the package check must cover both saved proofs together.
3. Export both saved edit IDs in their save order. For example, if the two saves
   returned IDs 4 and 7, use
   `fr history patch 4 7 --output change.patch`.
   A saved edit is a recorded source change. Exporting only edit 7 can omit the
   earlier proof even when the working file contains both.
4. Finish with a nonempty `change.patch` in the project root. The native session
   and its accounting must also finish within the attempt limit.

The evaluator takes only that requested file. It never searches another directory
for a replacement patch. It applies the patch to a clean original project and
independently checks both proofs, the fixed program, forbidden assumptions and
execution examples using the [existing private GCD checker](../proof-algorithms/README.md).
Checking the agent's working file alone cannot establish correct delivery.

## The fixed comparison

The [requirements](requirements.json) declare twelve unstarted slots: Luna Fast,
GLM Flash and Kimi K3, two repetitions with each of two setups. The exact profiles
come from the existing [model settings](../proof-writing/profiles.json).

| Setup | What the agent receives |
| --- | --- |
| Shared skill | The task, both correct drafts, the current fr skill and package check |
| Finish card | Those same inputs, plus a short instruction card: check and save, check the package, deliver and stop |

The finish-card request explicitly tells the agent to read its card. Both setups
have the same tools and correct answers. Order alternates by profile and repetition.
Each complete attempt keeps **150 seconds**, 768 MiB sampled memory and 75 CPU
seconds. Setup, instructions, checks, delivery and native accounting are included.
The workstation guard and external observer are still required.

For each profile, two successes with the finish card and none with the shared
skill warrant a different delivery task before adoption. The reverse result
warrants inspecting harm. Other results are inconclusive about a success advantage.
Keep every timeout and failed slot; never rerun a slot to replace its result.
Compare total work including failures and instruction size, not only successful
attempts. The baseline is **the same agent with the shared skill alone**. This is
not a comparison with ordinary Lean, and does not measure fr's overhead against it.

## What the controls establish

The [hosted algorithm workflow](../../../.github/workflows/proof-algorithms.yml)
checks the supplied drafts, saves both through fr, runs the package check and
exports both IDs under each setup. It grades both patches in separate clean
containers. It also rejects:

- Only the last saved edit, only termination, or only correctness.
- An invalid proof that fails the package, a changed executable, or invalid patch text.
- A patch outside the project root or changed checking settings.

Focused tests also reject empty, oversized and symlinked patches, changed supplied
drafts or skills, missing resource measurements, and non-finite or Boolean times.
Synthetic observations check late patches, late accounting and stopped sessions.
These synthetic times test the decision rule; **they are not measured agent times**.

The workflow retains `proof-delivery-controls/summary.json` and `protocol.json`.
The protocol records the source fingerprints, checker identity, exact profiles,
requests, order and decision rule. A fingerprint identifies contents; it does not
prove correctness. The summary records scripted controls with zero model calls.
Neither file reports a new agent success.

The first [hosted qualification](https://github.com/e6qu/fun-refactor/actions/runs/41612735437)
passed both supplied-proof setups and every rejection control. Both setups
delivered the identical independently accepted patch. The retained
[summary](qualification/summary.json), [protocol](qualification/protocol.json) and
[GitHub source](qualification/source.json) preserve those observations. All twelve
live slots remain unstarted. Recheck the recorded input fingerprints before using
this qualification to admit a local attempt.

**Live calls remain disabled.** These hosted checks do not admit a local native
agent run. Before any paid attempt, qualify the exact local CLI, fr binary, Lean,
assisted task archive, external observer and native accounting together within
the existing limits. Freeze those identities and the control run before the
first call. The existing model runner does not yet recognize this diagnostic;
do not relabel an old algorithm slot or reuse a closed slot to run it.
Public uploads remain submitted patches and reviewed summaries only. Raw local
records and account data stay private. The twenty original algorithm outcomes
and every M3 outcome remain unchanged.
