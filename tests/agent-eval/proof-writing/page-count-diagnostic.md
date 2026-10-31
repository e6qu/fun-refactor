# Next diagnostic: finish the page-count proof

This is a planned four-attempt diagnostic, not a completed result.
The `M3-page-count` study has a separate four-slot schedule and shares M3's original task preparation.
Pass its checker controls and freeze its execution plan before making calls.
Keep the full thirty-six-attempt comparison paused until this diagnostic supports restarting it.

## Question and prediction

Can GLM Flash and Kimi K3 finish the fixed page-count proof with the repaired reader and diagnostics?
For example, eleven items at five per page need three pages.
For valid inputs, the number of pages must stay between zero and the number of items.

The earlier attempts reached rejected drafts late, or searched without checking a draft.
The smaller diagnostics found instruction omissions, rejected valid syntax and lost error locations.
The reader repair now passes hosted checks and an exact replay without model calls.
The tagged-error repair still needs that qualification.

We predict that both agents can deliver at least one accepted proof in two attempts each.
This tests complete delivery on the unresolved task. It cannot isolate the effect of one repair.
If either agent delivers none, the prediction fails and the full comparison stays paused.

## Fixed conditions

| Choice | Requirement |
| --- | --- |
| Task | The existing [Rust page counter](../checked-translation/pages.rs), generated Lean definitions and unchanged `page_bounds` claim |
| Agents | The GLM Flash and Kimi K3 low-reasoning profiles in [profiles.json](profiles.json), with pinned OpenCode2 and fr executables |
| Attempts | Two fresh sessions per agent, four total, all with fr; alternate agents in the frozen order |
| Instructions | The current shared execution instructions and fr skill, fixed before the first call |
| Limit | The original 150 seconds for preparation, replies, commands, checks, patch delivery and session accounting |
| Other limits | The existing declared CPU, memory, disk and transcript limits; one local workload at a time under the guard |
| Protected inputs | Source, Lean definitions, theorem, assumptions, proof markers, package settings and `.fr/checks.json` |
| Reference proof | Keep it outside every agent project; start with an unfinished proof region |

Lean proves a mathematical model of the program.
Keep the invalid-input branch, signed-64 bounds and eleven-at-five example in the fixed claim.
An accepted model proof leaves the connection to Rust execution unproved.

## Check before calls

1. Pass hosted native, Python-record and documentation checks for the repaired code.
2. Check the rebuilt executable against the earlier valid and invalid drafts without model calls.
   The valid calculation must pass; unknown library names must remain rejected and point to their submitted lines.
3. Prepare this task with the existing M3 source and property generation.
   Accept the separate reference proof in an isolated project.
   Reject unfinished proofs, altered claims or source, wrong patches and changes outside the proof region.
4. Check the exact native client settings with scripted replies and no upstream model access.
   Freeze the four-slot plan, inputs, tool identities, instructions and decision rule together.

Do not reuse a closed plan, overwrite an earlier record or substitute a different model.
If the local guard refuses, stop local work. Do not raise its limits.

The executable requirements live in [requirements.json](../proof-page-count/requirements.json).
The `proof-page-count-prepare` workflow group checks scripted client replies and seven patch controls on GitHub.
It takes the passing M3 native build run as `collection_run`.
The later `proof-page-count-grade` group takes that preparation run and checks all four submitted patches independently.
Neither workflow makes model calls. Local calls use the explicit `M3-page-count` selection in `selected_local`.
The plan records the original page-count source, claim generator and isolated reference proof as checking inputs.
The reference proof stays outside the projects supplied to agents.

## Count complete results

An independent check applies each submitted patch to a fresh copy of the unchanged project.
It must accept the proof and verify exact patch undo and redo.
The original session must also show a useful fr check, an accepted fr write and a passing package check.
The client must deliver its patch and finish within 150 seconds while preserving protected inputs.
A valid proof from a timed-out session remains failed.

Record the first proof check, first passing proof, package check, patch delivery and session finish separately.
Keep unknown times and unavailable usage unknown.
Read returned checks and diagnostics; command execution alone does not establish success.
Preserve every failed slot and inspect any service, collection or resource failure before interpreting agent ability.

## Decision after all four slots close

If each agent has at least one accepted complete result, prepare the next full thirty-six-attempt comparison.
That comparison keeps the same claims, models, settings, two repetitions and 150-second limit.
It includes Luna and ordinary-tool attempts so we can compare complete times for matching successful tasks.

If either agent has no accepted result, inspect its first unresolved proof or delivery step.
Record the requested action, actual tool response and next action before choosing another change.
Do not start another broad comparison or lengthen the timeout to obtain a passing result.

These four attempts have no ordinary-tool timing baseline.
They cannot establish fr overhead, a causal improvement or a general completion rate.
Publish patches and reviewed summaries only for local agent attempts.
Keep transcripts, prompts, drafts, snapshots and account data local.
Retain the earlier 335 M3 outcomes, eight repair attempts and four multiplication attempts unchanged.
