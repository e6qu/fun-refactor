# Build an efficient code tool for AI agents

`fr` should help an AI coding agent understand an unfamiliar codebase, find the right change,
make it, and check or prove the requested result. It should work through the CLI with agents
such as Claude Sonnet and OpenAI Luna, without knowing which repository it will encounter.

The agent should be able to start with a small code map, ask about relevant functions and callers,
and read exact source only where needed. This is **progressive disclosure**: reveal more detail
when it answers a question, rather than load the whole project into the conversation. A second
agent should receive the specific question and enough checked context to answer it, without
repeating the first agent's entire investigation.

Success means correct work with less total agent effort, context and cost. A shorter tool response,
more supported commands, or more passing fixtures does not by itself demonstrate that success.
The [product review](docs/product-review.md) assesses what to keep, what to challenge and how to
measure it. It is the starting point for the next substantial implementation change.

## Where we are

The CLI can locate declarations and callers, and return bounded structured views and source excerpts.
It can preview edits, run declared checks, undo and redo edits, and export patches. It has resumable
task records and Lean proof support for declared subsets. Analysis and editing support vary by
language; parsing a file does not mean `fr` understands every possible behavior in it.
Run `fr --json audit` and `fr capabilities` for the installed tool's current support and limits.

The [generated status](docs/roadmap-status.md) records **7 of 18 technical acceptance items
demonstrated in their stated test cases**. Eleven remain open, and none of the four milestones
below is complete. These counts are not a percentage of product readiness or effort remaining.

