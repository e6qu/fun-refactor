# What GLM and Kimi actually did

## Latest access and range deliveries use fr; page counting is still open

The [new complete comparison](local-results/summary.json) retains 36 attempts: 28 submitted patches and eight timeouts without one.
Independent grading accepted all 28 patches. This review now covers 96 OpenCode2 attempts across four complete comparisons.
Every outcome and raw-record hash remains recorded. Only patches and summaries are public.

Both GLM and Kimi checked and saved access and range proofs with fr in both repetitions.
That fixes a concrete workflow omission in the preceding comparison, where several successful patches never used fr.
All these proofs passed independent checking. The comparison does not isolate the effect of one instruction change.
Across seven complete comparisons, GLM's accepted fr-available attempts number 4, 2, 3, 4, 3, 3 and 4 out of six.
Kimi has four accepted attempts out of six in each comparison. Neither agent has an accepted fr page-count proof in fourteen attempts.
The first and latest total success counts therefore match; tool use has changed without a sustained overall increase.
The next complete comparison is paused for the [improvement method](../../../docs/agent-improvement.md).

Page counting remains the difficult case: all eight GLM/Kimi attempts timed out, including ordinary-tool attempts.
These four fr-available attempts received a root skill containing the exact checking and saving commands:

| Agent and repetition | Completed tool time | First proof check | Observed work |
| --- | ---: | --- | --- |
| GLM 1 | 12.3 s | None | Reads the goal and claim, then makes eight library-search calls; no checked draft |
| GLM 2 | 32.3 s | Lake at 88.0 s | Reads the model, searches, then makes two failed builds; no fr invocation |
| Kimi 1 | 11.7 s | None | Reads the goal and makes three library-search calls; no checked draft |
| Kimi 2 | 4.2 s | fr at 136.3 s | Reads the claim, searches, then checks two rejected drafts near the deadline |

Times begin when the complete attempt starts. Completed tool time includes reads and writes, counting overlaps once.
The remaining time includes replies, client overhead, waiting and unfinished calls; these records alone cannot separate them.
It is not isolated fr overhead or CPU time.

All four read the root skill; none opened the Lean reference or arithmetic lesson.
GLM again uses `rg -rn`, which replaces matching names, and sometimes supplies no replacement argument at all.
Its second attempt reaches real proof errors: `omega` cannot settle the supplied constraints,
and later tactics continue after a goal has already been solved.
Kimi's last attempt reaches fr only with about fourteen seconds left; its first checked draft applies facts with the wrong argument types.
The exact command alone therefore did not produce early feedback on this harder task.

The simpler tasks also expose recoverable mistakes. GLM's first range attempt guesses a namespaced target,
then a Rust target, then supplies a whole Lean module where tactics are required. It eventually copies the Lean target,
checks the tactics, saves the proof and verifies the package. Kimi also recovers from a Rust target in its first range attempt.
Both agents occasionally add the unsupported `--token-limit` flag to a write or package-verification command and then correct it.
The instructions keep the actual supported commands explicit; no compatibility form is added.

Luna's first page-count attempt reads the arithmetic lesson and repairs drafts using fr.
One draft passes fr but later fails the required package check because unused simplification arguments produce warnings.
Luna removes them and delivers its patch. A tiny unrelated identity example reproduces that mismatch:
`simp only [keepModel, Bool.not_not]` proves the identity, but the unused `Bool.not_not` fails package verification.
The new checker repair rejects warnings in the submitted draft before saving; hosted validation is pending.

Another unrelated probe confirms that Lean accepts a local lemma such as
`have nonnegative (a b : Int) ... := by ...`, while the measured fr reader rejects its parameters.
The parser repair adds named, anonymous and expression examples, plus checks that the next declaration remains separate.
It must pass hosted generation and native tests before use in a new comparison.

The next root skill links arithmetic directly and starts by checking `refine ?_`, an unfinished first draft.
The lesson adds `exact?`, Lean's search for a fact that proves the current goal, using a generic multiplication example.
All four lesson blocks compile with Lean 4.28.0. The root is 816 bytes and the largest reading path is 7,145 bytes,
within the unchanged ceilings. These later changes have not yet been measured by agents.
No evaluated task's reference proof is supplied, and the 150-second limit stays fixed.

For pairs accepted in both tool setups, complete proof-writing time compares as follows:

| Agent | Matching pairs | Ordinary tools | With fr | Change |
| --- | ---: | ---: | ---: | ---: |
| Luna Fast | 6 | 45.6 s | 45.1 s | -1.1% |
| GLM Flash | 4 | 42.3 s | 60.8 s | +43.8% |
| Kimi K3 | 4 | 55.1 s | 70.6 s | +28.2% |

The baseline is the same agent doing the same task with ordinary tools. Times include replies, tools, checking and delivery.
These are not program-runtime measurements or isolated fr overhead. Failed attempts remain in the full report's totals;
the small matched samples establish no general savings, and actual account charges remain unknown.

## Previous comparison: showing assumptions and conclusions

In the preceding [claim-view comparison](claim-view-results/summary.json), GLM and Kimi each reached one fr page-count proof check.
Their other two fr-available attempts used ordinary Lean builds instead.
All four timed out without a patch. Reaching the checker is progress within an attempt;
it does not establish that either agent can finish this proof.

The earlier investigation below covers 72 OpenCode2 attempts: the claim-view comparison and its two predecessors.
It compares recorded commands, returned tool results, timestamps and reported token counts.
The [claim-view summary](claim-view-results/summary.json), [earlier low-setting summary](low-reasoning-results/summary.json)
and [high-setting summary](introduction-results/summary.json) retain attempt identifiers and record hashes.
Raw transcripts, private drafts, source archives and account data remain local.
No attempt was rerun or reclassified for this review.

