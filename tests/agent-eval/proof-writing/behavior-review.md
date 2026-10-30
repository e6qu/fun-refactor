# What GLM and Kimi actually did

Neither agent reached a proof check in any of the eight page-count attempts with fr
across the latest low-setting comparison and the preceding high-setting comparison.
These attempts cannot show whether improved proof-error messages help either agent:
neither got far enough to receive those messages.

This review covers the 48 OpenCode2 attempts in those two comparisons.
It compares recorded commands, returned tool results, timestamps and reported token counts.
The [latest summary](local-results/summary.json) and
[preceding summary](introduction-results/summary.json) retain the attempt identifiers and record hashes.
Raw transcripts, private drafts, source archives and account data remain local.
No attempt was rerun or reclassified for this review.

## The four recent page-count attempts with fr

Times below are seconds since the complete attempt started. Each attempt stopped at 150 seconds.
The tool column adds completed tool-call intervals, including reads and writes, counting overlapping time once.
It measures recorded tool intervals, not isolated CPU time or fr overhead.

| Agent and repetition | Completed tool time | Last completed tool | Observable result |
| --- | ---: | ---: | --- |
| GLM 1 | 11.4 s | 103.2 s | Read the skill and model, then made six library-search calls; no draft or fr command |
| GLM 2 | 15.5 s | 147.6 s | Read both skill pages; started writing a draft at 139.4 s; looked up the fr goal at 146.8 s; never checked the draft |
| Kimi 1 | 6.4 s | 86.8 s | Read the skill and model, then made two library-search calls; no draft or fr command |
| Kimi 2 | 5.5 s | 54.8 s | Read the skill and model, then made two library-search calls; no draft or fr command |

All four read the root skill. GLM 2 also read its Lean reference.
The root skill already told them to check a draft before searching the library.
Kimi 2 located the installed fr executable, but did not invoke it.
Finding the skill or executable was therefore not the immediate obstacle in these attempts.

Kimi's last recorded response starts at 90.4 seconds in repetition 1 and 61.0 seconds in repetition 2.
Neither response produces another tool call or final answer before the deadline.
In the earlier high-setting Kimi repetition 1, the last tool completes at 11.9 seconds.
A response starts at 15.5 seconds and produces no further observable action before termination.

No client or collection error appears in the four recent command records before termination.
They report no system sleep and use about two to three sampled CPU seconds each.
Their sampled memory peaks are far below the 768 MiB attempt limit.
They stop on the wall-clock limit, not a CPU, memory, disk or transcript limit.
Most elapsed time lies outside completed tool calls.
Command timestamps alone cannot split that remainder into model generation, provider waiting, client overhead or unfinished calls.
The saved session metadata below narrows the explanation for Kimi.
Reported token counts cover finished responses; they omit any unfinished response at termination.

## Saved sessions show long unfinished analysis

After the attempts closed, a read-only export through the pinned client recovered their saved response metadata.
No model was called. Original attempt records and timeout results remain unchanged.
The export is a later diagnostic inspection, not session accounting completed within the attempt's deadline.
The twelve inspected sessions retain the requested model, reasoning variant and build agent.
That verifies the saved client selection, not the provider's internal computation policy.

Kimi's final responses contain substantial text in the client's reasoning field:

| Setting and repetition | Recorded reasoning interval | Characters in unfinished reasoning |
| --- | --- | ---: |
| High 1 | 15.5–150.1 s | 13,358 |
| High 2 | 73.1–150.1 s | 18,978 |
| Low 1 | 90.4–150.1 s | 6,047 |
| Low 2 | 61.1–150.1 s | 10,022 |

The text itself stays private and was not inspected for this review; these are lengths and timestamps only.
Each final response is marked aborted at the deadline.
This rules out interpreting the whole gap as a response that produced nothing:
Kimi generated substantial analysis without reaching another command or delivering a proof.
The intervals still include any waiting inside the response; they are not isolated model-compute measurements.
Character counts are not token counts, usage totals or charges.
The low setting did not prevent this behavior in either repetition.

The earlier high-setting GLM sessions also retain long analysis parts, including one with over 12,000 characters before a late goal lookup.
The latest GLM repetition 1 has no saved final response after its last tool result, so its final delay remains unexplained.
The latest GLM repetition 2 ends with an aborted response after the late goal lookup.
There is no evidence here that either aborted response is a separate error that caused the timeout.

Four inspected low-setting simple-task sessions provide a useful contrast.
They complete their responses, reach proof checks and deliver patches.
Their longest saved reasoning parts range from zero to about 1,300 characters.
This is a small descriptive comparison, not a general limit on how much analysis a successful proof needs.

## Fr's lookup still adds friction

In the earlier high-setting GLM repetition 1, the first library lookup happens at 135.8 seconds.
It requests a source-byte budget without enabling source output, so fr refuses the command.
The corrected lookup starts at 140.4 seconds and returns about 10 kB of text despite its 700-byte source budget.
That budget covers source excerpts only; identifiers, locations, signatures and other report fields add bytes.
It returns 14 of 80 matching declarations, leaving 66 for later pages.
This is a poor last-second lookup even though the command itself completes in under two seconds.

