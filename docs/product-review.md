# Does fr help an agent do useful work more efficiently?

Use this page to decide what helps an agent finish a task and what needs further measurement.
Read the [plan](../PLAN.md) for current work and [recorded experiments](evaluations.md) for measurements.

The baseline is the alternative used in a comparison. Here it is an agent using ordinary search, reads and edits.
An arm is one tool setup, such as those same tools plus `fr`.
Overhead includes extra instructions, indexing, process calls, report validation and review needed to use the tool.
For a checkout repair, count that work alongside the final test outcome.
The [measurement guide](benchmarks.md) explains which existing numbers include these costs and which leave them unmeasured.

We want an agent to finish a real task correctly with less unnecessary work. For example,
“Fix checkout rounding without changing the API” should lead to the right calculation, a small
reviewed patch and a behavior test that catches the reported error.

The tool is worth maintaining when it helps that whole task. Finding more symbols or
returning a shorter report matters only when it improves the agent’s decision or reduces total work.
A second agent should receive a focused question and useful findings, without repeating the search.

## Where we want to get

| User request | A useful outcome |
| --- | --- |
| “Explain this unexpected behavior” | A correct explanation with the relevant code and a clear account of unanswered questions |
| “Fix this bug” | A reviewed repair that passes independent behavior and regression checks |
| “Add this feature” | The requested behavior, including constraints that a superficial implementation could miss |
| “Check this property” | A precise statement, an accepted proof and a checked connection to the actual code |
| “Let another agent continue” | A handoff that saves repeated investigation and still rechecks changed dependencies |

We need repeated comparisons on unfamiliar projects to show whether `fr` makes those outcomes
more reliable or cheaper. Count all reading, retries, failures and work by every agent. The current
results below do not yet establish that benefit.

## What the existing evidence says

