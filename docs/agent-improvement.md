# Learn from each agent experiment

We want smaller agents to deliver correct changes and proofs with less wasted work.
Each experiment must answer a question that changes what we do next.
This process improves fr, its instructions and the checks we use to judge them.

## What the existing proof runs establish

The [retained comparisons](../tests/agent-eval/proof-writing/README.md#earlier-attempts) include these complete runs.
Each count below covers six attempts with fr available: two attempts on each of three tasks.

| Complete comparison | GLM accepted / 6 | Kimi accepted / 6 |
| --- | ---: | ---: |
| First | 4 | 4 |
| Second | 2 | 4 |
| Clock includes sleep | 3 | 4 |
| Reader repair, high reasoning | 4 | 4 |
| Low reasoning | 3 | 4 |
| Assumptions and conclusions view | 3 | 4 |
| Direct checking commands | 4 | 4 |

Neither agent delivered an accepted page-count proof in these fourteen attempts per agent.
Their latest access and range deliveries all use fr checks and writes.
The [behavior review](../tests/agent-eval/proof-writing/behavior-review.md) records searches, mistakes and late checks.
That shows a workflow change. It does not show a sustained rise in total success.
Several changes and settings differ between comparisons, so the history cannot identify one cause.

The latest successful fr tasks took about 48–98 seconds for GLM and 64–82 seconds for Kimi.
Those observations support 150 seconds for these smaller tasks.
Luna proved page counting in about 73 seconds; GLM and Kimi still have no observed completion time for that proof.
A timeout tells us where observation stopped. It cannot tell us when, or whether, another minute would produce a proof.

## Follow the same loop for the tool and its evaluator

1. **State the user's result.** For page counting, keep the program and claim fixed and deliver a patch that passes independent Lean checking.
   Record the time, failed attempts and unknown costs alongside that result.
2. **Find the first unresolved obstacle.** Record what the agent requested, what the tool returned and what the agent did next.
   Separate observations from explanations. An unfinished response does not establish a provider outage or continuous model computation.
3. **Check the measurement.** Give the evaluator deliberately wrong inputs and check that it rejects them.
   Reproduce suspected tool defects without a model call before testing another instruction.
   Check every study mode's execution instructions too: protected files, serial work, required checks and finishing rules must stay present.
4. **Write one explanation and its prediction.** Name the proposed change, the comparison, the expected result and what would contradict it.
   Freeze these choices before starting. Keep the model, task and unrelated settings identical between the compared conditions.
5. **Run the smallest useful diagnostic.** Test the suspected obstacle on a tiny example before another complete comparison.
   Record whether the example teaches a step, supplies feedback or asks for an unaided solution.
6. **Make and record a decision.** Keep, revise or discard the change using the rule written before the run.
   A mixed result remains inconclusive. Preserve every outcome and choose a different question when the explanation fails.
7. **Check the whole task and a different example.** A useful local repair must still deliver a correct complete result.
   Use a previously uninspected example before claiming that the improvement applies beyond the development task.

After each cycle, review the prediction itself. Did its measurement distinguish the competing explanations?
Did the proposed change help the intended step? Did that help the final result?
When the prediction fails, revise the explanation or the measurement before choosing another implementation change.
Keep a small regression check for each confirmed defect. This gives the next cycle a stronger starting point.

Two diagnostic cycles that leave the same obstacle unchanged trigger a review of the explanation and task requirements.
Do not keep changing wording or repeat the full study until one run happens to pass.

## Check what each measurement means

| Observation | What it establishes | What still needs checking |
| --- | --- | --- |
| The shell command exits successfully | The process returned normally | Read `passed` and the diagnostics; fr can return a rejected proof in successful JSON output |
| The agent invokes fr | Tool use occurred | Did the returned information help the next action? |
| A draft receives useful feedback | The checker exposed a relevant goal or error | Did the agent interpret it and repair the proof? |
| An intermediate goal passes | That proof step works | Does the entire unchanged theorem pass? |
| A patch passes Lean | The submitted model proof checks | Did the client finish delivery within its limit and preserve protected inputs? |
| A familiar example improves | That development example improved | Does a different example improve too? |

Supplying an error report at the start makes feedback earlier by construction.
It only helps the user if the agent repairs and delivers a correct result more reliably or with less complete work.
Checking earlier alone cannot count as that improvement.

Keep both timing views: matching successful tasks, and total time including failed attempts.
The baseline is the same agent doing the same task under the declared comparison condition.
Do not compare a successful access proof with a failed arithmetic proof and call the difference fr overhead.
Completed tool intervals leave other elapsed time unexplained; they do not isolate model speed, provider waiting or fr execution time.

## Check the evaluator with known wrong answers

The [report tests](../tools/agent_eval/test_selected_report.py) deliberately supply late successes, stopped attempts and invalid times.
The calculator must reject them before reporting a success or speedup.
It uses the recorded elapsed time and explicit deadline, and keeps timed-out valid patches separate from completed attempts.

The [report audit](../tests/agent-eval/proof-writing/report-audit.json) checks twelve complete reports covering 480 M1, M2 and M3 attempts.
The stricter calculator leaves their statistics unchanged.
Their original summaries, patches and hashes remain unchanged.
This audit establishes agreement on those records. It does not establish that every possible collection error has a test.

For a proof checker, retain a clean accepted proof and deliberately wrong variants.
Check unfinished proofs, changed claims, stale dependencies, draft warnings and protected source edits.
For a timing collector, check overlapping tool calls, missing timestamps and unfinished responses.
Missing evidence stays unknown. A guessed zero can create a false speedup.

## First controlled M3 diagnostic

The next complete 36-attempt comparison is paused. Native and CI regression checks can continue.
The [roadmap](../PLAN.md#next-work) owns the remaining milestone work.

The first question was whether the agents could repair a specific arithmetic error once they had useful feedback.
This separates interpreting feedback from discovering a complete page-count argument.
Use an unrelated small integer-division claim and a draft with the observed wrong-argument-type mistake.
Check that Lean rejects the original draft and accepts a repair before making a paid call.
Keep supplied reference tactics outside both agent projects.

The following choices were fixed before its eight attempts:

| Choice | Fixed requirement |
| --- | --- |
| Agents | The pinned GLM Flash and Kimi K3 profiles and local OpenCode2 release |
| Conditions | Same task, draft, fr and instructions; one condition also receives fr's actual rejection report at the start |
| Changed factor | Initial visibility of the diagnostic; no extra proof steps or library hints |
| Repetitions | Two per agent and condition, eight attempts total; fresh sessions and balanced condition order |
| Limits | Original 150 seconds and workstation resource limits; include generating the initial check in both conditions' complete time |
| Main result | Independently accepted repair and completed delivery within the limit |
| Additional observations | First correct repair, repeated type errors, complete elapsed time and available usage |
| Decision per agent | If supplied feedback gives 2/2 accepted repairs and the other condition gives 0/2, test clearer feedback exposure next |
| Alternative decisions per agent | If both give 2/2, inspect full-proof construction next; if both give 0/2, inspect diagnostic comprehension; every mixed result remains inconclusive |
| Potential harm | If supplied feedback gives 0/2 and self-checking gives 2/2, investigate whether supplying the report obstructs repair |

Two repetitions can expose a repeatable obstacle; they cannot establish a general success rate or a definitive cause.
First check execution validity. A service, collection or resource failure cannot establish that an agent misunderstood feedback.
Keep such failures in the record, clarify their cause and suspend conclusions about the proposed change.
This is an assisted diagnostic on a development example. It cannot close the page-count milestone or replace its failures.
The [diagnostic guide](../tests/agent-eval/proof-feedback/README.md) records its separate eight-slot schedule and independent patch controls.
The shared runner now has an explicit diagnostic mode; the full study keeps its separate thirty-six slots.
Its hosted controls passed before inputs were frozen and the eight attempts ran once.

### Result and correction to the method

The [result and observed failures](../tests/agent-eval/proof-feedback/README.md#what-the-first-run-established) retain all eight attempts.
Independent checking accepted all eight proof patches; six complete attempts passed.
GLM timed out after a passing proof and package check. Kimi replaced the supplied checking configuration with proof-check output.
Both remain failures. GLM's feedback comparison is inconclusive; Kimi's altered checking input blocks interpretation.
This supports no claim that showing the initial report improves task success.

Review found an evaluator defect: the diagnostic's shorter prompt skipped the shared execution instructions.
The repair now adds those instructions to both conditions. A regression rejects the exact measured implementation and passes the correction.
The skill also explains that `.fr/checks.json` holds settings and that proof writes report `applied`.
The later construction check used these corrections. Its different task and single condition cannot isolate their effect.

The method therefore adds an instruction audit before paid calls. Checker rejection controls alone missed this problem.
Keep a passing proof separate from finishing the whole task, and read the actual output fields before inferring another required action.
The next case started with an empty proof region and a different claim, asking for construction rather than repair of a supplied draft.

### Construction result: check valid syntax as well as the reference

The [four multiplication attempts](../tests/agent-eval/proof-construction/README.md#what-the-first-run-established) ran once after all prerequisite checks passed.
Independent grading accepted GLM's two patches and Kimi's one patch. The other Kimi attempt timed out without a patch.
All four kept their checking inputs unchanged. The original decision rule requires inspecting that failure before another paid batch.

Both Kimi attempts wrote inline calculations: proof steps that chain equalities or inequalities.
Fr rejected the syntax before Lean ran and pointed to the first proof line.
The failed attempt changed indentation and searched the library. The other changed proof form and finished.
Separate identity proofs confirm the defect without model calls: Lean accepts the inline chain, while the measured fr rejects it.
Fr accepts the same chain when its first step starts on a new line.

The method therefore adds valid syntax variants to the controls.
One accepted reference proof does not establish that every supported way to write that argument reaches Lean.
Check inline and nested forms, following tactics and malformed steps before testing an instruction change.
Repair the observed tool defect first. Keep the timeout failed and separate this small example from page-count success.

Broader comparisons resume after a diagnostic supports a concrete change and the relevant regression checks pass.
Record each experiment's source and input hashes, result, decision and next question before starting another cycle.
Publish only patches and summaries. Keep native logs, prompts, drafts and account data local.
Keep the original 150-second benchmark and its failures intact when discussing any different future budget.
