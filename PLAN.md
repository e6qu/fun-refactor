# Help an agent finish the right change with less work

The goal is straightforward. Give an agent a bug or feature request in a project it has never seen.
Help it find the right code, make the change and check the result. We want fewer
unnecessary reads and retries, lower total cost, and reliable answers about what still needs checking.

For example, you need to fix checkout rounding without changing the public API. The agent
should find the calculation and its callers, explain the mistake and prepare a small patch.
Then it should run checks that catch the reported error. Finding a function name is only one step toward that result.

## Where we are

`fr` already helps an agent find functions and callers, read selected code, preview edits and run
checks. It keeps saved changes so the agent can undo a mistake or resume interrupted work.
It can also trace some simple Python values and prepare proof tasks for small supported functions.

Before preparing a Python rename, check the installed tool with `fr --json audit` and `fr capabilities`.
They report available commands and their limits. After the rename, run the test that checks the user’s problem.

These pieces have useful tests. They do not yet add up to a demonstrated advantage on unfamiliar
work. In the small live comparisons, using `fr` took more calls, reading and time. One attempt
with `fr` met a requirement the ordinary-tool attempt missed. That is a useful result, but too
little evidence to say the tool generally saves work.

The [current status](docs/roadmap-status.md) owns the test counts and the remaining requirements.
All four larger goals below still have unfinished work. A passing test establishes the behavior
it checks; it is not a percentage of product readiness.

## Latest measured result

The recent Python work asks: “Can this input reach this output?” Small examples check arguments,
assignments and helper calls against Python execution. Unused function bodies and plain bindings
such as `metadata = 0` no longer block an otherwise supported answer.
Small literal collections such as `registry = {}` now receive the same treatment.
Reading or changing that registry still needs separate support.
Plain function-name assignments such as `public = relay` can preserve the helper's behavior.
Calls through the new name retain the original function's source locations.
Unused built-in references such as `sorter = sorted` can also coexist with a supported helper.
Calling or reading that stored sorting function still leaves the answer incomplete.

Real libraries expose the remaining gap. Importing a package can execute code that `fr` does not
understand. The current tests in Toolz, funcy and pydash still leave real-package answers incomplete.
Moving the same helper into a small isolated file sometimes produces a complete answer. That
isolated success does not establish that the original package is understood.

The [Python behavior guide](docs/python-contract.md) explains the examples and limits.
The [real-library results](docs/python-repository-flow.md) keep unsuccessful attempts alongside
successful ones. The [product review](docs/product-review.md) explains what we still need to measure.

## How the guides should read

The overview and guides now start from the reader’s question before describing the machinery.
Keep a concrete task, an example and an explanation of what the result lets the user conclude.
Keep exact commands and data fields available for readers who need them. Keep one current format;
reject missing or old data without adding compatibility readers or migration fallbacks.

## Next large chunk

Continue the Python work from the actual library failures. Improve general language rules,
such as how imports and module-level code affect a helper call. Never teach the analyzer the names
of test projects or their expected answers. An unfinished search must still say the answer is unknown.

The next reserved test project is jaraco.functools. Its implementation remains unopened. Keep it
unread while developing the next changes, then freeze the implementation before the first evaluation.
Toolz, funcy and pydash are already familiar development examples. The earlier Boltons reservation
was unsuitable because prior work had already used it. Keep that mistake in the recorded history.

Keep editing, undo, interrupted-work recovery and patch delivery working as this expands.
The stopped packaging experiment stays stopped. A later live study needs suitable independent tasks,
explicit cost limits and complete usage records before it can run. The [closed investigation](tests/agent-eval/opencode/changes/delivery-2026-10-09/README.md)
does not authorize restarting it.

## Remaining product gaps

| User question | What we still need to show |
| --- | --- |
| “Where should I look for this bug?” | Accurate explanations and relevant source lines on unfamiliar tasks, with less wasted reading |
| “Does this input reach the output?” | Useful answers in real projects, including a clear unknown answer when imports or effects exceed support |
| “Did the fix solve my problem?” | Independent behavior checks for the requested change, alongside builds and regression tests |
| “Can another agent pick this up?” | A useful handoff with the question, checked findings and remaining work, without repeating the whole investigation |
| “Can I trust the saved answer after an edit?” | Correct invalidation and renewed checks when the code behind a finding changes |
| “Does fr save time or money?” | Repeated comparisons that count all reading, retries, failed attempts and work by every agent |
| “Does the proof apply to my code?” | A stated property, a checked proof and a checked connection to the actual implementation |

The broader comparison targets Sonnet and Luna. Its plan remains a proposal. Tool availability
alone does not establish use: an agent that fixes a bug without calling `fr` has not shown a benefit
from `fr`. The [review](docs/product-review.md) owns the comparison criteria and decisions about what to simplify.

## Evidence and remaining gaps

Use the [evaluation guide](docs/evaluations.md) to inspect the recorded results and their limits.
It separates tests of prescribed edits from experiments where an agent must discover a solution.
Failures, timeouts and unstarted attempts stay visible. Missing cost information stays unknown.

For example, a script that applies a known repair and replays its patch can check delivery.
It cannot tell us whether an agent would find that repair, or whether the tool makes the search cheaper.
Likewise, a smaller response does not establish a smaller bill for the complete task.

The [restoration guide](tests/agent-eval/EVIDENCE-ARCHIVES.md) explains how to recover historical
records. Detailed run history belongs with those records, not in the current work instructions.

## Generalization requirements

- Test a language rule in several independently written forms. Renaming a helper or moving a supported file must preserve the answer.
- Choose at least three independent projects before building project-scale analysis. Keep one unread during implementation and freeze the analyzer before evaluating it.
- Preserve the first failure on an unfamiliar project. Once its code informs a fix, choose another unread project for the next evaluation.
- Keep production behavior independent of test repository names, revisions and expected patches. User rules can select inputs and outputs, but cannot skip language behavior.
- Report unsupported behavior and exhausted searches. “I could not finish checking” must never become “this cannot happen.”
- Keep previous editing and recovery examples reproducible. Their success alone cannot establish that the tool works generally or reduces agent cost.

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
