# Can an agent repair the arithmetic error once it sees it?

This small diagnostic follows the [improvement method](../../../docs/agent-improvement.md).
It tests GLM Flash and Kimi K3 on one assisted repair before another full proof comparison.
It has no live results yet and cannot complete M3.

The [source function](fixtures/quotient.rs) divides `a` by `b` using signed-64 integers.
The fixed Lean claim says its model returns a nonnegative quotient when `a` is nonnegative and `b` is positive.
Lean's `Int` means unbounded mathematical integers. The model uses division that truncates toward zero.
The source-to-model connection remains unproved; this experiment checks proof repair under the fixed model and assumptions.

The [starting draft](fixtures/initial-tactics.txt) supplies ordinary integers where a library theorem expects proofs of inequalities.
A tactic is one proof step. The agent copies this unchanged draft into `.fr/proof-tactics`, repairs it and asks fr to check it.
The initial checker must return `passed: false`, no proof receipt and a type-error diagnostic.
The separate [reference tactics](controls/reference.lean) prove that the task is solvable and stay outside each agent's project.

## One changed factor

Both conditions have the same source, initial draft, theorem, fr executable, skill, instructions and client settings.
The controller runs the same initial check within each complete attempt's 150-second budget.
One condition receives its actual rejection report in the initial prompt. The other can request the report through fr.
Earlier feedback in the supplied-report condition follows from that setup; it cannot establish a better result.

Two models × two conditions × two fresh repetitions give eight attempts.
Condition order reverses between repetitions and differs between models.
The [requirements](tasks.json), [profiles](profiles.json) and [checker settings](checkers.json) fix these choices.
Both models keep explicit low reasoning in local OpenCode2 2.0.32. No helper model or substitution is allowed.

A success requires a completed attempt within its original limits and an independently accepted nonempty patch.
Only the fixed proof region may change. The independent grader reapplies each patch in a private checkout with no network or credentials.
It checks the unchanged claim, allowed axioms, protected inputs and exact patch undo and redo.
A valid patch from a timed-out client remains a failed attempt.

Before model calls, the [hosted workflow](../../../.github/workflows/proof-feedback.yml) must pass four scripted native exchanges and seven patch controls.
Those controls accept the reference and reject missing or malformed patches, unfinished or wrong tactics, changed claims and source edits.
It also checks that the exact initial draft produces the actual type error.
The execution packet freezes those successful controls, native inputs, binaries, settings, initial source and checker image.
Dispatch the existing refinement workflow with group `proof-feedback-prepare`, an explicit prefix and the M3 build ID in `collection_run`.
After the eight attempts, group `proof-feedback-grade` takes the successful diagnostic preparation run ID in that field.
The existing serial runner uses the explicit `M3-feedback` study, which schedules these eight diagnostic slots.
It keeps the full M3 study and its thirty-six slots separate.

The runner's `freeze` command takes an explicit `--binaries` JSON file.
For this diagnostic it contains exactly `fr` and `opencode2`, each naming its absolute executable path.
The OpenCode2 companion executable must sit beside its launcher. The runner copies and verifies both before any call.
Complete M1, M2 and M3 comparisons also require the selected `codex` executable and its companion.
The separate `--runtime` JSON identifies `python`, `rust_root`, `lean_root`, `fr_build` and `controls` through absolute paths.
The last two identify the hosted macOS build directory and downloaded successful control receipt.
Freeze only committed inputs after the preparation workflow succeeds; use the workstation guard for every local invocation.

## Record enough to learn

Retain every outcome, complete elapsed time, available usage, patch identity and reviewed fr contribution.
Review the first observed correct repair and any repeated wrong-argument error after feedback becomes available.
Use null when a timestamp or error count cannot be established from the recorded tool output.
Native stream hashes bind the review to the original private output. Command counts alone earn no benefit credit.

The initial check consumes the same preparation time and transcript allowance in both conditions.
Complete elapsed time also includes native startup, responses, tools, delivery and session accounting.
CPU and memory samples describe the client process group; the workstation guard also monitors preparation and capture.
Actual account charges and unfinished-response usage remain unknown.

Apply the [recorded decision rules](tasks.json) separately to each agent.
First review execution validity: distinguish usable attempts from service failures, collection failures and unknown execution.
Resource stops, altered checking inputs and detected sleep also block conclusions about proof repair.
Keep their original outcomes and costs; repair or clarify execution before interpreting the comparison.
If supplied feedback produces 2/2 repairs and self-checking produces 0/2, investigate clearer exposure next.
If both produce 2/2, investigate constructing the full proof. If both produce 0/2, investigate interpreting diagnostics.
If supplied feedback produces 0/2 while self-checking produces 2/2, investigate whether the supplied report obstructs repair.
Every other result remains inconclusive. Two repetitions establish no general success rate or definitive cause.

Publish only submitted patches and summaries. Keep initial error reports, native logs, prompts, source snapshots and account data local.
Preserve all earlier M3 failures; this assisted example cannot replace them.
