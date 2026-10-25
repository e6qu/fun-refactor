# Check whether fr helps the selected agents finish real work

The product focus is refactoring, targeted code changes and Lean proof writing for smaller agents.
The [roadmap](../PLAN.md) names concrete tasks, completion checks and the three selected model/application profiles.
This page owns comparison rules. It does not authorize model spending.

## What the user should get

| Request | Useful result | Required check |
| --- | --- | --- |
| Rename a function | A reviewed change covering its declared callers and imports | Behavior stays the same; unrelated same-named functions stay intact |
| Repair a bug or add a feature | A patch meeting the requested behavior | Independent checks reject the old mistake and incomplete repairs |
| Prove a property | Agent-written Lean tactics for the unchanged claim | Lean accepts the proof; assumptions and the source/model connection remain visible |

Prefer retaining surrounding source, comments and formatting.
A deliberate wider rewrite needs a complete diff and explicit checks.
Syntax validity, behavior preservation and proof acceptance are separate results.

The existing tools can find declarations and callers, prepare changes, run declared checks, undo and export patches.
The proof tools expose Lean goals and check supplied tactics for supported models.
A scripted repair or a supplied proof establishes that path works.
A live attempt must show whether the selected agent can choose the right changes or write the proof itself.

## Compare the selected agents

Use Codex with GPT-6 Luna in Fast mode, OpenCode2 with GLM-5.3-Flash, and OpenCode2 with non-Pro Kimi K3.
The [target table](../PLAN.md#target-agents) owns exact model IDs.
The selected client releases pass [scripted native checks](https://github.com/e6qu/fun-refactor/actions/runs/40190446873)
for both tool setups, including real command output and effective model settings.
Scripted replies check the client integration; they do not establish live model access or task success.

For each profile, compare ordinary read/search/edit/check tools with those tools plus fr.
Use the same request, source revision, checking commands and limits within that pair.
The model receives no repair, proof tactic or edit location supplied by the evaluator.
Freeze independent task checks before extending support.
Use the exact requested theorem and definitions in both proof attempts.

M1 starts with a Boltons rename, the recorded Unicode repair and a Rust access-rule proof.
Three profiles, three tasks and two tool setups give eighteen first attempts.
One unchanged repetition gives thirty-six attempts.
Rotate setup order and keep all outcomes.
These familiar tasks measure development progress; they are not new unread-project qualifications.

The user selected the installed local CLIs with their existing account stores; credentials stay local.
Freeze exact client releases, model IDs, reasoning, speed, providers and enforced attempt limits before live calls.
Record limits the CLIs cannot enforce and keep unavailable account charges unknown.
Read effective settings from the application and reject substitutions.
Keep auxiliary model calls in the same accounting.
Configured credentials and a catalog listing do not establish successful access.
Do not inherit personal defaults or silently switch models.

## Measure correct results and complete costs

Correctness comes first.
A behavior pass means the delivered patch passes its independent checks.
A proof pass means Lean accepts the fixed property under the recorded definitions and assumptions.
A human supplying the missing solution is recorded intervention, not an unaided success.

Retain every attempt, including failures, timeouts, cancellations and unstarted work.
Record whether fr was offered, actually used and involved in the delivered result.
A successful ordinary edit with zero fr calls establishes no fr contribution.

Count:

- Model input, output, cached input, actual charges or explicitly estimated charges.
- Tool instructions, setup, indexing, source reads, edits, retries and checks.
- Application, tool and child processes, including background workers and shutdown.
- Complete elapsed time, processor work, memory, storage and human intervention.

A token is a unit of text counted by a model.
Bytes measure text size; neither value alone measures cost or time.
Report missing billing or resource data as unknown.
Separate first-use setup from later reuse, while keeping both visible.
Compare costs for equivalent successful outcomes and total spending per success.
If there are no successes, spending per success is undefined.

The [measurement guide](benchmarks.md) names existing baselines and overhead.
The older two-task Luna comparison found more calls, text and time with fr.
Its feature passed only with fr, while its bug repair passed in both setups.
Each task ran once. Those observations establish no general savings or success rate.

## Make the next improvement earn its place

Use transcripts to identify the obstacle before changing the tool.
For example, repeated reconstruction of a workflow manifest is different from choosing the wrong rounding rule.
A smaller report helps only when the agent still receives what it needs for the decision.

Reproduce tool errors with deterministic checks.
Fix the actual cause without increasing deadlines or discarding the failing example.
Improve the existing task route and its focused instructions.
Add analysis support when a named refactoring, code-change or proof task requires it.
Then prepare a new matched comparison and preserve the earlier batch.

## Proof acceptance

Lean checks a theorem against definitions and assumptions.
A model is a description of admitted source behavior in Lean.
Source hashes identify reviewed bytes; signature maps identify the declared arguments and result.
Neither establishes that the model computes the same result as the original program.

[M3](../PLAN.md#m3-help-the-agents-write-repair-and-maintain-lean-proofs) names the Boolean language cases and numeric properties.
Keep theorem text and assumptions fixed while an agent repairs tactics.
Reject admitted gaps, unapproved axioms and a weaker replacement claim.
Changes to a source function, model, property or relevant dependency must expire the affected result.

A model-only proof is reported as a model-only proof.
The earlier source-implementation proof requirements remain open until a checked connection actually satisfies them.
The focused proof-authoring milestone does not promise a verified translator for arbitrary programs.

## Existing setup and cost records

The project skill is linked from the [README](../README.md#give-fr-to-an-agent).
Existing scripted controls cover discovery, tool access, shutdown, stale edits and patch replay in their pinned applications.
The [preparation guide](agent-comparison-preparation.md) states their exact scope.
They are not live outcomes for the newly selected profiles.

The [session comparison](project-session-cost.md) asks matching questions on three familiar projects through five approaches.
Direct CLI calls were fastest in that record.
Keeping Python running reduced its startup cost; saving answers added work without a consistent advantage.
These prescribed questions do not measure an agent discovering a repair or writing a proof.

## First comparison with CodeGraph

This is an earlier preparation, retained and inactive under the focused roadmap.
It used [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph), two familiar tasks and three applications.
The proposed eighteen attempts covered ordinary tools, CodeGraph and fr in Claude Code, Codex and the pinned older OpenCode.
The [original preparation](agent-comparison-preparation.md) retains its controls and requirements.

That schedule is not the new M1 schedule.
Keep its original attempts and failures. Do not silently change model names, client releases or checking programs in an old record.

## Repeated-task study

The earlier broad study is retained and inactive.
It remains an unfinished requirement of the [earlier qualification checklist](earlier-roadmap-checklist.md), not a completion gate for the focused roadmap.

Its original scope has four task types: explanation, bug repair, feature work and a bounded proof.
It requires at least three independent projects, two model families and the original repetitions.
Thirty-two single-agent attempts and sixteen delegated attempts give forty-eight attempts.
The delegated cases allow at most two child agents per attempt.
No selected new comparison completes this earlier requirement by relabeling its results.

## Stopped work and historical decisions

Keep the closed client investigation and stopped packaging study stopped.
The [candidate review](candidate-review-status.md) retains failed, cancelled and unstarted work.
Original acceptance conditions and evidence stay in the [retained-results report](roadmap-status.md).
Moving an unfinished claim out of the active queue does not mark it complete.

The [pre-cleanup product review](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/product-review.md)
preserves the older decisions. The [previous roadmap](https://github.com/e6qu/fun-refactor/blob/c923d3cde7/PLAN.md) preserves the superseded work sequence.

