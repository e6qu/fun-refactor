# Does fr help an agent do useful work more efficiently?

Review baseline: PR #394, commit `05fb612ae7974f70025e1d6208d54e5ef1e7efb8`, September 30, 2026.
This is a repository review and an evaluation plan. It contains no new live-agent measurements.

The product is a CLI that lets an AI agent understand code, make needed changes and fixes, and
develop and check proofs. It should reduce the effort and context needed to complete those tasks
correctly, including when an agent delegates a focused question to another agent. Each feature
must justify its complexity against that objective.

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

The [technical status report](roadmap-status.md) shows seven demonstrated items and eleven open
items. Its counts measure the stated test obligations. They do not measure agent efficiency.

## Models to evaluate

Official documentation checked September 30, 2026:

| Family | Current documented model ID | Standard input / output price per million tokens | Evaluation implication |
|---|---|---|---|
| Claude Sonnet | `claude-sonnet-5-5` | $2 / $10 | Record adaptive thinking settings and provider usage, including cache reads and writes |
| OpenAI Luna | `gpt-6-luna` | $0.10 / $0.50 | Record reasoning effort and usage; separate uncached input, cached input, cache writes and output |

Sources: [Anthropic model IDs](https://platform.claude.com/docs/en/models/overview),
[Sonnet pricing](https://www.anthropic.com/claude-sonnet-5-5),
[Luna model documentation](https://developers.openai.com/api/docs/models/gpt-6-luna).
These are dated API prices, not subscription billing or a forecast of cost per completed task.
Record the actual model response identity, provider, harness version and price schedule for every
run; an alias may change. Do not silently replace the chosen model mid-comparison.

Compare tools within each model first. Sonnet and Luna have different tokenizers, prices and
reasoning controls; the same text or nominal effort label does not make their costs comparable.
Neither provider's advertised coding performance establishes a benefit from `fr`.

## What deserves scrutiny

These are initial judgments based on code, existing results and dependency boundaries. They are
not claims that a subsystem is unused. File sizes below count tracked working-tree bytes at the
review baseline; they exclude Git history, caches, binaries and installed dependencies.

| Area and concrete code | Initial disposition | Evidence needed before removing or expanding it |
|---|---|---|
| Parsing, indexing, source locations, maps and bounded reads: `src/index/`, `src/project.rs`, `src/project/disclose.rs` | Keep as the core; measure response overhead | Can an agent locate and explain code with fewer total tokens and fewer mistaken conclusions than search plus file reads? |
| Progressive context and object reuse: `sdk/python/src/fr_ir/context.py`, `intent.py` | Keep the capability; compare direct CLI and optional SDK use | Measure repeated reads, cached-provider input, local object growth and complete task cost. Digest verification is useful, but does every response need all its metadata? |
| Goal, intent, guide and task layers: `src/project/agent_guide.rs`, `agent_intent.rs`, `task.rs`; SDK `guide.py`, `intent_actions.py`, `investigation*.py` | Challenge overlap and consolidate after measurement | Disable one layer at a time while retaining the same edit checks; count instructions, requests, refusals and task failures. Keep an adapter if consumers need it rather than duplicating implementations |
| Editing, history, checks and patch delivery: `src/edit/`, `src/history/`, `src/checks/` | Keep correctness and recovery mechanisms | Simplify the agent-facing route where possible without bypassing review, stale-input rejection, checks or recovery |
| Python flow and caching: `src/project/control_flow.rs`, `dataflow.rs`, `flow_*.rs` | Evaluate usefulness before adding more constructs | Does value tracing change the agent's decision on an unfamiliar task, and does it beat reading the relevant functions? Preserve unknown results |
| Proof authoring and verification: `src/spec.rs`, `src/spec/`, `kernels/` | Keep proof work in scope; challenge unrelated proof machinery | Require a user-requested property, a checked connection to actual code, useful counterexamples or a concrete safety invariant. Kernel theorem counts are not proof-task success |
| Framework translation and application models: `src/application_ir.rs`, `src/transpile/`, `src/translate.rs` | Candidate for isolation or reduced scope | Compare maintained user tasks with authoring overhead and runtime CI costs. Shared analysis or writer code may still be core; inspect imports before moving it |
| Browser playground and WASM: `web/`, `src/wasm.rs`, `crates/wasm-libc/` | Candidate for optional distribution or removal from the CLI critical path | Establish consumers and inspect feature/CI coupling. A demonstration UI is not required for the CLI objective, but existing shared virtual-workspace tests may be |
| Bundled grammars: `grammars/`, language features in `Cargo.toml` | Measure before trimming language coverage | Lean grammar sources account for 83.5 MiB of tracked files; that is not the binary contribution. Measure release size and build/runtime costs per feature on runners. Proof tasks still need Lean support |
| Evaluation artifacts: `tests/agent-eval/results/` | Prioritize storage review | 5,603 tracked files occupy 624.0 MiB. Identify current test dependencies, unique historical failures and duplicate snapshots. Archive only with reproducible retrieval and integrity checks |
| Documentation and evaluation scripts: `docs/`, `tools/` | Consolidate repeated instructions and obsolete entry points | Keep one current guide and immutable historical results; verify consumers and links before deleting a script |

The largest storage candidate is retained evidence, not the 278 KiB browser source directory.
Deleting a small UI may simplify maintenance while barely changing checkout size. Removing files
from the working tree also does not shrink existing Git history. Treat storage, build time,
runtime resources and agent context as separate measurements.

The storage counts can be reproduced from `git ls-tree -r -l 05fb612a --
tests/agent-eval/results grammars/lean web` by summing blob sizes within each directory. A working-tree
inventory counted the same tracked files; no build caches were traversed.

## Pilot before expanding or deleting subsystems

The [study planner and evidence auditor](agent-study.md) now accept independent task manifests and
account for declared parent and child invocations. They retain missing cells, failures and unknown costs.
The provider gateway now reserves and settles requests, and an isolated serial loop connects agent
tools and private grading for fix/feature tasks. Container resource accounting is implemented;
whole-worker resources, complete context measurements and proof grading remain open.
The [OpenCode rehearsal](opencode-rehearsal.md) adds a separate local Kimi/GLM protocol check. Its
synthetic tasks and CLI-reported usage do not satisfy the independent pilot requirements.
The [explanation adapter](opencode-source-evidence.md) now checks finite factual answers against
source actually retrieved from three pinned Python projects. This does not establish general
understanding, efficient delegation or token savings.
The [native OpenCode adapter](opencode-native-tools.md) addresses the observed JSON-action
failures. Tool availability and actual use are reported separately: an agent that uses only
ordinary reads in the `fr` arm supplies no evidence of a benefit from `fr`.
The native trials have eight reviewed passes out of twelve; only one of six `fr` arms used the CLI.
The later compact exploration comparison originally had three passes, nine citation failures and six timeouts
across eighteen attempts. A separate coverage correction recovers three answers whose exact quotations
crossed adjacent retrieved pages. The reviewed total is six passes; original records remain unchanged. Both observed `fr` calls came from guided attempts and stopped at names.
All completed factual values were correct; source citation checks still failed in nine attempts.
The public excerpt added prompt bytes without an established benefit. The next source-reference
pilot retained two passes and four timeouts on one reviewed task. Both passes used source IDs;
guided Kimi followed fr behavior and source continuation. The other passing attempt used ordinary
tools. The guided run delivered 6,784 source bytes but 19,736 total tool-result bytes; the other
pass delivered 9,119 source bytes and 14,644 total tool-result bytes. Source bytes alone cannot
establish context efficiency. Required factual anchors and historical records remain unchanged.
Next add independently graded change tasks and measure total costs, including metadata and handoffs.

The older `tools/agent_eval/investigation_run.py` path invokes Codex and names two Rust tasks.
The current manifest-driven host is `tools/agent-eval-host.py`; it retains parent and child calls.
Neither path supplies new matched Sonnet results. Keep orchestration in the evaluation host and
keep `fr` focused on code operations; do not build a general agent platform inside the product.

Freeze four task instances across at least three independent repositories: explain a behavior,
fix a bug, add a feature, and prove a stated property about a bounded function. Choose tasks by
coverage needs before implementation, rather than choosing ones the current commands already solve.
Pin revisions, requirements, hidden graders, model settings, resource budgets and a held-out project.
For understanding, grade factual accuracy and source references; for changes, use independent
behavior and regression checks; for proofs, check the theorem and its connection to the source.

The initial pilot is 48 attempts:

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
- Tool CPU, sampled aggregate RSS, disk and cache growth, cold versus warm behavior, and full build
  or check costs. Response byte limits do not measure complete prompt usage, and sampled memory
  peaks can miss short-lived allocations.

The CLI currently labels its disclosure budget a conservative token upper bound compatible with
byte-fallback tokenizers, calculated from serialized UTF-8 bytes in
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
For public routes, establish compatibility needs and choose consolidation, deprecation or an optional
package. For stored evidence, retain unique failures and the manifests that tests depend on; first
prove that a fresh checkout can retrieve and audit any replacement archive. Do not rewrite Git history
as part of ordinary cleanup.

The first cleanup experiment should target guide/intent/task overlap and oversized context packets.
Those costs occur on the path every agent uses. In parallel with that design review, inventory
evidence snapshots for exact duplication and active references. Defer further framework expansion
and broad new proof infrastructure until a task demonstrates why it is needed.

This review does not label working subsystems as dead code or remove evidence to improve the
appearance of the results. The next removal PR should state what behavior is preserved, what is
deliberately retired, which measurements justify the cut and which full gates passed on GitHub.
