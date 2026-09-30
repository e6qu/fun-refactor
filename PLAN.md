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

## Next large chunk

First measure which parts help agents:

1. Extend the existing evaluation runner to compare ordinary tools with the `fr` CLI on the same
   tasks, separately for Sonnet and Luna. Record all parent and child agent costs. Keep tasks,
   model settings, budgets and correctness checks fixed before running them.
2. Compare a single agent with narrowly delegated investigation and review. Share small findings
   with source references; measure repeated reads, handoff costs and conflicts as well as latency.
3. Measure the public guide and edit routes with individual layers disabled. Consolidate routes
   that add instructions and round trips without improving task outcomes. Assess browser UI,
   application migration and retained artifact storage using the [removal review](docs/product-review.md#removal-decisions).
4. Use observed failures to choose general analysis improvements. The next candidate is Python value tracing.
   Determine whether a selected input can reach a selected use through assignments, branches and helpers.
   Show where an answer becomes uncertain.

This ordering replaces the previous instruction to build task-specific flow rules for two named
repositories. The tool should learn reusable language behavior. Projects are tests of that behavior.

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
