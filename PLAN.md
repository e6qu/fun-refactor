# Help an agent finish the right change with less work

Give an agent a bug or feature request in a project it has never seen.
Help it find the right code, make the change and check the result.
The goal is fewer unnecessary reads and retries, lower total cost and dependable handoffs.

For example, fixing checkout rounding should produce a reviewed patch and a test that catches the reported error.
Finding the calculation is only part of that outcome.

## Where we are

`fr` can find functions and callers, preview edits, run declared checks, undo changes and resume saved work.
Selected Rust and Python project changes pass behavior checks and patch replay in another checkout.
Selected number, date and text questions work through original Python package imports.
Broader imports, exceptions and shared mutable values remain incomplete.

We have not demonstrated general time or cost savings for agents.
The [complete session comparison](docs/project-session-cost.md) measures direct CLI calls, restarted Python clients and Python clients kept running across questions.
It also compares saving and validating answers with asking again. Read its included costs and limits before interpreting the overhead.

The [status report](docs/roadmap-status.md) owns current test counts and completion claims.
Milestone C, correct resumption and reuse, meets its stated requirements. A, B and D remain open.
The four-package check meets the specific project-tracing requirement; milestone B still needs broader reusable language rules.
The [product review](docs/product-review.md) explains the evidence and agent integration gaps.

## Next three PRs

The model-free preparation in PR 1 has merged. Its live comparison needs agreed models, settings and spending limits.
Continue the language work in PR 2 while that decision remains open. Its results should shape PR 3.
Each PR includes its implementation, independent checks, examples, limitations and tracking updates.
These are three delivery packages, not replacements for the four milestones below.

### PR 1. Make fr usable by agents and measure its value

Example: an agent finds a helper, previews its rename, checks its callers and undoes the change.

Done: all three applications discover the project skill, and the introductory change lesson passes.
The CLI and Python comparison now includes startup, edits, validation and shutdown.
Direct CLI calls were fastest in these small sessions; keeping Python running reduced its overhead.
Every measured Python approach cost more than the direct CLI baseline.
Saving answers did not show consistent savings over fresh Python checks. These results do not measure agent savings.

The [comparison preparation](docs/agent-comparison-preparation.md) defines eighteen first attempts.
Its controls exercise the real applications in all nine tool setups, retain their configurations and prompts, and check complete background-process shutdown.
Task controls must reject unchanged and incomplete code, accept the reference repair and replay its patch independently. These controls use no live models.
Complete-task controls now pause each application, rename a local variable, reject the stale edit and deliver a refreshed repair.
They preserve the intervening change and grade the patch in another checkout. These are scripted controls on familiar tasks.
Next: agree live models, offered tools, credentials, prices, runtime settings and full-attempt limits before paying for attempts.

- Keep one portable skill with explicit setup for Claude Code, Codex and OpenCode.
  Check discovery in clean sessions and record actual tool use separately from scripted integration controls.
- Teach the complete find, inspect, preview, check and undo route through examples that introduce each new concept.
- Compare direct CLI calls with the Python library restarted for each question and kept running across questions.
  Compare saved answers with fresh analysis. Count startup, edits, validation, shutdown, memory and storage, with matching answers required.