A separate guarded inspection with the pinned fr executable finds the exact `ediv_le_self` declaration and its signature.
It also reports syntax gaps in 13 library files.
That result establishes that this declaration is visible, not that fr reads the whole standard library correctly.
For future assistance, prefer a small set of intact declaration names and argument types over a broad page of proof bodies.

## GLM's searches sometimes damage the answer

In low-setting repetition 1, GLM uses `rg -rn` while looking for division inequalities.
For ripgrep, `-r` means replace matched text; the following `n` is the replacement.
The output therefore changes the real declaration name `ediv_le_self` into `n_self`.
GLM's next command searches for `theorem n_self` alongside other names.
It later recovers the real name, but this consumes another search without advancing a proof.

This was an agent-generated command, not a command supplied by the skill.
It also demonstrates why an apparently successful shell command is not enough:
the search returned text, but that text no longer preserved the library's names.

In repetition 2, GLM reads the actual declaration of `Int.tdiv_nonneg`.
Its later unsubmitted draft nevertheless supplies ordinary integer arguments where the declaration expects proofs of inequalities.
A separate, tiny synthetic Lean check reproduces that argument-type error.
The draft also selects the item-count upper bound where it needs the page-size lower bound.
Those are different parts of the supplied assumptions, not interchangeable facts.
The original draft remains untested in its recorded attempt; this inspection does not create a new grading result.

## The ordinary-tool attempts expose the arithmetic difficulty

Both setups receive the same model and claim. On valid inputs, page counting divides the number
of items by the page size and adds one when a remainder remains.
For example, eleven items need three pages of five. The proof must also show that this rounding
never produces a negative result or more pages than items.

Across both comparisons, four of the eight ordinary-tool page-count attempts reach a Lean build.
None submits a patch; none of the eight fr attempts reaches a proof check.
This is an observed difference in these runs, not proof that fr causes the delay.
The ordinary-tool checks expose several distinct mistakes:

- GLM uses `nlinarith`, an arithmetic tactic absent from the supplied standard-library-only installation.
  It also uses an unqualified `le_of_lt` name that the installed Lean does not resolve.
- Both agents confuse related division identities or their orientation.
  For example, a fact stating `size * quotient + remainder = items` does not directly prove
  `items = size * quotient + remainder`; the equality needs its direction reversed.
  Switching the multiplication order is another separate step.
- Kimi repeatedly asks `omega` to finish a goal involving a product of changing integer values.
  This arithmetic tactic does not infer every missing relationship between those values.
  Lean's returned counterexample constraints show the relationship it still lacks.
- Some commands print only the last 15–30 lines of a failed build.
  This can hide the first error while preserving later failures caused by it.

The agents do find relevant library facts; a blanket claim that the library was inaccessible is incorrect.
Their difficulty includes turning the facts into correctly typed, checked proof steps.
Searching more names alone does not establish that progress.

## They can use fr on simpler proofs

In the low-setting comparison, both agents deliver all sixteen access-rule and range-check patches
across the two tool setups. Fifteen attempts complete normally.
The remaining GLM attempt deletes the supplied check file after checking its proof and correctly stays failed.
Independent grading determines patch validity; delivery alone is not acceptance.

With fr, their first correctly addressed proof checks on these simpler tasks start around 14–38 seconds.
They then save the proof and deliver the patch within the deadline.
Some initially guess the wrong target format and recover from fr's refusal.
These examples show that both clients can discover and execute the proof workflow.
They do not establish that the same approach succeeds on the arithmetic task.

The page-count prompt adds 151 bytes for the fr setup before any skill reads.
One of four recent fr attempts reads the longer Lean reference.
The two repetitions per setup cannot establish that this extra context causes the different behavior.
Both ordinary-tool and fr page-count attempts fail, and model/provider timing is not isolated.

## What the next change must address

The current draft-location and retained-goal fixes address real failures after proof checking begins.
They do not explain the absence of checks here.
Repeating the same comparison or increasing its deadline would leave that problem unresolved.

The next proof-assistance work should make the transition from a claim to a first checked draft concrete.
It should expose the claim's parts and available declaration signatures without supplying an evaluator's proof.
For example, an agent should be able to inspect an inequality fact's required arguments,
try it in a small draft, and receive the exact remaining goal before another broad search.
The success measure is an earlier useful check and a correct delivered proof, not more fr invocations.

The current branch implements the first part: `proof-task` exposes each claim part's assumptions and conclusions.
It replaces empty proof templates, and the skill explicitly asks the agent to inspect this view before drafting.
The work also corrects the syntax reader's grouping of proposition operators.
These changes still need hosted validation and a new comparison; the recorded failures above remain unchanged.

Before claiming a fix, distinguish these questions:

1. Did the agent check a draft early enough to repair it and deliver a patch?
2. Did it use actual declaration names and argument types from returned results?
3. Did Lean accept the unchanged claim in the independent receiver?
4. Where did elapsed time remain unaccounted for by completed tool calls?

Keep the selected models, fixed claims and 150-second limit.
Any new comparison must retain failures and use newly frozen inputs after a concrete repair passes its checks.