## After adding the claim view and mapped errors

All four page-count attempts below had fr available. The limit remained 150 seconds.
Completed tool time is the union of recorded tool intervals, including reads and edits.
It is not isolated CPU time or fr overhead.

| Agent and repetition | Completed tool time | First proof-check command | What happened |
| --- | ---: | --- | --- |
| GLM 1 | 17.5 s | fr at 121.3 s | Wrote an early draft, searched, then checked a replacement; Lean rejected an incorrect assumption split |
| GLM 2 | 23.7 s | Lake at 128.2 s | Edited the theorem and searched the library; no fr command was observed |
| Kimi 1 | 8.4 s | Lake at 127.8 s | First edit began at 116.2 s; no fr command was observed |
| Kimi 2 | 12.9 s | fr at 143.8 s | Requested the claim view at 40.0 s, searched, then checked a draft near the deadline |

All four read the root skill. GLM 1 also read its Lean reference.
Only Kimi 2 requested the new claim view; it clipped the response with `head`.
None used fr's library-signature lookup in these four attempts.
The new view and lookup therefore cannot be credited with a successful delivered proof.

GLM 1's error is concrete: it treats an assumption joined by “and” as an “or”.
Lean expects the conjunction constructor `mk`, but the draft selects alternatives `inl` and `inr`.
The repaired diagnostic points to line 11 of the draft, rather than line 31 of the generated checking file.
GLM also repeats `rg -rn`, which replaces matched text and damages the names it is searching for.

Kimi 1 reads real division facts early but waits until 116.2 seconds to edit.
Its build reports ineffective simplification and two unresolved multiplication-lemma names.
Kimi 2 does inspect the claim view, but its first fr check starts with about six seconds left.
These are different failures; a shared claim that the library was inaccessible would be wrong.

Read-only exports of all eight current OpenCode2 page-count sessions confirm the selected model and low setting.
Their saved responses contain substantial reasoning output before some late checks.
For example, the largest recorded reasoning parts in the two fr repetitions contain 8,899 and 5,093 characters
for GLM, and 10,341 and 7,869 for Kimi. The text stays private.
Character counts are not token totals or charges. Saved response timing can overlap tool timing;
it does not establish continuous computation or isolate provider waiting.
The original timeouts and incomplete per-attempt accounting remain unchanged.

## The simpler tasks also expose workflow mistakes

GLM's first fr-available access attempt runs `fr check --json` through a pipeline that returns no output.
Its fallback never runs because `head` supplies the pipeline's successful exit status.
The delivered patch receives no fr contribution credit.
Its second access attempt and first range attempt use no fr command either.

GLM's second range attempt does use fr, but guesses two wrong targets before reaching the correct checker.
It rewrites Boolean comparisons without converting them into propositions that `omega` can use.
It then tries scratch examples and runs out of time without a patch.
For example, `decide (offset ≤ total) = true` needs conversion before it supplies the arithmetic fact `offset ≤ total`.

Kimi's second access attempt also delivers a patch without an observed fr command.
Its first access attempt and both range attempts have useful fr checks or writes.
Available tools, observed commands and a useful contribution are separate facts in the summary.

The timing baseline is the same agent using ordinary tools on the same task.
Across pairs accepted in both setups, complete proof-delivery time changes as follows:

| Agent | Matching pairs | Ordinary mean | fr-available mean | Change |
| --- | ---: | ---: | ---: | ---: |
| Luna Fast | 6 | 40.1 s | 53.1 s | +32.5% |
| GLM Flash | 3 | 42.4 s | 42.1 s | -0.7% |
| Kimi K3 | 4 | 65.7 s | 71.3 s | +8.6% |

The three matching GLM fr-available attempts made no useful fr step; their small timing difference
does not establish a fr speedup. The table retains them rather than selecting only helpful uses.
Times include startup, replies, tools, checks and delivery. They measure proof-authoring time,
not execution speed of the Rust programs or isolated fr overhead. These small samples do not
establish general savings; account charges remain unknown.

After this comparison, the root skill gained the exact proof-check command and saving instructions.
A new [arithmetic guide](../../../skills/fr/references/lean-arithmetic.md) teaches Boolean comparisons,
“and” versus “or”, and library arguments that are proofs. Its three examples pass pinned Lean 4.28.0.
The changed instructions pass the hosted proof/client checks and M1/M2 workflows.
The recorded comparison does not measure these later changes.

## Earlier comparisons before the claim view

Neither agent reached a proof check in any of the eight fr page-count attempts across the earlier
low-setting and high-setting comparisons. The following sections retain that earlier investigation.

## The earlier low-setting page-count attempts with fr

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

## What these earlier failures required

The draft-location and retained-goal fixes address failures after proof checking begins.
These earlier attempts needed help reaching that first check too.
Repeating their setup or increasing the deadline would have left that problem unresolved.

The resulting work exposes claim parts and declared library types without supplying an evaluator's proof.
An agent can inspect a fact's required arguments, try a draft and read the remaining goal before another broad search.
The success measure remains an early useful check and a correct delivered proof.

`proof-task` now exposes each claim part's assumptions and conclusions, replacing empty proof templates.
The proposition-grouping, mapped-error and library-signature changes passed the hosted controls before the current comparison.
The results at the top of this review show the remaining gap: late checks, proof mistakes and skipped fr steps.

Before claiming a fix, distinguish these questions:

1. Did the agent check a draft early enough to repair it and deliver a patch?
2. Did it use actual declaration names and argument types from returned results?
3. Did Lean accept the unchanged claim in the independent receiver?
4. Where did elapsed time remain unaccounted for by completed tool calls?

Keep the selected models, fixed claims and 150-second limit.
Any new comparison must retain failures and use newly frozen inputs after a concrete repair passes its checks.