- Compare ordinary tools, ordinary tools plus [CodeGraph](https://github.com/colbymchenry/codegraph), and ordinary tools plus `fr` on the same unfamiliar tasks.
  Give every setup the same ordinary editing and checking tools. Independently grade explanations and delivered behavior.
  Count setup, indexing, instructions, model usage, retries and failed attempts. Record whether the agent used the offered tool.
- Fix waste supported by the measurements. Prefer a shorter route through existing interfaces before adding a wrapper or background service.

Complete when all three agents can discover and use the skill, the change loop passes, and complete comparison results retain failures and limitations.
Scripted controls establish integration behavior; live attempts establish adoption. A small pilot does not close the repeated-agent requirement in milestone A.
Freeze tasks, graders, model settings and agreed spending and resource limits before live calls.
The stopped studies stay stopped; this roadmap does not authorize their restart.

### PR 2. Complete broader Python analysis and translation

Example: determine whether customer input reaches `send(value)` through an imported helper, including an exception or a shared list mutation.
A shared list mutation changes an object that another name can read later.

PR #571 checked a page-count helper in both Rust/Python directions and fixed precision loss in integer division.
It also located unchecked Python exceptions and shared-object operations, including in saved answers and the SDK.
The [complete example](docs/check-language-behavior.md) explains the inputs, independent checks and remaining limits.
Text conversions and twelve read-only string methods now pass 68 checks against ordinary Python, including helper returns and reassignment.
Stringcase's README supplied a reserved lowercase question before implementation; its first inspection passed after fixing the checker and procedure.
The same executable also passed the Boltons, dateutil and JDcal questions, with original imports and saved-answer controls.
The [package example](docs/original-package-questions.md#follow-a-customer-label-through-stringcase) explains the results and preserved limits.
The [cleanup repair](docs/check-python-cleanup.md) follows a customer message through an imported helper.
It preserves the pending return while cleanup runs and notices when cleanup replaces it.
It also follows explicit helper raises through cleanup, including nested sections and loop exits.
Next, choose a project change that needs exception matching or shared mutable values.
Toolz, Funcy and Pydash still need broader startup work. Keep their full-package failures and existing limits visible.
Full exception and shared-object analysis remain open work in this delivery package.

- Handle package imports, exceptions and shared mutable values exposed by the chosen tasks.
  Use the original package imports. A copied helper in an isolated file answers a different question.
- Test both inputs that reach the output and inputs that cannot reach it.
  Compare results with ordinary Python execution and independent inspection.
- Check project-scale analysis across at least three independently selected projects, including one kept unread during implementation.
  Keep unsupported behavior and exhausted searches explicit; preserve existing limits.
- Extend caller discovery for the selected changes, including affected configuration and tests.
  Independently check the complete declared change scope before delivery.
- Translate one useful bounded function between Rust and Python.
  Declare input ranges, arithmetic, overflow, errors and evaluation order before implementing support.
  Independently compare every input in the chosen bounded range and the declared boundary cases.

Complete when the broader tracing, effects and translation requirements have qualifying results under the existing acceptance conditions.
One unchanged analyzer must handle the selected projects. Keep their editing, saved-answer checks, undo and patch replay passing.

### PR 3. Connect proofs to code and repeat the agent comparison

Example: prove a stated access rule about the actual function, then reject that claim when a relevant source edit changes its behavior.

- Connect a precise property and checked proof to one existing implementation.
  Identify the assumptions and components trusted to connect the code to its mathematical description.
- Write one new function together with its specification and proof, checking the same connection to its implementation.
  Retain relevant old and new inputs. Source changes must invalidate affected proof claims.
- Run independent behavior checks and replay the delivered changes in another checkout.
- Repeat unfamiliar agent tasks after the improvements, meeting the existing repeated-agent requirements.
  Report correctness first, then complete costs with failures and work by every participating agent included.
  Keep the CodeGraph comparison distinct from the ordinary-tools/fr results required by milestone A.

Complete when both source-proof requirements and the repeated-agent requirement have qualifying results.
A proof about a separate model alone is insufficient. Negative or inconclusive cost results remain visible.
Use an existing symbolic execution tool when a concrete task needs input searches; no new engine is planned.
The [earlier investigation](docs/python-symbolic-investigation.md) explains that decision.

## Milestones and implementation checklist

The checkboxes track implementation, not demonstrated product outcomes.
The [status report](docs/roadmap-status.md) checks both implementation and the specific test requirements.
The [acceptance catalog](tests/agent-eval/roadmap.json) owns their detailed conditions and evidence links.
Keep these counts reconciled with the catalog; close milestones only when all required work and results qualify.

### A. Find relevant code and explain the evidence

Open outcome: repeated unfamiliar agent tasks with correct answers and complete cost accounting. PRs 1 and 3 address it.

- [x] Report exact declaration, reference and call locations.
- [x] Link analyzed expressions to source and expose missing or ambiguous locations.
- [x] Explain findings with their inputs, assumptions and omitted information.
- [x] Retain compiler inputs, versions and disagreements with fr.
- [x] Save requested outcomes, checked facts and remaining questions.
- [x] Run the initial bug and feature comparison without supplying edit locations.

### B. Explain how values move through code

Open outcomes: broader project tracing and honest handling of exceptions and shared mutable values. PR 2 addresses them.

- [ ] Specify reusable language rules connecting source, analysis and edits, with independent examples and explicit limits.
- [x] Represent supported assignments, branches, loops, returns and explicit exceptions.
- [x] Describe supported argument passing, returns and actions, including recursive calls.
- [x] Trace user-declared inputs, outputs and cleaning functions with source locations.
- [x] Distinguish possible calls and value paths from statements established by a proof.

### C. Resume work and reuse correct answers

Complete for its stated tests. Preserve this behavior throughout all three PRs; agent savings remain a separate question.

- [x] Distinguish current code references, saved-content fingerprints and candidate matches after edits.
- [x] Track source, import searches, settings and analyzer inputs behind answers.
- [x] Verify saved answers and analyze again when reuse cannot be established.
- [x] Resume saved tasks after discarding outdated findings.
- [x] Invalidate check and proof results after relevant input changes.
- [x] Measure first use, reuse and rechecking after edits on three independent projects.

### D. Make checked changes and useful proofs

Open outcomes: translation and proofs connected to existing and new implementations. PRs 2 and 3 address them.
Broader caller discovery and proof invalidation also remain implementation work.

- [ ] Discover all relevant callers, contracts, configuration and tests for the declared change.
- [x] Add general language rules needed by observed structural changes.
- [x] Translate a function under explicit arithmetic, error and evaluation-order rules.
- [ ] Connect proofs to an existing implementation and to a new implementation developed with its specification.
- [ ] Retain proof inputs for edits and translations, rejecting outdated claims.
- [x] Check small models governing edits, task transitions and interrupted-write recovery.

## Generalization requirements

- Test each language rule with independently written examples. Renaming a helper or moving a supported file must preserve the answer.
- Select project-scale tasks before implementation. Keep one project unread until the analyzer and test procedure are frozen.
- Preserve the first unfamiliar-project failure. Once its code informs a fix, use a new unread project for independent evaluation.
- Keep production behavior independent of test project names, revisions and expected patches.
- Report unsupported behavior and exhausted searches. An unfinished check cannot establish that a path is impossible.
- Keep failures, timeouts and unstarted attempts visible. Missing usage or billing information stays unknown.
- Compare complete task outcomes and costs. Smaller responses and prescribed repairs alone cannot establish agent savings.

## Working rules

Use `fr --json audit` and `fr capabilities` for current command support and limits.
Use `fr` on its own code where its operations fit, and record concrete gaps.
Preview writes, preserve unrelated work and keep undo, recovery and patch delivery working.

Keep settings explicit and maintain one current interface and format.
Add no compatibility readers, fallback routes or needless wrappers.
Keep agent scheduling and model access in the evaluation host.

Full builds, full test gates and large result collections run on GitHub.
Local checks and `fr` use the [workstation guard](docs/development.md#working-on-a-shared-desktop); move refused work to CI.
Fix the cause of failures and timeouts. Keep existing limits and coverage.

Keep tutorials and historical runs in their guides, not this roadmap.
Use the [handoff](docs/continuity.md) for investigation details, the [measurement guide](docs/benchmarks.md) for comparison rules,
and the [evaluation guide](docs/evaluations.md) for results and preserved failures.
