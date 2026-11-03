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

The [requirements](requirements.json) fixed twelve slots before the calls: Luna Fast,
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
live slots were unstarted at that qualification. Those records remain historical;
the later complete results below use a fresh qualification and native admission.

These retained records describe the earlier preparation code. The native runner
now recognizes **`M3-delivery`** as a separate study. A prepared protocol keeps
live calls disabled. Only an execution plan frozen after fresh hosted checks and
all six complete local checks enables a call. Do not relabel an old algorithm
slot or reuse a closed slot to run it.
Public uploads remain submitted patches and reviewed summaries only. Raw local
records and account data stay private. The twenty original algorithm outcomes
and every M3 outcome remain unchanged.

## Check the complete local workflow before a live call

Admission means checking that the actual client, tools and complete workflow fit
the declared budget. A scripted local server supplies replies; it forwards no
request to a model provider. The real CLI must execute the commands and finish
its session accounting. Check Luna, GLM and Kimi under both setups: six checks.
Each check keeps 60 seconds and 768 MiB, with 128 MiB left for live client state.
An initial-goal check alone cannot satisfy admission.

Use a private Python 3.14.6 environment and install the existing hash-locked
`tests/agent-eval/selected-agents/native-requirements.txt` with
`python -m pip install --no-deps --require-hashes --requirement FILE`.
It pins WebSockets, the library used by the scripted Codex server. The preflight
rejects a missing or different package before starting a control. Receipts bind
that dependency and the Python executable too. Use that environment's Python
for the control and the frozen runtime.

First download the successful current algorithm workflow's `algorithm-qualification`,
`proof-delivery-controls` and `algorithm-macos-tool` artifacts. An artifact is a
file bundle saved by GitHub. Use the exact Mac executable, its source record and
the pinned local client and Lean installations. The qualified task inputs are
inside `algorithm-controls/inputs/euclid`; the delivery receipt is
`proof-delivery-controls/summary.json`. The earlier retained receipt above cannot
replace fresh checks after changing the runner.

For example, run the shared-skill check for Luna from the repository root.
Replace each `/absolute/...` path with the verified tool or downloaded input it names:

```sh
PYTHONPATH=tools:sdk/python/src python3 -B -m agent_eval.algorithm_admission observe \
  --study M3-delivery --setup shared-skill --profile codex-luna-fast \
  --fr /absolute/algorithm-macos-tool/fr \
  --client /absolute/pinned-codex/codex \
  --lean-root /absolute/lean-4.28.0 \
  --inputs /absolute/algorithm-controls/inputs/euclid \
  --controls /absolute/proof-delivery-controls/summary.json \
  --output /private/tmp/new-luna-shared-skill-control \
  --guard /Users/zardoz/.codex/tools/fr-local-guard.py
```

The observer measures outside the CPU throttle; it launches actual work through
the required guard. Keep the checks serial and use a new private output directory
each time. Keep long-lived tools, receipts and execution plans under the ignored
`.local-agent-runs/` directory in this workspace. Create it with owner-only access.
Temporary system directories may disappear between sessions. Delivery projects
stay beside their explicit execution plan, so both the observer and controller
measure the same persistent project directory. Run `finish-card` too, then repeat both setups for `opencode2-glm-flash`
and `opencode2-kimi-k3` with the pinned OpenCode2 executable. Each summary binds the
study, setup, client, fr, Lean executables, task archive, checking code and guard.
It must show both saved proofs, package success, saved-edit export, patch replay
in a clean checkout, and finished accounting. Stop and diagnose any failed check;
keep its receipt and the unchanged limits.

Combine the six successful `summary.json` objects in one private JSON object.
Its keys are `PROFILE/SETUP`, for example `codex-luna-fast/shared-skill`.
Pass this file as the explicit `admission` path to `selected_local freeze`.
The other runtime paths are `python`, `rust_root`, `lean_root`, `fr_build` and
`controls`. The binaries file names exactly `fr`, `codex` and `opencode2`.
`fr_build` is the verified Mac-tool directory; `controls` is the fresh delivery
receipt. Freeze with `--milestone M3-delivery` and the qualified `inputs` parent
directory. The freezer refuses a missing setup, changed executable, incomplete
workflow, or inadequate memory reserve before writing an enabled execution plan.

Run each of that plan's twelve IDs once through `agent_eval.selected_guarded`.
Use a fresh conversation and project for each. Keep every failure and timeout.
Review the actions and actual timings before exporting patches and summaries
with `selected_public export-algorithm --study M3-delivery`. The
`grade-algorithms.yml` workflow accepts that study and independently checks every
patch before applying the fixed decision rule. A valid late patch stays failed.
The collector copies the patch before its guarded process exits. Its completed
attempt time therefore gives an upper bound on patch capture. Scoring uses that
bound and the completed accounting receipt. The exact first patch-creation time
can remain unknown; a guessed stage time cannot turn a failed attempt into success.

## Complete results

All twelve fixed attempts ran once. The
[independent GitHub grader](https://github.com/e6qu/fun-refactor/actions/runs/41645916940)
accepted all eleven delivered patches. The
[report](../proof-algorithms/local-results/M3-delivery/README.md) retains every outcome.

| Agent | Accepted patches | Complete attempts: shared skill / finish card |
| --- | ---: | --- |
| Luna Fast | 4 / 4 | 2 / 2 and 2 / 2 |
| GLM Flash | 4 / 4 | 1 / 2 and 0 / 2 |
| Kimi K3 | 3 / 4 | 0 / 2 and 0 / 2 |

Six OpenCode sessions finished and delivered correct patches, but their JSON
streams omitted the final response's usage record. The saved sessions contain it.
Complete usage therefore remains unknown under the frozen rule; these six attempts
stay failed. One Kimi shared-skill attempt saved both proofs and passed the package
check, then spent its remaining time on an unsupported history flag and delivered
no patch. The deadline was not extended.

The [review](../proof-algorithms/local-results/M3-delivery/review.md) explains the
accounting failure, runner repair and next checks. Luna's result is a tie; the
GLM/Kimi comparison cannot support a guidance conclusion. No profile qualifies for
a delivery transfer test. Both proofs were supplied, and M3 remains open.
