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
measure it. Use those criteria when expanding or removing features.

## Where we are

The CLI can locate declarations and callers, and return bounded structured views and source excerpts.
It can preview edits, run declared checks, undo and redo edits, and export patches. It has resumable
task records and Lean proof support for declared subsets. Analysis and editing support vary by
language; parsing a file does not mean `fr` understands every possible behavior in it.
Run `fr --json audit` and `fr capabilities` for the installed tool's current support and limits.

The [generated status](docs/roadmap-status.md) records **8 of 18 technical acceptance items
demonstrated in their stated test cases**. Ten remain open, and none of the four milestones
below is complete. These counts are not a percentage of product readiness or effort remaining.

We have not established a general efficiency advantage. In the retained two-task Rust comparison,
the `fr` agent used more calls, context and time on both tasks.
It completed an allocation requirement the ordinary-file agent missed. The [evaluation results](docs/evaluations.md#unknown-target-investigations)
include that failure. We need repeated comparisons with current models and unfamiliar tasks.

The recent boltons and more-itertools tests exercise known, scripted edits, recovery after an
interruption, behavior checks and patch replay. They are useful regression tests. They do not
show that an agent can independently diagnose arbitrary projects or that `fr` saves tokens.

## Latest measured result

The [Rust/Python explanation profile](docs/compiler-profile.md) checks eleven declared features
with eighteen original and renamed/relocated cases, plus the existing compiler and checked-origin
collectors. It retains exact source locations, build inputs, toolchain identities, runtime outcomes
and explicit unsupported results. Its 26 corruption tests reject missing cases and false claims.

The review exposed a real selection bug: choosing the second same-named Python function returned
the first body's model. Semantic queries and edit preparation now refuse ambiguous declarations
in the source index or lowered model, including duplicate containing classes. Tests also preserve
valid selections of same-named methods in distinct classes. This is a general matching rule.

The profile demonstrates A.compiler-profile for its listed features. It does not establish complete
language semantics, runtime dispatch, agent efficiency or source-connected proofs.
The [source-repair](docs/source-repair.md) and [resume](docs/workflow-recovery.md) comparisons
remain workflow regression gates with their stated limits.

## Next large chunk

Specify reusable Python behavior, acceptance item
[B.contract](docs/roadmap-status.md#bcontract-specify-reusable-python-behavior).
An agent needs to know which language rules justify an answer and when to stop trusting it.

1. Document evaluation order, assignments, calls, exceptions and source locations as language
   rules. Distinguish supported behavior from normalization and unsupported constructs.
2. Compare each supported rule with independently authored executable examples and renamed
   or relocated equivalents. Include positive, negative and explicit unsupported outcomes.
3. Fix exposed defects through reusable rules and regressions. Repository identities, fixture
   names and expected answers must never select semantics.

Keep repository and recovery gates passing. The independent live study still needs representative
resource admission, actual fr-use accounting and complete parent/child costs. The
[closed client investigation](tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md)
did not justify a configuration fix. Preserve the stopped packaging collection and failed
workstation admission; neither may be resumed or replaced by a short control.

## Remaining product gaps

| Gap | What must be demonstrated |
| --- | --- |
| Agent efficiency | Correct changes on unfamiliar tasks with complete context, time and provider-cost accounting |
| Actual fr adoption | Measure tool use separately from availability; ordinary-only success in an fr-enabled arm is not evidence of benefit |
| Delegation | Count parent/child usage, failed children, repeated reads, handoffs and integration effort |
| Useful analysis | General value tracing through assignments, branches and helpers, with explicit unsupported cases |
| Simpler workflows | Measure guide, intent and task routes separately; consolidate overhead without losing correctness or recovery |
| Source-connected proofs | Check a useful property against the implementation, including stale-proof rejection after changes |

The proposed Sonnet/Luna study still needs independent tasks and complete cost accounting.
The [product review](docs/product-review.md) owns evaluation and removal criteria. Production
analysis must not recognize benchmark repositories or encode their expected repairs.

## Evidence and remaining gaps

| Evidence | Established result | Limit |
| --- | --- | --- |
| [Source repair and ordered patches](docs/source-repair.md) | Forty cells cover actual wrong bodies, whole-chain reversal and original-to-final receiver replay; undo/correct uses fewer calls | Prescribed edits; ordered patches retain intermediate source; no atomic composition or live-agent comparison |
| [Checked recovery delivery](docs/workflow-recovery.md) | Reviewed resume and undo-and-retry through checks, delivery and receiver replay | External prerequisite repair; no combined source-repair patch or live-agent comparison |
| [Guided body preparation](docs/workflow-routes.md#preparing-body-edits-and-recovering-failed-checks) | 595 fewer caller/submission bytes in four fixtures; 40 cells cover checked delivery, stale inputs, reopened undo and conflicts | Prescribed bodies; no model/token comparison or repaired-delivery result |
| [Editing routes](docs/workflow-routes.md) | Equal patches/lifecycles across three edit shapes; guided writes remove one redundant process; 23 refusal controls pass | Prescribed tasks; no reduction in complete-program bytes or autonomous efficiency result |
| [Delivery diagnostic](tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md) | 31 captures completed; one repetition stayed unstarted after failed memory headroom | Inconsistent CPU differences; no targeted client fix or workstation admission |
| [Streaming CPU comparison](tests/agent-eval/opencode/changes/streaming-2026-10-09/README.md) | Client dominates sampled CPU; short controls cannot admit representative streaming | Six CPU stops; no macOS admission or general agent-efficiency result |
| [Configured change pilot](tests/agent-eval/opencode/changes/configured-2026-10-08/README.md) | Resource stop retained and independently replayed | One failed capture, three unstarted cells; no behavior or efficiency comparison |
| [Native source-reading report](docs/native-read-outcomes.md) | Two passes and four timeouts, including observed failed work | No successful ordinary/fr pair; billing and complete context remain unknown |
| [Native code-change report](docs/native-change-outcomes.md) | Ten behavior passes and two timeouts; exact submissions graded on GitHub | No attempt used fr |
| [Candidate review status](docs/candidate-review-status.md) | Scoped reviews, verified grader counterexamples and explicit packaging admission | Finite review, failed submissions and incomplete independent task selection |
| [Technical acceptance](docs/roadmap-status.md) | Eight of 18 items demonstrated in their stated cases | Ten items and all four milestones remain open |

Detailed run history belongs in the linked reports, retained fixtures and Git history.
Keep failures visible. Smaller source pages or scripted successes alone do not establish lower
total agent cost. The [restoration guide](tests/agent-eval/EVIDENCE-ARCHIVES.md)
indexes historical storage archives.

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
demonstrated items retain their original, limited scope.

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

Use small reports with explicit follow-up reads, exact source references and stale-result rejection.
Check behavior independently of the proposed patch. Preview edits, preserve unrelated work and
retain undo and recovery. Dogfood `fr` where its public operations fit, and record concrete gaps.

Keep agent scheduling and model access in evaluation hosts. The [study guide](docs/agent-study.md)
defines independent task plans and parent/child accounting; the
[OpenCode guide](docs/opencode-native-tools.md) defines configured-client collection and replay.
Fake-provider controls are protocol tests, not live efficiency evidence.

Full builds, test gates and large evidence regeneration run on GitHub. Local checks and `fr`
use the [workstation guard](docs/development.md#working-on-a-shared-desktop).
Keep CI jobs within 15 minutes without dropping coverage; queue time is separate from execution.

The [acceptance catalog](tests/agent-eval/roadmap.json) owns detailed conditions and evidence links.
`tools/roadmap-status.py --check` verifies the generated report. Independent behavior tests must
also pass. Close a milestone only when both implementation and demonstrated outcomes are complete.
