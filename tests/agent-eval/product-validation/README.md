# Unfamiliar tasks and complete costs

This is roadmap PR 5. The user authorized at most 96 new live attempts: 48 initial
attempts and 48 validation attempts on a separately reserved set. No attempt has run yet.

Each round compares three selected agents on four tasks with ordinary tools and
ordinary tools plus fr, twice each. The tasks cover refactoring, code/call-graph
investigation, deliberate behavior changes and a proof of unchanged executable Lean code.
The project tasks use the upstream revisions in [reservation.json](reservation.json).
The proof tasks are newly authored examples; they are not unread upstream projects.

## Keep the validation set separate

The reservation was committed before opening any of these projects in this milestone.
This records the order of our work, not a claim about a model's training data.
Do not open validation project source until the product implementation and the decision
from the initial round are frozen. Task authoring after that freeze cannot change fr.
Retain source archives, licenses and exact content hashes when preparing each task.

Freeze each round's requests, source, independent checkers, wrong-answer controls,
native client checks, implementation, client versions, model settings, order and limits
before its first model call. Preparation does not launch a model. A changed input
requires a new checked freeze before calls, never a silent replacement of prior results.

## Compare complete attempts

Use the roadmap's Luna Fast, GLM Flash and non-Pro Kimi K3 profiles. Both tool setups
receive the same task, settings, limits and checking commands. Rotate setup order by
profile, task and repetition. Run serially in fresh application state.

Keep the complete attempt at 150 seconds, including preparation, checking, accounting
and delivery, with a ten-second finishing reserve. Preserve the existing 768 MiB
attempt memory, 75 sampled CPU seconds, 512 MiB workspace growth and 1 MiB transcript
limits. The workstation's outer limits remain stricter where applicable, including
its 180-second ceiling, 1 GiB aggregate RSS, 2 GiB target ceiling and 64 GiB free disk.
Builds, full gates, task preparation and independent grading run on GitHub.
Credentials, prompts and raw client transcripts remain local.

Retain all 96 scheduled slots, including unstarted work, failures, timeouts and
resource refusals. Never retry a failed slot. Report independently accepted outputs
separately from attempts completed within limits. Missing usage or charges stay unknown.
Report setup costs, matched successful times and total spending per success separately.
A successful ordinary edit without a contributing fr step does not establish fr use.

## Improve only what the evidence identifies

The current [session comparison](../../../docs/project-session-cost.md) is the
development baseline. All 135 answers agree; Python and saved-answer sessions still
take longer than the direct CLI in those familiar examples.
Profile before changing code, retain the original measurements, and reject a faster
implementation that changes complete answers, invalidation, corruption detection or delivery.

The initial live round identifies workflow obstacles. Freeze one proposed correction
and its prediction before changing the product. Use at most two small deterministic
diagnostic cycles for the same obstacle. Then freeze the implementation before
opening the reserved validation sources and run the validation round once.

A benefit claim needs preserved correctness and a corresponding measured improvement
on the reserved tasks. Separate results by agent and task family; two repetitions
do not establish a general success rate. Publish negative or inconclusive results
and an explicit keep, revise or discard decision. A larger wall-time limit, dropped
failure or missing cost cannot count as an improvement.

The exact task checkers and decision thresholds must be committed and checked before
calls. Neither this preparation nor a scripted rehearsal closes the live comparison.
The original M3 gate, the stopped CodeGraph study and list reversal remain unchanged.
