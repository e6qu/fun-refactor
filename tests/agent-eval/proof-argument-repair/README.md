# Can a type-error repair instruction help GLM and Kimi finish page count?

The task is still the complete page-count proof. Eleven items at five per page
need three pages. Invalid inputs return `-1`; valid signed-64 inputs must satisfy
the fixed result bounds. The original Rust source, Lean model, claim and
assumptions stay unchanged. The proof starts empty.

A theorem can require a proof about a number as an argument. The number itself
does not meet that requirement. For example, `n` is an integer, whereas a named
assumption `hn : 0 ≤ n` proves that it is nonnegative. Lean's error context tells
the agent which type it supplied and which type was required.

This comparison tests one instruction: after that error, read the context,
match required proof arguments to available assumptions, and check the repaired
step before searching for another theorem. An assumption can imply a needed
inequality without having exactly the same wording; the instruction permits
deriving and naming that proof. Neither condition receives a finished proof,
extra theorem names or stronger assumptions.

## Fixed comparison

The [requirements](requirements.json) own eight fresh slots: GLM Flash and
non-Pro Kimi K3, two instruction settings and two repetitions each. Both use
OpenCode2 2.2.5, low reasoning, the same fr executable and shared skill.
Fr's direct proof checks and newly generated packages explicitly use one Lean
compiler worker. Both comparison settings use that same `-j1` setting.
Both explicitly use `BUN_OPTIONS=--smol`, `BUN_JSC_forceRAMSize=268435456`
and `BUN_JSC_useJIT=0`. The runtime sizes memory collection for 256 MiB and
disables just-in-time compilation; the hint does not impose a memory cap.
The [earlier client investigation](../opencode/changes/2026-10-08-memory/README.md)
rejected smaller-heap mode alone and found that the combined settings passed
its controls at a CPU cost. Those results concern an older client and workflow.
Fresh complete qualification must establish the unchanged reserve here too.
The order reverses between repetitions. All complete attempts retain the
150-second deadline and existing CPU, memory, disk and output limits.

The baseline is the same agent with the shared skill alone. The other setting
adds the [repair instruction](fixtures/argument-repair.txt). Both read the same
handout path; its content is the only different supplied input. The collector
rejects changes to that file. Costs include preparation, instructions, tool use,
package checking, patch delivery, accounting and failed attempts. This comparison
does **not** measure fr overhead against ordinary Lean tools.

Before any model call, GitHub must qualify the complete claim, accepting the
reference and rejecting unfinished, invalid and altered patches. Each local
client and instruction setting must then pass a fresh complete scripted session:
read the supplied instructions, expose the mapped type error, check a corrected
reference, save it, build the package, deliver a replayable patch and finish
with complete native accounting. Those scripts receive reference drafts solely
to test the setup. Live attempts receive none. Freeze the exact clients, tool,
task archive, admission receipts, checking code and all eight slots afterwards.

The reusable `proof-feedback.yml` workflow can be dispatched with explicit
`study=M3-argument-repair`, `operation=prepare` and the successful algorithm build
run. Independent grading uses `operation=grade` and the successful preparation
run. Local admission uses `agent_eval.algorithm_admission observe`; live slots
use `agent_eval.selected_guarded`. Both enforce the workstation guard and
independently sampled complete-workflow limits. Never bypass a refusal.

## Decide from observed actions

Record the first mapped argument-type error, the next actual action and the
next proof check separately. Keep private report hashes with concise reviewed
summaries. The mechanism prediction is met only when that next check repairs
the argument mismatch before another theorem search. A different remaining
goal can still make the complete proof fail. If no argument-type error occurs,
the mechanism is **unobserved**, including when a proof succeeds. First-check
timing is an observation, not a prediction about this post-error instruction.

Each agent must also finish at least one independently accepted complete proof
in its two instructed attempts. Incomplete accounting cannot count as success.
For each agent, two instructed successes versus zero baseline successes supports
a different transfer task before adoption. The reverse requires inspecting
possible harm. Ties and mixed results are inconclusive. Two repetitions cannot
establish a general success rate. A failed prediction keeps the broader
36-attempt study paused.

No old slot is reopened. M3 remains open. An accepted proof establishes only a mathematical
model; its connection to Rust execution remains unproved. Publish submitted patches
and reviewed summaries only. Raw prompts, transcripts, drafts, source archives,
session exports and account data stay private.

Current state: qualification, with no new model calls yet. The
[reviewed probe summary](qualification-probes.json) preserves both failed complete
controls and the isolated package checks. The first control exposed an input-format
bug in patch validation. The next completed checking, saving, package build,
patch replay and native accounting, but peaked at about 751 MiB against the
unchanged 640 MiB admission ceiling. A receipt incorrectly reported its completed
patch replay as false; that flag is fixed.

The single-worker package probe sampled about 87 MiB less memory. That isolated
sample did not predict enough reserve in the complete workflow. Fresh complete
qualification must establish the reserve with the declared client settings before
any calls. Raw records stay private; no passing probe replaces failed admission.
