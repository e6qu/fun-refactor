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

## What the recent Python work means for a user

The recent Python work asks: “Can this input reach that output?” Small examples compare assignments
and helper calls with Python execution.

Illustration: your helper lives beside these declarations:

```python
metadata = 0
registry = {}
sorter = sorted

def relay(value):
    return value

public = relay
```

Those unused declarations can now coexist with a supported trace through `public`.
The trace points back to `relay`. Using or changing `registry`, or calling the stored `sorter`,
still needs behavior the analyzer does not support. That question remains unfinished.

Real libraries expose the remaining gap. Importing a package can execute code that `fr` does not
understand. The current tests in Toolz, funcy and pydash still leave real-package answers incomplete.
Moving the same helper into a small isolated file sometimes produces a complete answer. That
isolated success does not establish that the original package is understood.

The [Python behavior guide](docs/python-contract.md) explains the examples and limits.
The [real-library results](docs/python-repository-flow.md) keep unsuccessful attempts alongside
successful ones. The [product review](docs/product-review.md) explains what we still need to measure.

## How the guides should read

Start each guide from the reader’s question before describing the machinery.
Show the task, a concrete example, the meaning of the result and the next action.
Introduce concepts where the example first needs them. A reader should not need another manual to understand the lesson.
State required files, installed tools and values returned by earlier commands before using them in an example.
Explain each performance comparison's baseline and included costs; the [measurement guide](docs/benchmarks.md) supplies examples.
Keep exact commands and data fields available for readers who need them. Keep one current format;
reject missing or old data without adding compatibility readers or migration fallbacks.

## Next large chunk

Choose work by a useful question that we can answer from beginning to end.
For an unfinished Python trace, first [explain the recorded import gaps](docs/explain-python-imports.md).
The explanation separates a helper file found but left unread from names the checker could not resolve.
It also shows code left unchecked during import. Use those details to choose the next language change.

Calls can now [replace checked Python defaults](docs/replace-python-defaults.md) without consuming the stored object.
For example, `choose(value=stored_list)` can receive submitted input through `choose(read_input())`.
Calling `choose()` still needs object behavior the analyzer does not check.
This improves a general call rule; it does not finish the familiar libraries' package imports.

The [numeric investigation](docs/python-symbolic-investigation.md) checked whether Pydash keeps a quantity between user-supplied limits.
Ordinary tests and an existing symbolic execution tool found the same missing assumption: the caller must order the limits.

Symbolic execution searches for inputs satisfying conditions in the code. This example does not justify building another execution engine inside `fr`.
Use ordinary tests for this question; consider the existing checker again when a real task needs harder input searches.
Keep original package imports separate from copied-function results, count setup and checking costs, and reproduce reported failing inputs in Python.
The experiment adds no analyzer support and does not restart the stopped agent studies.

