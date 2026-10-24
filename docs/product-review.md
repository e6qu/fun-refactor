# What users can rely on and what we still need to measure

Fr should help an agent finish a correct change in unfamiliar code with less total effort.
A useful result includes the explanation, reviewed patch, behavior checks and a dependable way to continue after edits.

For example, a checkout rounding repair must find the calculation and relevant callers, fix the reported case,
and pass checks that reject the original mistake.
Finding the function or applying an already supplied replacement establishes only part of that outcome.

Use [PLAN.md](../PLAN.md#next-work) for the concrete upcoming work.
The [status report](roadmap-status.md) owns current acceptance counts.
This page explains the missing product results and the comparisons needed to evaluate them.

## What we can demonstrate

| User need | Checked behavior | What remains unproved |
| --- | --- | --- |
| Find the relevant code | Declarations, known callers, exact locations and selected source reads | An agent choosing the right explanation on an unfamiliar task |
| Follow a submitted value | Selected original-package questions and declared Python examples | Complete imports, runtime errors and shared-object behavior |
| Make and deliver a change | Selected repairs and renames with checks, undo and patch replay | An agent independently discovering a generally correct repair |
| Continue after another edit | Affected answers expire; unrelated findings survive; saved and fresh answers agree | General reductions in task cost |
| Check a property | Small model proofs and bounded behavior comparisons | A checked argument covering the actual implementation |

A possible value connection describes the supported analysis.
Running Python checks observed behavior for supplied inputs.
A proof checks a stated rule under assumptions.
Keep these meanings distinct in reports and tutorials.

## Agent setup

The same project skill serves Claude Code, Codex and OpenCode.
The [README](../README.md#give-fr-to-an-agent) links their discovery locations and the complete [portable skill](../skills/fr/SKILL.md).
The [first-change lesson](../skills/fr/references/first-change.md) introduces finding, reviewing, checking and undoing a rename.

Scripted controls use the actual applications with ordinary tools, CodeGraph or fr.
They check assigned tool access, complete shutdown, interruption, rejecting a stale edit and replaying a refreshed repair.
The [preparation guide](agent-comparison-preparation.md) records their configurations and limits.

These checks establish installation and prescribed behavior.
A live attempt must show whether an agent uses the offered tool and discovers a correct repair.
The existing native-change records include behavior passes with zero fr calls; those passes establish no benefit from fr.
See the [change results](native-change-outcomes.md) and [reading results](native-read-outcomes.md) for the recorded outcomes.

## What the cost records mean

The [complete session comparison](project-session-cost.md) asks matching questions on glob, Boltons and JMESPath.
It compares direct command calls, a Python client restarted per question, a client kept running,
and saved answers checked before reuse.
All approaches use the same optimized native fr program.
The direct-command baseline includes a small Python script that launches commands and reads JSON.

Session time includes startup, source preparation within the session, questions, edits, validation, shutdown and cleanup.
Downloading sources, building the native program and preparing the Python package are separate setup costs.
All 135 answers must agree, including incomplete and missing-name answers.

Direct command calls were fastest in that recorded comparison.
Keeping Python running reduced its startup overhead.
Saving answers added work and showed no consistent advantage over fresh Python checks.
Those observations cover three familiar projects and a short prescribed session.
They do not measure an agent finding a repair, total model charges or general Python-versus-Rust performance.

Read the [measurement guide](benchmarks.md) before comparing a timing or storage number.
It identifies each operation, baseline, included work, missing costs and limits.
Preserve unsuccessful and inconclusive results.

## First comparison with CodeGraph

This plan uses [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph).
The [prepared schedule](agent-comparison-preparation.md#the-proposed-18-attempts) uses two existing tasks:

- Repair a Unicode string comparison.
- Calculate escaped text length without allocating a temporary string.

Run each task with Claude Code, Codex and OpenCode, using three tool setups:
ordinary tools; those tools plus CodeGraph; those tools plus fr.
That gives eighteen first attempts.
The projects are familiar to development. Fresh application sessions do not make them unread projects.

Keep ordinary searching, reading, editing and checking available in every setup.
Keep each specialized tool out of the other setups.
Use the same exact model and reasoning settings within an application's three setups.
Rotate setup order and grade each delivered patch with the same independent checks.

Before live calls, record models, settings, task and tool versions, prompts, credentials, prices and total spending limits.
The existing prepared task limits remain in force.
The schedule binds those inputs to their exact contents; review it again after a relevant change.

Count installation, indexing, instructions, tool calls, edits, retries, checks and shutdown.
Count background programs as well as the program the agent starts directly.
For example, an indexer can remain running after its client exits.
The [container controls](agent-comparison-preparation.md#check-tool-setup-and-background-programs-without-a-model) test that accounting and cleanup.

Report correctness first. Then report full costs, including failed attempts and cases where the offered tool went unused.
This first comparison tests integration and task behavior.
It cannot complete the larger repeated-task requirement.

## Repeated-task study

This is the separate requirement behind milestone A.
It remains planned, not authorized or running.

Select four task instances across at least three independent projects:

1. Explain a behavior.
2. Repair a bug.
3. Add a feature.
4. Prove a stated property of a bounded function.

Choose requirements and independent checking programs before implementing support.
Keep one project unread until the checker and procedure are frozen.
Record project revisions, model settings, prompts and full-attempt resource and spending limits.

The established study has 48 attempts:

| Attempt type | Required coverage | Count |
| --- | --- | ---: |
| One agent | Four tasks, two model families, two repetitions, ordinary tools and ordinary tools plus fr | 32 |
| Delegated work | Two of those tasks, both model families, two repetitions, both tool setups; at most two child agents per attempt | 16 |

A repetition is another complete attempt with the stated setup.
A model family identifies the selected model line; freeze exact model identities before running.
Both setups get the same ordinary tools, requirements and checking access.
The independent hidden checks stay unavailable to both.

Start serially on remote workers with an agreed total spending cap.
Do not substitute another model when access or accounting is missing.
Keep failed, cancelled and unstarted attempts visible.

This small repeated study can expose overhead and failure modes.
A general success-rate or savings claim needs a larger independent evaluation with a stated method for quantifying uncertainty.

## Account for the whole attempt

Retain:

- Whether the delivered behavior passes independent checks, including regressions, unsupported claims and human intervention.
- Exact model and application settings, raw provider usage, prices and actual or explicitly estimated cost.
- Every parent and child agent's work, failed children, summaries and integration work.
- Setup, instructions, source reads, tool results, repeated reads, retries and whether the offered tool was used.
- Elapsed time and summed agent time. Parallel work can shorten elapsed time while increasing total work.
- Tool processor work, sampled combined memory, storage and cache growth, builds, checks and full shutdown.

A token is a model-specific unit of text.
A response byte limit does not measure the whole conversation or the provider's bill.
Cached and new input can have different prices; keep the provider's categories and missing fields intact.
Sampling memory can miss short peaks. Explain what the monitor includes.

Show successful and unsuccessful outcomes before cost summaries.
Report costs for every attempt and for comparable successful pairs.
Total spending divided by successful outcomes is undefined when there are no successes.

## Proofs must describe the implementation

[R3](../PLAN.md#r3-prove-the-properties-of-the-code-we-ship) names the existing page-count function and new edit-range check.
Both need exact input domains, arithmetic behavior and a checked connection from source to the stated property.
A relevant edit must expire the old proof claim.
A fresh check must also reject deliberately incorrect code or a mismatched model.

Use existing input-search tools when a concrete task needs them.
The [earlier symbolic-execution investigation](python-symbolic-investigation.md) found a failing input already covered by its generated tests.
It establishes no present need to build another engine.

## Stopped work and historical decisions

Keep the closed client investigation and stopped packaging study stopped.
The [candidate review](candidate-review-status.md) retains their unresolved questions and original outcomes.
This document does not authorize paid calls or restart either study.

The [pre-cleanup product review](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/docs/product-review.md)
preserves the dated setup review and earlier design discussion.
New results belong beside their original inputs and limits.

