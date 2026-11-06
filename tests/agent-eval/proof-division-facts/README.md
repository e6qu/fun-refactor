# Can the agents select and apply current division facts?

No agent attempts have started and no execution plan is frozen.
The [argument-repair comparison](../proof-argument-repair/README.md) observed no mapped argument-type error
and delivered no patch. Repeating that instruction would not test its proposed repair.
This diagnostic instead asks whether the agents can select and apply suitable division declarations.
It uses the commands merged in [#603](https://github.com/e6qu/fun-refactor/pull/603).
Fresh hosted controls and complete local client checks must pass before calls.
This is a supporting diagnostic; it cannot replace an algorithm proof or close M3.
The [earlier page-count attempts](../proof-page-count/README.md) keep their original failures.

## The question

For eleven items divided into groups of five, the quotient is two and the remainder is one.
Their sum, three, is at most eleven.
The fixed claim asks for that upper bound for every nonnegative integer `a` and positive integer `b`.
`Int.tdiv a b` is integer division rounded toward zero; `Int.tmod a b` is its remainder.

The supplied Rust function computes their sum.
Fr prepares a Lean model, a mathematical description of that computation, and one fixed theorem about the model.
The theorem assumes `0 ≤ a` and `1 ≤ b` and asks for `division_partsModel a b ≤ a`.
Lean integers do not overflow. This experiment proves no source-to-model equivalence or Rust overflow claim.

A manual control now proves the complete page-count claim with explicit division facts.
That establishes a sufficient manual repair, not an agent success or a completion time.
The recorded agents also tried division identities, but mixed names, division forms and argument types.
Some commands cut off feedback; one search returned useful conversion declarations without a checked draft.
We therefore cannot assume that every fact was unavailable or that supplying names alone solves the task.

The smaller question is whether an agent can apply the right current declarations once supplied,
and whether it can select them from the same installed library without that handout.
This case leaves out the page counter's invalid-input branch, Boolean conditions and rounding.
It cannot close M3.

## Change one input

Both conditions have the same task, unfinished theorem, fr, skill, instructions, library and checks.
Editing or deleting a supplied skill file fails the attempt, as does changing the handout or check settings.
Both receive a protected `.fr/declarations.txt` file:

| Condition | Handout |
| --- | --- |
| `select-facts` | Ask the agent to select current declarations from the installed library |
| `supplied-declarations` | Four declaration types from the pinned Lean installation, listed alphabetically |

The supplied names are `Int.mul_le_mul_of_nonneg_right`, `Int.tdiv_nonneg`,
`Int.tmod_add_mul_tdiv` and `Int.tmod_nonneg`.
Their types describe their inputs and conclusions. The handout supplies no tactics, intermediate proof or completed proof.
The control proof stays outside both agent projects.

Both conditions also receive the [arithmetic skill](../../../skills/fr/references/lean-arithmetic.md).
It already demonstrates `Int.tdiv_nonneg` and `Int.mul_le_mul_of_nonneg_right` in separate examples.
The comparison tests the extra four-type handout on top of that shared help.
The selection condition must still find a suitable remainder identity and assemble the complete argument.

The [requirements](requirements.json) fix two repetitions per condition for each pinned GLM Flash and Kimi K3 low-reasoning profile.
That gives eight fresh local OpenCode2 sessions, with condition order balanced between repetitions and profiles.
Use OpenCode2 2.2.5, Lean 4.28.0 and the copied Python 3.14.10 installation.
Both projects use the same generated explicit division import and one Lean compiler worker.
Freeze fr, client executables, inputs and checker identities before calls.
No other model, retry, old study plan or extra attempt can replace a scheduled failure.

Each complete attempt keeps 150 seconds, including preparation, handout installation, replies, tools, checks, delivery and session accounting.
The existing memory, CPU, disk and transcript limits stay fixed. Local workloads run serially under the workstation guard.
Source generation and shared checker qualification happen before attempts; retain their records separately.
The comparison cannot treat this prepared handout as free work or establish fr overhead against ordinary tools.

## Qualify before spending model calls

1. Pass the existing complete CI gate and the new diagnostic's focused checks.
2. Generate the fixed project on GitHub using the checked algorithm-tool build.
   The `proof-algorithms.yml` workflow now calls the reusable `proof-feedback.yml` workflow
   with study `M3-facts` and operation `prepare`. It makes no model calls.
   Accept the isolated reference and reject unfinished tactics, altered source or claims, missing patches and wrong patches.
3. Compare the handout against Lean's actual `#check` output for all four declarations.
   Reject changing or deleting either condition's handout, as well as changed checker settings.
4. Require four fresh complete local client checks: each agent with each handout.
   Scripted replies must start a draft, inspect actual context and goals, reject unfinished tactics,
   check and save the supplied control proof, build the package, replay the delivered patch
   and finish session accounting. These control drafts never enter live projects.
   Keep the original 60-second control limit, 768 MiB ceiling and 128 MiB memory reserve.
   Both conditions must keep fr access, shared instructions and the same deadline.
5. Freeze all eight slots and input identities only after these checks pass. Start every slot once.

The shared evaluator keeps the theorem, assumptions, source, proof markers and settings fixed.
The reference proof and a false stronger mathematical claim have separate manual controls; neither is supplied to an agent.
After calls, the same reusable workflow with operation `grade` checks each original patch independently on GitHub,
using the same qualified tool and fresh task, with exact patch undo and redo.

## Judge complete delivery

A success needs an independently accepted proof patch, a useful fr proof check, an accepted fr proof write,
a passing supplied package check and completed delivery within 150 seconds.
A passing proof from a timed-out session remains failed.
Changing the handout or check settings also remains failed.

Record the first proof check, first passing proof, accepted write, package check, patch delivery and final session finish.
Both prompts name the exact theorem target and the explicit `fr proof start` command.
The independent resource observer stays outside the CPU throttle and includes the controller,
native client and detached workers. Missing or altered observer records reject export.
Missing times, unfinished usage and actual account charges stay unknown.
One CLI session can make several provider requests; do not equate the session count with a request count.

Decide separately for each agent using these rules, fixed before calls:

| Supplied declarations accepted / 2 | Select facts accepted / 2 | Next action |
| ---: | ---: | --- |
| 2 | 0 | Test better selection of current facts for the modeled operation |
| 2 | 2 | The small case does not reproduce the complete-task obstacle; inspect the remaining branches and delivery |
| 0 | 0 | Inspect applying facts and delivery; the handout alone did not solve the problem |
| 0 | 2 | Inspect whether the handout obstructs the agent's argument |
| Any mixed result | Any mixed result | Keep the result inconclusive and inspect actual actions before choosing another question |

A service, collection or unexpected resource failure blocks an ability conclusion.
Keep the failure, fix its cause and distinguish it from an arithmetic mistake.
Two repetitions guide the next diagnostic; they establish no general success rate or definitive cause.
Keep the complete comparison paused until the recorded decision supports a concrete change and its checks pass.
If the local guard refuses a workload, stop local checking and move affected work to GitHub;
do not raise limits or bypass the guard. A local admission refusal cannot be replaced by a hosted spot check.

Publish only submitted patches and reviewed summaries of local attempts.
Prompts, transcripts, drafts, snapshots and session exports stay local.
Keep all 335 earlier M3 outcomes and sixteen earlier diagnostic outcomes unchanged.
