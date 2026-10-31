# Construct a small multiplication proof and finish delivery

This is the next development check in [PLAN.md](../../../PLAN.md#next-work).
The runner now has an explicit four-attempt mode. Its hosted qualification must pass before calls. No agent attempts have run.

The [Rust function](fixtures/scaled.rs) multiplies two signed-64 integers.
The fixed Lean claim says that multiplying a nonnegative number by a value of at least one cannot reduce it.
For example, `3 * 2 = 6`, which is at least three; multiplying zero still gives zero.

Lean's `Int` means unbounded mathematical integers. Rust's signed-64 result can overflow.
For example, multiplying the largest signed-64 value by two exceeds that type's range.
This check proves the declared mathematical model. A separate proof must connect it to Rust and account for overflow.
The [requirements](requirements.json) fix that scope, the selected low-reasoning GLM and Kimi profiles, and all attempt limits.
[Checker settings](checkers.json) fix tool versions and permitted axioms.
An axiom is an assumption Lean accepts without proving it; only the listed standard ones are permitted.

## What the local control establishes

A guarded local control used the measured fr executable to generate this model and claim.
Tactics are the proof steps that Lean checks.
Fr rejected an unfinished proof, accepted the separate [reference tactics](controls/reference.lean), applied them and passed the package check.
The supplied `.fr/checks.json` stayed unchanged. The [control summary](control-summary.json) records the identities and limits of this observation.
This is a solvability check without model calls. It supplies no agent result or isolated grading qualification.

The solved control project must never become an agent seed.
Prepare fresh projects whose theorem still needs a proof and keep the reference outside them.
Both agents receive the shared skill and corrected execution instructions.
That skill includes a related multiplication example, so this remains a familiar development check.

## The four planned attempts

Run two fresh attempts each for GLM Flash and Kimi K3, all with fr.
The original 150 seconds include preparation, responses, commands, checks, delivery and session accounting.
Keep check settings unchanged. Record the first passing proof, package check, submitted patch and completed session separately.
Count success only when observed tool results establish fr checking, an actual fr write and the passing supplied package check.
Unknown evidence for a required check cannot count as a completed attempt.
Every submission needs independent checking in a fresh project, with exact patch undo and redo.
A valid proof from a timed-out attempt remains failed.

Before calls, pass the hosted tests of installed OpenCode2 selections with scripted replies.
Check unchanged source and check settings, recorded timing and independent patch rejection.
Accept the external reference and reject unfinished or wrong proofs, altered source or claims, and missing or malformed patches.
Retain the measured eight-slot division study without rerunning or rewriting it.

If all four attempts pass, inspect the first unresolved step in constructing the page-count proof.
If any fail, keep every outcome and inspect the first unresolved failure before another change or paid batch.
This single-condition check has no timing baseline. It cannot establish that the corrected instructions caused an improvement or saved time.
Publish only submitted patches and summaries. Account charges and unavailable usage stay unknown.

## Prepare, run and check

Dispatch the existing refinement workflow with group `proof-construction-prepare`, an explicit prefix and a passing M3 native build ID in `collection_run`.
This checks four scripted client exchanges and seven independent patch controls without model calls.
Its `diagnostic-qualified-inputs` artifact contains the unfinished source archive and control receipt.

Use the [local runner](../../../tools/agent_eval/selected_local.py) with `--milestone M3-construction`.
The explicit binaries JSON names only `fr` and `opencode2`; its companion sits beside the OpenCode2 launcher.
The runtime JSON names `python`, `rust_root`, `lean_root`, `fr_build` and `controls` through absolute paths.
Freezing copies executables, inputs and settings into a plan and records their hashes.
Freeze only committed code and the successful hosted inputs, under the workstation guard. Run each of the four frozen slots once.

Review the four closed records and export with `selected_public export-construction`.
Keep unknown stage times null. Upload only the submitted patches and reviewed summary.
Then dispatch group `proof-construction-grade` with the successful preparation run ID in `collection_run`.
Attach those exact independent verdicts with `selected_public accept-checks` before applying the decision rule.
The final report command requires the explicit `--study M3-construction` selection.