Continue the Python work from the actual library failures. For example, importing a helper can
run a package's setup code before the helper is available. Explain which statement prevents an
answer, then improve the language rule needed to handle it.
The [startup investigation](docs/python-package-startup-results.md) shows why another declaration rule alone cannot finish these package questions.
Funcy and Pydash exceed the current file or source budgets in their complete observed Python executions.
Small no-argument startup functions can now fill dictionaries created earlier in their own file.
The [startup lesson](docs/check-python-startup.md) compares this check with a function that replaces the selected helper and must stay unfinished.
Saved answers already depend on source files and import lookups.
The next implementation [asks about two helpers together](docs/check-python-together.md).
It prepares shared startup once within one request, then returns separate function answers.
Both questions share the existing work and output limits; changed sources and unknown effects remain visible.
Separately saved startup checks still need a strict dependency-checked admission path.
Do not describe request-local sharing as reuse across requests or as finishing a large package import.
Never teach the analyzer the names of test projects or their expected answers.
An unfinished search must still say the answer is unknown.

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
each milestone must demonstrate, its supporting tests and what is missing.
The catalog is the machine-readable list of those requirements. Its identifier is `agent-analysis-v2`.
Open requirements include unfamiliar projects and complete agent-cost accounting;
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
- [x] Save the compiler command, build inputs, tool versions and any disagreement with fr.
  See [compiler checks](docs/project-checks.md#retained-compiler-diagnostics).
- [x] Save the requested outcome, possible explanations, checked facts, their inputs and remaining tests.
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
- [x] Record how supported functions pass arguments, return values and perform actions, including calls to themselves.
  See [function summaries](src/project/flow_summaries.rs).
- [x] Let users name input, output and context-specific cleaning functions; report possible connections and their source locations.
  See [dataflow](src/project/dataflow.rs).
- [x] Separate possible callees, possible input-to-output connections and statements established by a proof.
  See the [analysis contract](docs/agent-investigations.md).

Still needed: useful reaching-input and blocked-input answers across independently selected projects.
Shared mutable objects and implicit exceptions also need honest handling.
An implicit exception is an error an operation can raise without an explicit `raise`. The two recent
repository delivery tasks still report incomplete or refused flow analysis.

### C. Resume work and reuse correct answers

Example: another agent changes a helper while work is paused. On resumption, discard conclusions
that depended on that helper, keep unrelated findings, and require a fresh edit review.

- [x] Distinguish references to current code, fingerprints of saved contents and candidate matches after an edit.
  See [target sessions](sdk/python/src/fr_ir/investigation_session.py).
- [x] Record which source files, imports, settings and analyzer code support an answer, including searches that found nothing.
  See [module dependencies](src/project/flow_modules.rs).
- [x] Save and verify analysis results; analyze again when the tool cannot fully check the inputs needed for reuse.
  See [flow caching](sdk/python/src/fr_ir/flow.py).
- [x] Save tasks and discard outdated findings before continuing.
  See [resumption tests](tests/investigation.rs).
- [x] Tie check and proof results to their inputs, rejecting old results after relevant changes.
  See [retained proofs](src/spec/retained.rs).
- [x] Measure first analysis, reused results and rechecking after edits on a small generated set of programs.
  See [cache measurements](docs/evaluations.md#current-matched-results).

Still needed: project-scale change tests and measurements of time, memory and disk on unfamiliar
repositories. Small synthetic timing improvements do not demonstrate cheaper agent tasks.

### D. Make checked changes and useful proofs

Example: change a function's interface and its callers, run the relevant behavior tests, and produce
a patch another checkout can apply. For a proof request, connect the theorem to the actual code.

- [ ] Discover all relevant callers, contracts, configuration and tests for the declared change.
  [Caller discovery](docs/change-scopes.md) and [structural changes](docs/structural-changes.md) cover
  specific cases; dynamic dispatch (choosing a callable from runtime values) and inferred contracts remain incomplete.
- [ ] Add general language rules needed by observed tasks. Define what succeeds, what fails and what other actions the code performs.
- [ ] Translate a function under explicit arithmetic, error and evaluation-order rules, independently
  checking its source and target behavior.
- [ ] Connect a property and proof to an existing implementation, and write a new implementation
  together with its specification and proof.
- [ ] Retain old and new proof inputs for changes and translations; reject outdated proof claims.
  [Model comparisons](docs/model-comparisons.md) cover small Boolean models; broader source proofs remain open.
- [x] Check small models deciding when edits may proceed, task steps may change state and interrupted writes may recover.
  See [Lean checks](docs/lean-specs.md) and [host recovery](docs/host-recovery.md).

Still needed: broader structural changes, a useful numeric translation, and checked connections
between source implementations and proofs. Passing a theorem about a separate model is insufficient.

## Working rules

Use small reports with explicit follow-up reads, exact source references and stale-result rejection.
Check behavior independently of the proposed patch. Preview edits, preserve unrelated work and
retain undo and recovery. Use `fr` on its own code when its public operations fit, and record concrete gaps.

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