The [two-task live comparison](evaluations.md#unknown-target-investigations) used
`gpt-5.6-luna`, low reasoning effort, with ordinary files or `fr`. Each task has one comparison
pair, so the sample is too small to estimate a general success rate.

| Task | Ordinary files | fr | Practical result |
|---|---|---|---|
| Fix Unicode string comparison | Passed; 17 calls, 27,604 visible bytes, 326,494 input tokens, 126.8 seconds | Passed; 23 calls, 75,165 visible bytes, 618,428 input tokens, 185.6 seconds | fr cost more context and time in this pair |
| Add an allocation-constrained escaping feature | Failed; 22 calls, 59,707 visible bytes, 644,478 input tokens, 223.9 seconds | Passed; 33 calls, 70,857 visible bytes, 763,904 input tokens, 307.0 seconds | fr completed a requirement the other attempt missed, at higher measured usage |

Input totals include cached tokens; they are not dollar costs. These results do not establish a
general efficiency benefit. Smaller synthetic evaluations also contain cases where structured
reports disclose much more text than ordinary source reads. Those losses must remain visible.

The [Python repository tests](evaluations.md#roadmap-outcomes-on-pinned-python-repositories) contain
prescribed fixes and symbol queries in `tools/python-repository-acceptance.py`. Their 365 behavior
cases establish delivery, interruption recovery and patch replay for those fixes. They do not
measure an agent discovering a solution. A search of production Rust and Python SDK sources found
no references to the two repository identities; that check alone does not establish generalization.

The [current status report](roadmap-status.md) owns the count of checks that meet their stated
requirements. Those checks cover particular examples. Three of the four larger project goals still have
unfinished requirements. Continuing work and correct reuse meets its stated tests; that does not establish savings for an agent.

## Agent setup review, 22 October 2026

We ship one portable [fr skill](../skills/fr/SKILL.md), with references for finding code, edits, checks, saved work and proofs.
Its name and description supply the basic information these agents need to discover a skill.
The checkout exposes that bundle through `.claude/skills/fr` and `.agents/skills/fr`.
The [README](../README.md#give-fr-to-an-agent) links the skill and its introductory lesson.
The [installation guide](agent-skill.md#install-the-handoff) also explains installation for other projects.

The skill checker runs its shell examples against temporary projects.
Separate [native discovery checks](https://github.com/e6qu/fun-refactor/actions/runs/39746584042) pass with pinned Claude Code, Codex and OpenCode clients.
They check absent, project and user installations, plus this checkout's linked bundle.
These checks use isolated settings and no live model credentials. Claude Code receives a scripted response from a local server.

| Agent | What we have checked | What remains |
| --- | --- | --- |
| Claude Code | Native discovery advertises the installed skill once | Whether a live agent chooses and follows it on unfamiliar work |
| Codex | Native discovery lists the installed skill once, with no loading errors | Whether native skill use improves complete tasks; earlier evaluations supply prepared prompts |
| OpenCode | Native discovery lists the installed skill once | Whether an agent chooses fr and uses it successfully during changes |

The [OpenCode change records](native-change-outcomes.md) contain ten behavior passes and two collection failures, with zero fr calls.
Those passes establish no benefit from fr.
The [reading records](native-read-outcomes.md) include one guided attempt using fr, but no successful ordinary/fr comparison pair.
The repository also has a separate `.claude/skills/lean-spec` guide for Lean work.

For discovery requirements, see the official [Claude Code](https://code.claude.com/docs/en/skills),
[Codex](https://learn.chatgpt.com/docs/build-skills) and [OpenCode](https://docs.opencode.ai/docs/skills/) documentation.
An optional Codex `agents/openai.yaml` file is not required for basic skill discovery.

## What to learn from CodeGraph

This review means [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph), the project linked by [Graphify's comparison](https://graphify.com/vs/codegraph).
CodeGraph documents an installer for all three agents and one main exploration tool returning relevant source, call paths and likely affected code.
It maintains a local index after edits.
Its published agent measurements cover architecture questions on seven repositories, with four runs per setup.
They are not a matched comparison with fr or evidence of delivering our change tasks.

For example, CodeGraph targets “how does this request reach the database?”
Our wider goal also includes “change the relevant helper, check the result, undo it and deliver a patch.”
Finding a call path alone does not establish which submitted values reach an output or prove program behavior.
The practical lesson is easier installation and fewer choices before the first useful answer.
We should measure whether a simpler fr route helps before adding another wrapper or service.

## Proposed next steps

The [three-PR roadmap](../PLAN.md#next-three-prs) owns the agreed sequence and completion checks:

1. Make fr usable by agents and measure its value, including complete Python costs and a fair CodeGraph comparison.
2. Complete broader Python analysis and translation.
3. Connect proofs to code and repeat the agent comparison.

These packages preserve the four existing milestones and their acceptance requirements.
They do not authorize restarting stopped model studies.
The [numeric investigation](python-symbolic-investigation.md) supplies no present reason to build another symbolic execution engine.

### First comparison with CodeGraph

Start with 18 attempts: two existing change tasks, three agent applications and three tool setups.
One task repairs Unicode string comparison. The other adds a way to calculate escaped text length without allocating a temporary string.
Their [requirements and independent behavior checks](../tests/agent-eval/unknown-target/task.json) already exist.
These projects are familiar to development; fresh agent sessions do not turn them into a new unread-project evaluation.

For each application, compare ordinary search, reads, patches and checks with those same tools plus CodeGraph, then those tools plus fr.
Use the same exact model and reasoning settings within each application's three setups.
Compare those setups within each application before comparing applications, which may use different models.
Rotate setup order and retain every first attempt, including failure and non-use of an offered tool.
This small pilot checks integration, tool choices and complete task costs. It cannot establish general savings or close the repeated-agent requirement.

Give both tools their documented installation instructions. Count skill text, server setup, indexing, edits, validation and shutdown.
Count a first session with setup included; report later use separately instead of spreading an unmeasured setup cost across hypothetical tasks.
Keep ordinary editing and checking tools available in all setups. Keep the other two specialized tools out of each setup's configuration and executable search path.
Grade delivered behavior and patch replay with the same independent checks. An explanation also needs correct source references and explicit uncertainty.

Use the [pinned native clients](../tests/agent-eval/agent-first-use/hosts.json) and [CodeGraph release](../tests/agent-eval/agent-first-use/codegraph.json).
Before collecting, freeze the final fr revision, complete prompts, graders, model identities, reasoning settings, prices, credentials available on the runner and spending cap.
Reuse the existing task limits; diagnose a timeout before changing the procedure.
No live attempts have started. The stopped studies remain stopped, and their records stay separate.

## Models to evaluate

The broader study targets Claude Sonnet and OpenAI Luna. Local protocol and task rehearsals use
the configured Kimi K3 and GLM 5.3 Flash profiles. A rehearsal does not substitute for that study.

Freeze exact provider/model identities, reasoning settings, harness version and applicable pricing
in each run manifest. Resolve versions and prices when planning that run; an alias may change.
Compare tool conditions within each model first. Tokenizers, reasoning controls and cache accounting
differ, so the same text or effort label does not imply comparable cost. Keep unverified billing
unknown. Advertised model performance cannot establish a benefit from fr.

## What deserves scrutiny

These areas need measurement before expansion or removal. Check who uses each part and which
useful behavior depends on it before changing commands or formats.

For example, if a caller query sends more text than reading the helper, ask whether that extra text
prevented a wrong edit. If it did not help the task, shorten the route and measure again.
Keep review, undo and relevant checks working while simplifying the instructions.

| Area and concrete code | What to do first | What we need to learn before removing or expanding it |
|---|---|---|
| Parsing, indexing, source locations, maps and bounded reads: `src/index/`, `src/project.rs`, `src/project/disclose.rs` | Keep as the core; measure response overhead | Can an agent locate and explain code with fewer total tokens and fewer mistaken conclusions than search plus file reads? |
| Progressive context and object reuse: `sdk/python/src/fr_ir/context.py`, `intent.py` | Keep the capability; compare direct CLI and optional SDK use | Measure repeated reads, cached-provider input, local object growth and complete task cost. Digest verification is useful, but does every response need all its metadata? |
| Goal, intent, guide and task layers: `src/project/agent_guide.rs`, `agent_intent.rs`, `task.rs`; SDK `guide.py`, `intent_actions.py`, `investigation*.py` | Challenge overlap and consolidate after measurement | Disable one layer at a time while retaining the same edit checks; count instructions, requests, refusals and task failures. Review existing users before consolidating routes; keep one implementation and one current format |
| Editing, history, checks and patch delivery: `src/edit/`, `src/history/`, `src/checks/` | Keep correctness and recovery mechanisms | Simplify the agent-facing route where possible without bypassing review, stale-input rejection, checks or recovery |
| Python flow and caching: `src/project/control_flow.rs`, `dataflow.rs`, `flow_*.rs` | Evaluate usefulness before adding more constructs | Does value tracing change the agent's decision on an unfamiliar task, and does it beat reading the relevant functions? Preserve unknown results |
| Proof authoring and verification: `src/spec.rs`, `src/spec/`, `kernels/` | Keep proof work in scope; challenge unrelated proof machinery | Require a user-requested property, a checked connection to actual code, useful counterexamples or a concrete safety invariant. Kernel theorem counts are not proof-task success |
| Framework translation and application models: `src/application_ir.rs`, `src/transpile/`, `src/translate.rs` | Candidate for isolation or reduced scope | Compare maintained user tasks with authoring overhead and runtime CI costs. Shared analysis or writer code may still be core; inspect imports before moving it |
| Browser playground and WASM: `web/`, `src/wasm.rs`, `crates/wasm-libc/` | Candidate for optional distribution or removal from the CLI critical path | Establish consumers and inspect feature/CI coupling. A demonstration UI is not required for the CLI objective, but existing shared virtual-workspace tests may be |
| Bundled grammars: `grammars/`, language features in `Cargo.toml` | Measure before trimming language coverage | Measure release size and build/runtime costs per feature on runners. Proof tasks still need Lean support |
| Evaluation artifacts: `tests/agent-eval/results/` | Prioritize storage review | Identify current test dependencies, unique historical failures and duplicate snapshots. Archive only with reproducible retrieval and integrity checks |
| Documentation and evaluation scripts: `docs/`, `tools/` | Consolidate repeated instructions and obsolete entry points | Keep one current guide and immutable historical results; verify consumers and links before deleting a script |

## Measured evidence storage cleanup

The [archive conversion](../tests/agent-eval/EVIDENCE-ARCHIVES.md) saved 366.6 MiB in an expanded
checkout by compressing eleven historical flow reports. Each must restore its original bytes,
SHA-256 and Git blob ID. Current acceptance evidence and unique failures remain available.
This does not shrink Git history or establish lower agent cost. Further pruning requires a
consumer review and reproducible retrieval; measure storage, build time and runtime separately.

## Pilot before expanding or deleting subsystems

The configured packaging pilot stopped when its first live call hit the CPU cap.
The later resource investigation found no justified client configuration fix and is closed.
Keep the failed and three unstarted cells. The [current plan](../PLAN.md#next-three-prs)
continues tool work; it does not authorize restarting that pilot.

The [source-reading report](native-read-outcomes.md) has no successful ordinary/fr pair.
The [code-change report](native-change-outcomes.md) has ten behavior passes, two timeouts and no
fr use. Tool availability is not adoption. The [candidate review table](candidate-review-status.md)
records packaging's limited admission and the other tasks' gaps. These reports own detailed run
history; superseded collection instructions do not belong in the active product plan.

The [manifest-driven study host](agent-study.md) retains declared parent/child calls and private
grading. Its scripted controls are not live trials, and complete worker/context costs remain open.
Keep orchestration in the evaluation host and fr focused on code operations.

The following broader study is a proposal, not a running collection or authorization to spend.
Independent task selection, full accounting and resource admission must be ready first.

Freeze four task instances across at least three independent repositories: explain a behavior,
fix a bug, add a feature, and prove a stated property about a bounded function. Choose tasks by
coverage needs before implementation, rather than choosing ones the current commands already solve.
Pin revisions, requirements, hidden graders, model settings, resource budgets and a project kept unread during development.
For understanding, grade factual accuracy and source references; for changes, use independent
behavior and regression checks; for proofs, check the theorem and its connection to the source.

The proposed study contains 48 attempts:

- **32 single-agent attempts:** four tasks, two model families, two repetitions and two tool arms.
- **16 delegated attempts:** two of those tasks, both model families, two repetitions and both arms,
  with the same predeclared limit of two child agents per attempt.

The ordinary arm gets search, file reads, patches, compilers and proof tools. The `fr` arm gets
the same tools plus the released CLI and its small skill entry point; ordinary fallback remains
available and is recorded. Both arms get identical task requirements, check access and aggregate
limits. Hidden graders are unavailable to both. Randomize arm order and record cache state. Run
serially on remote workers initially, with a fixed total spend cap in the run manifest. If access,
model settings or provider accounting are unavailable, mark the cell blocked; do not substitute a model.

Use these small repeated samples to find overhead and failure modes. They cannot establish a
population-wide success or savings rate. Preserve unsuccessful and inconclusive attempts, rather
than retrying until each box is green. Any broader efficiency claim needs a larger independent
evaluation with a predeclared uncertainty analysis.

## Measure all the work

For each attempt retain:

- Independent task outcome, regressions, unsupported claims, human intervention and any partial result.
- Exact model and harness settings; uncached and cached input, cache writes, output and reasoning
  usage where provided. Keep raw provider accounting and missing fields; avoid counting a reasoning
  subset twice when it is already included in output. Calculate actual or explicitly estimated cost
  from that run's pricing, not from visible bytes.
- Parent and every child agent's usage, including failed children, summaries and integration work.
  Report wall-clock duration and total agent time separately; parallelism can reduce one and increase the other.
- Initial skill/schema instructions, every disclosed tool result, source reads, retries, pagination,
  repeated disclosure and fallback. Measure maximum active context as well as cumulative input.
- Tool CPU, sampled aggregate RSS (resident set size, the measured memory held by a process), disk and cache growth, cold versus warm behavior, and full build
  or check costs. Response byte limits do not measure complete prompt usage, and sampled memory
  peaks can miss short-lived allocations.

The CLI currently labels its disclosure budget a conservative token upper bound compatible with
byte-fallback tokenizers, calculated from serialized UTF-8 (the text encoding used to count source bytes) bytes in
[`src/project/disclose.rs`](../src/project/disclose.rs). That bounds a response under the stated
assumption; it is not a tokenizer measurement or the provider's charge for the full conversation.
Keep that safety limit separate from the usage measurements above.

Report success and failure counts first, then costs for all attempts and costs for the comparable
successful pairs. Also report total spend divided by successful outcomes, with failures included;
if there are no successes that quantity is undefined. Do not hide failures by reporting only the
cheapest successful runs or flatten quality and cost into an opaque score.

## Sharing context with subagents

Leave scheduling and model choice to the host agent. `fr` should supply trustworthy, compact
context and edit operations that any host can use. A handoff should contain the task question,
repository revision, exact code references, known facts, uncertainties and the permitted scope.
The recipient should be able to follow references for additional source, rather than receive a
dump of the parent's transcript or every stored proof record.

Keep findings separate from hypotheses. A content digest is a reference to data, not evidence
that a conclusion is true. Before integrating a child agent's proposed edit, refresh dependencies
and review it against current source. Measure duplicated discovery, contradictions, conflicts,
handoff tokens and revalidation work. Use delegation when its complete measured cost and outcomes
justify it. Measure that tradeoff for each task type.

## Removal decisions

For each candidate, record its actual callers and consumers, unique behavior, maintained checks,
build/runtime cost, agent-facing instructions and evidence of useful tasks. Compare the full route
with a version that omits or simplifies that component. A search for references is a starting point,
not proof that an external public API is unused.

Remove unreachable or redundant internal code when its behavior and consumers are accounted for.
For public routes, identify consumers and keep one current interface without compatibility readers or fallback routes.
For stored evidence, retain unique failures and the manifests that tests depend on; first
prove that a fresh checkout can retrieve and audit any replacement archive. Do not rewrite Git history
as part of ordinary cleanup.

The first cleanup experiment should target guide/intent/task overlap and oversized context packets.
Those costs occur on the path every agent uses. In parallel with that design review, inventory
evidence snapshots for exact duplication and active references. Defer further framework expansion
and broad new proof infrastructure until a task demonstrates why it is needed.

The first concrete output reduction separates source and relationship pagination in
`project explore`. The retained guided source-reading pass repeated the same 978-byte relationship
object while requesting its next source page. Focused continuations now return only the requested
view with its identity, limits and coverage. Python/Rust regression tests require complete page
traversal and fewer serialized bytes than the same combined page. This removes measured duplication;
it does not establish lower total agent cost or close an efficiency acceptance item.

This review does not label working subsystems as dead code or remove evidence to improve the
appearance of the results. The next removal PR should state what behavior is preserved, what is
deliberately retired, which measurements justify the cut and which full gates passed on GitHub.