We have not established a general efficiency advantage. In the retained two-task Rust comparison,
the `fr` agent used more calls, context and time on both tasks.
It completed an allocation requirement the ordinary-file agent missed. The [evaluation results](docs/evaluations.md#unknown-target-investigations)
include that failure. We need repeated comparisons with current models and unfamiliar tasks.

The recent boltons and more-itertools tests exercise known, scripted edits, recovery after an
interruption, behavior checks and patch replay. They are useful regression tests. They do not
show that an agent can independently diagnose arbitrary projects or that `fr` saves tokens.

## Current chunk: compare a reviewed packaging change with and without fr

The next deliverable is a paired code-change pilot on the packaging candidate: Kimi and GLM,
each with ordinary tools and with fr additionally available. First finish independent review of
the task, reference and grader. Then freeze all four attempts before any change calls. Keep public
check feedback identical, grade exact submissions on GitHub, and retain every failure and fr fallback.
Report correctness, actual fr use, calls, context, elapsed time and unknown usage separately.

Live execution is gated on the client resource diagnosis. The previous configured-client collection
stopped when aggregate memory crossed its unchanged 768-MiB capture limit after 4.43 seconds.
Two successful calls had already reached about 746–752 MiB. Retained measurements do not identify
which process caused the peak. Use public pinned client source and model-free hosted controls to
test a concrete remedy before considering another guarded local collection. No budget increases,
credential extraction, resumed stopped cells or local builds are part of this work.

The [hosted memory experiment](tests/agent-eval/opencode/memory/2026-10-06/README.md)
rejected `BUN_OPTIONS=--smol`: Linux captures completed at 681–690 MiB, above the
640-MiB admission target, and all macOS captures hit the 768-MiB cap. Local calls
remain blocked. Next locate the peak by process and capture phase on GitHub before
testing another remedy. Successful evidence validation is separate from memory admission.

PR #445 merged at `948e11ca` with all final checks passing. The next branch adds
process RSS and capture-stage attribution to the hosted diagnostic. The same samples
must reproduce the guard's peak RSS and cumulative CPU, including exited workers.
Stages are inferred from artifact creation; samples spanning stages stay ambiguous.
No new remedy or live collection is admitted by this instrumentation alone.

The [first profiles](tests/agent-eval/opencode/memory/2026-10-07-profile/README.md)
identify OpenCode as the largest process at all twelve peaks. On macOS, the client
alone uses 658–673 MiB, above the entire admission target. The next hosted experiment
tests a per-process catalog override with the explicitly configured scripted model.
It must preserve source delivery and terminal answers. Real-provider compatibility
and local headroom remain separate prerequisites, even if the scripted experiment passes.

The [complete review inputs](tests/agent-eval/opencode/reviews/2026-10-06-packaging-inputs/README.md)
are prepared with all eight grader cases and a requirement coverage checklist.
Both reviews remain unfrozen and unstarted. The four-attempt comparison will give
both arms the same public test and baseline failure, with GitHub grading afterward.
It will not provide tests during the agent attempt.

GitHub uses the existing `gh` login; OpenCode resolves its own configured provider access.
The packaging review must cover intersection fallback, explicit policies, bounds and exclusions,
original objects and one-shot inputs, and unchanged comparison APIs. Record any review limitation
before accepting the task. A pilot remains blocked if resource or review prerequisites fail.
No fr call is required: an agent choosing ordinary tools is an outcome to measure.

## Recent work: configured reviews and a verified duplicate-identity gap

PR #444 merged as `a5052841` after all 21 checks passed. Three source reviews completed;
one hit the memory limit and two cells remain unstarted. Both models found no contradiction
for one disabled-interpolation input. Kimi flagged an identity assertion that checked each result
against any input object, rather than its corresponding input object.

GitHub verified a related counterexample using explicitly distinct equal `Version` objects.
The incorrect repair passes all seven old cases and the public example but fails the new case.
The unchanged reference passes all eight current cases. The model's string-literal premise remains
unverified. The [retained comparison](tests/agent-eval/opencode/reviews/2026-10-06-configured/README.md)
keeps that distinction, the resource failure and incomplete usage accounting. None of the reviews
used fr. Whole-task acceptance and an efficiency advantage remain open. PR #443 released v0.54.0.

## Recent work: return useful discovery pages within the response budget

Before PR #441, `project explore` could refuse every result when a long scope and matching declarations
exceed its serialized response limit. Narrowing to one file does not always help. A small generated
fixture reproduces this failure with the released CLI.

PR #441 reduces page sizes to fit the existing compact or expanded budget. Names retain
complete rows and a continuation; behavior pages retain exact source slices and relationship rows.
The budget includes final context metadata and the output newline. Explicit expansion keeps the
current names-page position. Hosted tests must reconstruct every match, source byte and relationship,
check UTF-8 boundaries, and refuse stale continuations. A page with oversized mandatory metadata
still refuses rather than returning an incomplete row or a cursor that makes no progress.

This addresses a concrete disclosure failure. It does not establish token savings or a general
agent-efficiency advantage. All ten hosted pagination tests pass. The refreshed index comparison
reproduces all eight baseline samples; repository delivery passes 365 behavior cases; exact source
locations pass their nine recorded cases. Independent audits verify all three bundles against current
source. The roadmap remains at seven demonstrated technical items, with all four milestones open.
All 21 PR checks passed before merge at `183887b5`. Local main also includes release PR #439.

## Recent work: review source and finish with one checked answer

PR #438 merged at `25da0e17` after all 21 checks passed. It established the scripted submission
controls below. PR #440 merged at `8075e69e` and connects that answer to a frozen question and source packet.
It checks that citations came from the supplied packet or an earlier source read. It also retains
observed work when the client stops without exporting its session.

`tools/terminal-reviews.py` freezes, collects and replays these reviews. The audit refuses changed
plans, source, models, tool results and resource limits. Two consecutive failures stop the collection;
existing cells cannot be retried. The historical runners and stopped collections are unchanged.

All 21 PR checks passed. The [hosted client check](https://github.com/e6qu/fun-refactor/actions/runs/37376273874)
passed in 68 seconds, including all 24 offline controls and fr authoring of the extracted function.
Independent replay accepts packet-only review and an additional source read. It retains the expected
missing-answer, duplicate-answer and interrupted outcomes as failures, including their observed work.
At that stage, these were scripted checks only. PR #444 subsequently added configured-client access
and retained live Kimi/GLM reviews. The original explicit-endpoint plans remain replayable. See the [review commands](docs/opencode-native-tools.md#review-source-packets-with-terminal-answers).

This advances the evaluation runner. It does not establish whole-task acceptance, model reliability,
fr adoption, complete billing or a general efficiency advantage. The roadmap stays at 7 of 18
demonstrated technical items, with all four milestones open.

## Recent work: check native submissions with scripted responses

The last live review collection stopped after a missing submission and repeated submissions.
The new scripted check drives OpenCode 1.18.34 with a local fake provider. It exercises one terminal
answer, an omitted answer, two answers, and another tool call alongside an answer.
The audit compares the request, events, independent session export, MCP log and provider requests.
It counts every response and rejects mismatched evidence. No live model calls are part of this check.

The [scripted GitHub check](https://github.com/e6qu/fun-refactor/actions/runs/37338076823)
passed all four cases in 43 seconds on `c35a99c2`. Independent replay passes with the current audit.
Each case made two provider requests, including failures. Twelve offline tests check the audit.
Existing native runners and stopped collections retain their original rules and outcomes.
The current chunk connects this protocol to source review; live model reliability remains open.
Task acceptance, fr adoption, provider billing and general efficiency remain unproven.
See [the scripted submission check](docs/opencode-native-tools.md#check-terminal-submissions-with-scripted-responses).

## Recent work: finish the repository audit within its deadline

PR #435 passed its 20 PR checks, but its post-merge recipe replay exceeded the
15-minute deep-audit job deadline. The replay covers the entire workspace. Each
unchanged recipe step rebuilt the index twice, and the test built another unused index.

This change skips speculative indexing for empty selections and retains the current
index after an unchanged step. Real edits still refresh it. Regression cases cover
zero limits, renamed callers, changed-file selectors and permitted refusals. Dogfooding
with a single-file root also exposed blank patch filenames; previews now name that file.

GitHub refreshed repository delivery against `c272d5fb`: both tasks pass 365 behavior cases.
Recovery against `ededba2a` passes 430 handled-failure and 430 process-exit boundaries,
plus 224 model cases. Eight refreshed index samples reproduce the pinned baseline's complete
symbol/reference answers. Their retained source bindings match the current tree. The complete
deep run passed all 17 jobs. The recipe test fell from 12m57s to 5m04s in these two runner samples.
Its whole job fell from 14m47s to 6m52s, with coverage and the deadline unchanged. This is not a
general performance benchmark. PR #436 merged after all 20 final checks passed; its post-merge
workflows passed too. Stopped model collections remain stopped.

## Recent work: review concrete assertions and expose the remaining gaps

PR #430 preserved eleven historical reports in lossless archives, saving 366.6 MiB per expanded
checkout. Its 20 final checks passed. Current acceptance evidence remains unchanged.

The [reference-repair reviews](tests/agent-eval/opencode/reviews/2026-10-05-references/README.md)
finished three narrow Kimi reviews and retained three GLM timeouts. Two reviews found no scoped
contradiction. The packaging finding was incorrect: it confused version ordering with specifier
membership. A separate rejection binds that finding to exact source and successful GitHub behavior
evidence. The grader and reference remain unchanged. The linked table names each remaining review gap.

One attempted fr call omitted the handle required by behavior mode, then fell back to ordinary reads.
The new opt-in native adapter returns a ready names query after this refusal. Scripted tests check
scope preservation, replay and source accounting; no live benefit is claimed. The six recorded calls
used the old adapter. Earlier stopped review collections stay stopped, and their limits stay fixed.

Full independent task and grader review remains open. The roadmap still has seven of eighteen
demonstrated items, with all four milestones open. No general efficiency advantage is established.

PR #431 merged after all 20 checks passed; its post-merge workflows also passed. The subsequent
[boundary collection](tests/agent-eval/opencode/reviews/2026-10-05-boundaries/README.md) used the
recovery-capable adapter. Both dotenv reviews timed out, stopping four remaining calls. No review
gap closed. The sole fr call used a nonexistent path, so it did not exercise missing-handle recovery.

A separate source audit found repeated reads of identical files under different paths.
It adds 4,096 bytes for Kimi and 8,214 bytes for GLM beyond the existing same-path counters. The original reports remain
unchanged. These are observed source bytes, not measured token savings or a general efficiency claim.

The [single-assertion collection](tests/agent-eval/opencode/reviews/2026-10-05-assertions/README.md)
was frozen at `c938151b` before calls. Packets identify selected changed and unchanged files and
include one scoped grader assertion. Kimi and GLM completed the two-object identity review; Kimi
also completed the rejected-directory review. All three found no scoped contradiction.
GLM then answered as text without submitting; Kimi's next attempt submitted four times.
Those two protocol failures stopped the final cell. All five attempts finished within budget.
There were no fr calls, so neither tool adoption nor an efficiency advantage is established.

The [requirement review table](docs/candidate-review-status.md) names the completed review and gaps
for each requirement family. Two assertions now have completed review; whole-task acceptance stays
open. Before more live calls, address reliable native submission using scripted checks and a new
frozen design. This collection and every earlier stopped collection remain stopped.

## Evidence and remaining gaps

| Evidence | What it establishes | What is still missing |
|---|---|---|
| [Native source-reading pilot](docs/native-read-outcomes.md) | Two passes and four timeouts across six attempts; records observed work from timeouts | No successful ordinary/fr comparison pair; complete provider context and billing unknown |
| [Native code-change trials](docs/native-change-outcomes.md) | Ten behavior passes and two timeouts across twelve attempts; GitHub graded exact submissions | No attempt called fr; no evidence that its edit route helped |
| [Candidate task controls](tests/agent-eval/opencode/candidates/README.md) | Three new projects, with unchanged, reference and incomplete-repair controls checked on GitHub | Independent grader review and live comparisons remain open |
| [Source-based review attempts](tests/agent-eval/opencode/reviews/2026-10-04-native/README.md) | Two timeouts, 17 tool calls, no completed findings or fr use | Review scope did not fit the budget; the frozen stop rule prevented four remaining calls |
| [Narrow source-packet reviews](tests/agent-eval/opencode/reviews/2026-10-04-packets/README.md) | One GitHub-verified counterexample, two timeouts and three stopped calls | Broader independent review remains open |
| [Historical evidence archives](tests/agent-eval/EVIDENCE-ARCHIVES.md) | Eleven reports restore byte for byte; expanded checkout saves 366.6 MiB | Git history and agent-efficiency measurements remain unchanged |
| [Reference-repair reviews](tests/agent-eval/opencode/reviews/2026-10-05-references/README.md) | Three completed narrow reviews, three timeouts, one incorrect finding rejected against execution | Full task review, valid packaging assessment and live recovery-hint usefulness remain open |
| [Boundary reviews and source reuse](tests/agent-eval/opencode/reviews/2026-10-05-boundaries/README.md) | Two timeouts, four stopped calls; exact-file overlap audit preserves repeated reads across paths | No completed review; smaller questions and clearer unchanged-file context need a new frozen design |
| [Single-assertion reviews](tests/agent-eval/opencode/reviews/2026-10-05-assertions/README.md) | Three completed reviews cover two assertions; exact before/after identity context; two retained submission failures | Reliable native submission and the remaining requirement families; no fr adoption or efficiency evidence |
| [Public test feedback](docs/opencode-native-tools.md#run-public-checks-before-submitting) | Scripted repair, stale-result and container-boundary checks | No live-agent adoption or efficiency benefit established |
| [Technical acceptance](docs/roadmap-status.md) | Seven of 18 items demonstrated; GitHub refreshed the source-bound location and repository evidence | Eleven items and all four milestones remain open |

Earlier native discovery and citation experiments remain in the
[native evaluation guide](docs/opencode-native-tools.md). Keep their failures and the
[older Rust comparison](docs/evaluations.md#unknown-target-investigations), where fr increased
calls, context and time. Smaller source pages alone do not demonstrate lower total agent cost.

Next steps, in order:

1. Complete independent task and grader review with questions that fit the existing budget.
   The stopped review collections remain stopped. A different review requires a new frozen plan;
   do not increase limits or retry until a review happens to pass.
   Single-assertion packets with unchanged-file context produced three completed reviews, then two
   submission failures. Scripted terminal submission checks now pass. Check the
   [explicit gaps](docs/candidate-review-status.md). Address the recorded client memory failure
   before new local calls. Use existing OpenCode access and a new frozen design; stopped cells
   remain stopped. Finish the packaging task review before freezing its four change attempts.
2. Compare ordinary tools with the public fr read/edit routes and test feedback on those unfamiliar
   tasks. Freeze requirements, allowed tools, models, budgets and private checks before calls.
   Use the configured Kimi/GLM OpenCode profiles for bounded local rehearsals and GitHub for grading.
   The planned Sonnet/Luna study still needs independent tasks and complete cost accounting.
3. Compare a single agent with narrowly delegated investigation and review. Count parent and child
   usage, failed children, repeated reads, handoff context and integration effort.
4. Measure guide, intent and task layers individually. Consolidate routes that add instructions or
   round trips without improving outcomes. Follow the
   [removal review](docs/product-review.md#removal-decisions) for UI, migration and evidence storage.
5. Use observed task failures to choose reusable analysis improvements. The next candidate is
   Python value tracing through assignments, branches and helpers, with explicit uncertainty.

Keep ordinary tools available in every arm and retain failed attempts. Measure all tool output,
including fr metadata and delegation, alongside correctness. Production must not recognize a
benchmark repository or encode its expected repair.

## Generalization requirements

- Production analysis, editing and caching must not branch on benchmark repository names, paths,
  revisions or expected patches. A user may configure which inputs and uses matter; those rules
  cannot skip language semantics or force a desired answer.
- Specify a language feature and its limits, then test it in several independently written forms.
  Rename a helper or move a supported module, update its references, and check that the answer stays the same.
- Before implementing repository-scale analysis, select at least three independent projects and
  keep one out of implementation work. Freeze the analyzer before that project's first evaluation.
  Preserve failures; after using one to improve the implementation, choose a new held-out project.
- If the analyzer encounters unsupported behavior or reaches a resource limit, report it. A search
  that did not finish cannot establish that a value never reaches a use.
- Keep the old scripted examples reproducible. Their success cannot close the new generalization
  or agent-efficiency requirements.

## Milestones and implementation checklist

Checkboxes track implementation only. The [status report](docs/roadmap-status.md) lists the behavior
each milestone must demonstrate, its supporting tests and what is missing. The catalog is now
`agent-analysis-v2`: open requirements include unfamiliar projects and complete agent-cost accounting;
the seven existing demonstrated items retain their original, limited scope.

### A. Find relevant code and explain the evidence

Example: given a failing test, help the agent locate the responsible code and distinguish its
hypothesis from facts established by the tool or a compiler.

- [x] Report exact locations for declarations, references and calls, including repeated calls on one line.
  See [investigation contracts](docs/agent-investigations.md).
- [x] Link analyzed expressions back to source, including missing or multiple source locations.
  See [source-location tests](tests/semantic_evidence.rs).
- [x] Explain analyzed facts with their inputs, assumptions and omitted information.
  See [Python fact reports](src/project/flow_facts.rs).
- [x] Retain declared compiler invocations, build inputs, toolchain identity and disagreements.
  See [compiler checks](docs/project-checks.md#retained-compiler-diagnostics).
- [x] Save task requirements, hypotheses, observations, dependencies and required checks.
  See [task plans](sdk/python/src/fr_ir/investigation.py).
- [x] Run an initial bug and feature comparison without supplying edit locations to the agent.
  See the [original four trials](tests/agent-eval/unknown-target/cohort.json).

Still needed: repeated agent comparisons with full cost accounting and a consolidated statement
of which Rust/Python facts the tool can establish. Existing test scripts and the small older-model
comparison do not establish general autonomous problem solving.

### B. Explain how values move through code

Example: determine whether a value supplied to a function can reach an output after a helper call.
Check whether an earlier assignment overwrites it. Report possible paths separately from runtime-proven paths.

- [ ] Specify the reusable language rules connecting source, analysis and edits, with independent
  examples and explicit unsupported cases.
- [x] Represent assignments, branches, loops, returns and supported explicit exceptions.
  See [Python control flow](src/project/control_flow.rs) and [runtime comparisons](tests/flow_fixed_point.rs).
- [x] Summarize supported argument, return and effect transfers across functions, including recursion.
  See [function summaries](src/project/flow_summaries.rs).
- [x] Allow explicit input, output and sanitizer rules; return paths with exact locations and limits.
  See [dataflow](src/project/dataflow.rs).
- [x] Distinguish candidate call relationships, possible value transfers and proved statements.
  See the [analysis contract](docs/agent-investigations.md).

Still needed: generalization across independently selected projects, useful positive and negative
answers, and honest handling of shared mutable objects and implicit exceptions. The two recent
repository delivery tasks still report incomplete or refused flow analysis.

### C. Resume work and reuse correct answers

Example: another agent changes a helper while work is paused. On resumption, discard conclusions
that depended on that helper, keep unrelated findings, and require a fresh edit review.

- [x] Distinguish current code handles, saved content identities and possible matches across revisions.
  See [target sessions](sdk/python/src/fr_ir/investigation_session.py).
- [x] Record supported source, import, configuration and analyzer dependencies, including missing names.
  See [module dependencies](src/project/flow_modules.rs).
- [x] Store and verify reusable analysis records, rebuilding when dependency coverage is insufficient.
  See [flow caching](sdk/python/src/fr_ir/flow.py).
- [x] Persist tasks and invalidate affected steps before resuming.
  See [resumption tests](tests/investigation.rs).
- [x] Tie check and proof results to their inputs, rejecting old results after relevant changes.
  See [retained proofs](src/spec/retained.rs).
- [x] Measure first-run, cached and edited analysis on a small synthetic corpus.
  See [cache measurements](docs/evaluations.md#current-matched-results).

Still needed: project-scale change tests and measurements of time, memory and disk on unfamiliar
repositories. Small synthetic timing improvements do not demonstrate cheaper agent tasks.

### D. Make checked changes and useful proofs

Example: change a function's interface and its callers, run the relevant behavior tests, and produce
a patch another checkout can apply. For a proof request, connect the theorem to the actual code.

- [ ] Discover all relevant callers, contracts, configuration and tests for the declared change.
  [Caller discovery](docs/change-scopes.md) and [structural changes](docs/structural-changes.md) cover
  specific cases; dynamic dispatch and inferred contracts remain incomplete.
- [ ] Extend analysis and editing representations for reusable language features required by observed
  tasks, with defined success, failure and side effects.
- [ ] Translate a function under explicit arithmetic, error and evaluation-order rules, independently
  checking its source and target behavior.
- [ ] Connect a property and proof to an existing implementation, and write a new implementation
  together with its specification and proof.
- [ ] Retain old and new proof inputs for changes and translations; reject outdated proof claims.
  [Model comparisons](docs/model-comparisons.md) cover small Boolean models; broader source proofs remain open.
- [x] Check small decision models for edit admission, task transitions and interrupted recovery.
  See [Lean checks](docs/lean-specs.md) and [host recovery](docs/host-recovery.md).

Still needed: broader structural changes, a useful numeric translation, and checked connections
between source implementations and proofs. Passing a theorem about a separate model is insufficient.

## Working rules

The [study planner and auditor](docs/agent-study.md) now freeze independent tasks and compare retained
parent/child accounting. The host gateway now reserves and settles provider requests; a container
grader compares submitted code with private cases. A serial agent loop now connects those steps for
fix and feature tasks, with pinned source exports and bounded child investigations.
Both study arms now have bounded source pages with content hashes and continuation offsets. The
auditor counts overlapping reads across agents and checks delivered-byte counters against the trace.
Reads inside arbitrary commands remain unknown; partial page counts do not establish total savings.
An optional frozen cgroup budget now covers all agent and grader containers in an attempt, including
child investigations. It records kernel CPU and memory counters and fails the attempt on a resource
stop. Host/daemon resources, disk/cache growth and integration context still need measurement.
These tests use fake providers and do not count as live agent trials.
Select independent tasks, review graders and complete resource/context measurements before the pilot.
The [local OpenCode rehearsal](docs/opencode-rehearsal.md) uses configured Kimi K3 and GLM 5.3 Flash
profiles for small protocol and task checks under the workstation guard. Its synthetic tasks,
restricted discovery actions and CLI usage records do not close independent-study acceptance items.
The [explanation adapter](docs/opencode-source-evidence.md) checks finite factual claims and retrieved
source citations on three pinned upstream Python repositories. It measures repeated source bytes
across ordinary and fr reads. Full context, source-connected proofs and general efficiency remain open.

Keep reports small and allow explicit follow-up reads. Preserve uncertainty, exact source references
and stale-result rejection. Check behavior independently of the proposed patch. Review edits before
applying them, preserve unrelated work, and retain undo and recovery. Use `fr` itself when its public
operations fit the change, recording limitations when they do not.

Do full builds, test gates and large evidence regeneration on GitHub runners. Local checks and `fr`
invocations on this workstation use the resource guard described in the [development guide](docs/development.md).
Keep CI jobs within 15 minutes by dividing work without dropping test coverage.

The [catalog](tests/agent-eval/roadmap.json) holds the detailed acceptance conditions and evidence links.
`tools/roadmap-status.py --check` checks that the generated report matches it. It checks recorded
identities and assertions; the independent behavior tests must also pass. Update a milestone only
when its implementation and demonstrated outcomes are both complete.
