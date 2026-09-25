# Evaluation evidence

Evaluation artifacts answer narrow questions about fixed revisions and fixtures. They are retained
evidence, not timeless product documentation. Live support comes from `fr audit`, `fr capabilities`
and the command schemas.

## Evidence classes

- Deterministic protocol checks execute fixed workflows without a model.
- Live-agent trials retain prompts, tool events, settings, usage and scores.
- Matched cohorts give two agents the same requested outcomes and compare their exposed context.
- Formal checks prove named model properties under explicit assumptions.
- Compiler, test and behavioral checks establish only the behavior they execute.

Every retained run has a manifest under `tests/agent-eval/results/`. Manifests bind inputs,
evaluators, binaries and reports by digest. Diagnostic runs remain diagnostics after later fixes;
acceptance runs must satisfy their scorer at recording time.

## Current matched results

The [flow-cache comparison](../tests/agent-eval/results/2026-09-25-flow-cache/result.json) contains 288 samples across six generated scalar workloads.
Pipelines, imported fanout and mutual recursion each have two sizes. Three repetitions rotate the
previous SDK cache and bounded report chunks through cold, warm and six single-edit scenarios.
Every retained answer agrees with clean analysis. Independent Python execution, source digests and
AST coordinates check the scenario; missing helpers and response-budget cutoffs remain incomplete.

| Workload | Previous warm median | Report-chunk warm median |
|---|---:|---:|
| Pipeline, 8 helpers | 1.484 s | 0.721 s |
| Pipeline, 16 helpers | 2.983 s | 0.943 s |
| Imported fanout, 8 helpers | 0.846 s | 0.647 s |
| Imported fanout, 24 helpers | 1.422 s | 0.693 s |
| Mutual recursion, 8 helpers | 1.617 s | 0.707 s |
| Mutual recursion, 12 helpers | 2.376 s | 0.797 s |

This selects whole-report storage while retaining whole-analysis dependency validation. Warmed
stores use 1.35–2.91 times the bytes of the previous per-node encoding. Native calls and disclosed
context stay unchanged. The run records isolated worker and child peak RSS, transfer counts,
store operations and bytes. Native fact caching is disabled; operating-system caches are uncontrolled.
The corpus represents these admitted task shapes on one host, without a production or population speed claim.
It contains no live-agent trial or token measurement. The pinned baseline SDK and diagnostic probes
are retained beside the [task](../tests/agent-eval/flow-cache/task.json).
New warm medians still exceed new cold medians on these workloads. The result improves retained
storage overhead; it does not establish an overall latency benefit from enabling the optional cache.

The accepted 2026-09-17 matched cohort gives both agents seven preview outcomes. The local SDK arm
uses two agent calls and 46,120 input tokens. The direct-file arm uses seven calls and 89,118 input
tokens. Both pass without source mutation, failed commands, bypasses or human correction.

This one pair shows a 48.2% input reduction on its fixed fixture. It does not establish a population
effect, expose hidden reasoning or report billed quota. The immutable
[manifest](../tests/agent-eval/results/2026-09-17-matched-context-acceptance/manifest.json) contains
the exact environment and score.

A separate accepted pair performs one real source-writing task. Both agents change the same Rust
scalar and pass 33 compiled behavior cases without failed calls, direct commands or correction.
The SDK agent uses two exposed calls and 41,389 input tokens. Its retained review produces one
basis-bound write, eight passing check/apply/undo/redo/patch stages and the expected patch. The
direct-file agent uses four exposed calls and 67,985 input tokens. The observed input difference is
-26,596 tokens, or 39.1% fewer for the SDK arm, on this one fixed task. The
[source-writing manifest](../tests/agent-eval/results/2026-09-18-guided-delivery-acceptance/manifest.json)
retains the prompts, requests, responses, Codex settings and usage.

The source-writing pair was repeated after SDK packaging changed the runtime. The accepted repeat
again used Codex CLI 0.154.0, `gpt-5.6-luna` and low effort. Both arms passed the same exact-source
and 33-case compiler oracle with no failed commands or human correction. The SDK arm used two
exposed calls and 63,714 input tokens; the direct-file arm used four calls and 68,098 input tokens.
The observed difference was -4,384 input tokens on this repeat. The
[repeat manifest](../tests/agent-eval/results/2026-09-18-sdk-release-acceptance/manifest.json) is
bound to the new SDK runtime. Its retained source snapshots preserve the exact evaluator and guide
bytes used in that run, so the historical result stays auditable as the guide gains new routes.

The [representative registry](../tests/agent-eval/representative-acceptance.json) joins those two
live matched cohorts to six executable deterministic cases and ten live trials for a pinned
unfamiliar upstream workspace, Rust multi-file delivery, TSX/React body changes,
CSS/Tailwind/Mermaid surfaces, backend migration and agent-authored Lean tactics. It records each
fixture revision, independent oracle and exact postconditions. Audit metadata and real execution
remain separate: replay runs one worker and reports missing toolchains as infrastructure failures
rather than product failures.

The first accepted guided upstream read task uses the pinned `rust-lang/regex` revision
`2b527599eb9eea0dcc288c704584f242f26a5c61`. A fresh Codex CLI 0.155.1 agent on
`gpt-5.6-luna` at low effort followed `understand` and `trace` guidance, made 12 instrumented
calls, revealed five source slices of at most 512 bytes each, and identified the exact
`regex::escape` to `regex_syntax::escape` edge. The retained independent source oracle checks
the delegate, append helper, metacharacter predicate and the explicit statement that static
source did not prove runtime behavior. The source bytes stayed unchanged; no direct project
commands, failed commands or human corrections occurred. The CLI reported 249,966 input tokens,
224,000 cached input tokens and 2,166 output tokens; billed quota was unavailable. This is one
read-only trial, not cross-language or source-writing acceptance. The
[accepted manifest](../tests/agent-eval/results/2026-09-20-upstream-read-acceptance/manifest.json)
binds the prompt, tool events, run settings and score. Four preceding diagnostics retain the
sandbox launch failure, malformed goal transport, overly narrow answer format and overly narrow
instrumented exploration boundary. The agent's successful trace still disclosed unrelated
`field-based` method candidates for unknown receiver types; that uncertainty is the existing B5
static-analysis boundary, not an exact call claim.

The first accepted guided multi-file write uses the same pinned regex revision. A fresh Codex CLI
0.155.1 agent on `gpt-5.6-luna` at low effort made five instrumented calls to guide, preview,
review, execute and finish a `regex_syntax::escape` to `quote_regex` rename. The complete review
changed the declaration, the CLI caller and the `regex::escape` facade's delegate. The public
facade kept its name and behavior. The upstream library tests, CLI build and minimal-feature build
passed at every applicable delivery stage; apply, undo, redo and patch delivery all passed. A
separate receiver applied the patch to the pinned archive and passed 64 compiled cases against an
independent character-escaping oracle. Only those three tracked source files changed. The agent
used five commands, no failed or direct project commands and no human correction. CLI usage was
83,757 input tokens, of which 68,096 were cached, and 1,095 output tokens; billed quota was
unavailable. This is one guided rename, not evidence for authored multi-file body edits or other
languages. The [accepted manifest](../tests/agent-eval/results/2026-09-20-upstream-rename-acceptance/manifest.json)
binds the run; the preceding diagnostic retains the outer-sandbox launch failure before any agent
turn.

The first accepted guided TSX/Tailwind write uses the MIT-licensed
`aulianza/vite-react-starter` revision `0633ab1ff90cd0a09b70c849718b9504500a7bd5`. A fresh
Codex CLI 0.155.1 agent on `gpt-5.6-luna` at low effort used six instrumented calls to select the
header's exact `text-lg` class capability, preview `text-xl`, review the complete one-file diff,
execute and finish. The pinned project's read-only TypeScript check passed at each applicable
stage; apply, undo, redo and patch delivery all passed. A separate receiver applied the reviewed
patch, built the project with its lockfile dependencies and found `.text-xl` in the generated
Tailwind CSS. The source oracle confirms that only that class token changed. The agent used six
commands, no failed or direct project commands and no human correction. CLI usage was 98,419 input
tokens, of which 88,320 were cached, and 995 output tokens; billed quota was unavailable. This
single TSX surface edit does not establish authored TSX body, standalone CSS or Mermaid delivery.
The [accepted manifest](../tests/agent-eval/results/2026-09-20-upstream-react-acceptance/manifest.json)
binds the run. A local rehearsal found that a production build writes generated files during a
guided check, which changes the source snapshot. The live workflow therefore uses the read-only
TypeScript check and keeps the production build in the independent receiver oracle.

The first accepted guided Mermaid write uses MIT-licensed `tomooda/Micromaid` at revision
`e6e49600ad1e86f1a5bb1e375049534e5a6e2961`. A fresh Codex CLI 0.155.1 agent on
`gpt-5.6-luna` at low effort made six instrumented calls. It followed the diagram guide, then
expanded the bounded surface listing from eight to 20 items to find the exact `E` node capability.
The preview and review changed four identifier occurrences to `EvalStep` in the first README
flowchart, while retaining its visible label and five edges. The pinned Mermaid parser accepted
both README diagrams at every applicable check stage; apply, undo, redo and patch delivery passed.
An independent receiver replayed the patch, verified exact source bytes and graph structure, and
parsed both diagrams. The agent made no failed or direct project commands and needed no correction.
CLI usage was 108,869 input tokens, including 96,512 cached, and 1,014 output tokens; billed quota
was unavailable. This single Mermaid node rename does not establish standalone CSS or authored TSX
body delivery. The [accepted manifest](../tests/agent-eval/results/2026-09-20-upstream-mermaid-acceptance/manifest.json)
binds the run. A preceding [diagnostic](../tests/agent-eval/results/2026-09-20-upstream-mermaid-diagnostic-1/manifest.json)
retains a passing edit whose parser lockfile contained local paths and failed `npm ci` in a fresh
directory. It is excluded from acceptance. The accepted repeat uses a clean portable lockfile.

The first accepted guided standalone CSS write uses the same pinned MIT React project as the TSX
trial. A fresh Codex CLI 0.155.1 agent on `gpt-5.6-luna` at low effort used six instrumented calls.
It selected the `read-the-docs` definition in `src/App.css`, previewed and reviewed its rename to
`resource-links`, then executed the unchanged review. The declared PostCSS check parsed the sheet
before and after the edit. All eight delivery stages passed. An independent receiver replayed the
patch, verified that only one selector token changed, built the project and found the new selector
in generated CSS with the old selector absent. The agent made no failed or direct project commands
and needed no correction. CLI usage was 99,404 input tokens, including 78,080 cached, and 932 output
tokens; billed quota was unavailable. The selector is not referenced by current JSX, so this trial
establishes CSS source delivery and bundling without a rendered UI behavior claim. Authored TSX
bodies remain open. The [accepted manifest](../tests/agent-eval/results/2026-09-20-upstream-css-acceptance/manifest.json)
binds the run.

The first accepted guided authored TSX body write also uses the pinned MIT React project. Its
original `semantic-body` guide refused the `Layout` component because it had no exact semantic IR
body. The new `source-body` route requires explicit source permission, reveals the bounded
declaration, and binds a complete agent-authored body to the existing reviewed task writer. A fresh
Codex CLI 0.155.1 agent on `gpt-5.6-luna` at low effort used six instrumented calls to add a
conditional accessible name to `Layout`'s `<main>`. The declared TypeScript check and all eight
delivery stages passed. A separate receiver replayed the reviewed patch, built the project and
rendered mobile, responsive and omitted-type layouts with the expected labels and classes. The
source oracle permits either exact position for the new attribute beside `className`; no other
source byte may change. The agent made no failed or direct project commands and needed no
correction. CLI usage was 96,815 input tokens, including 86,272 cached, and 1,099 output tokens;
billed quota was unavailable. The [accepted manifest](../tests/agent-eval/results/2026-09-21-upstream-tsx-body-acceptance/manifest.json)
binds this single-body result. The preceding [diagnostic](../tests/agent-eval/results/2026-09-21-upstream-tsx-body-diagnostic-1/manifest.json)
passed delivery and rendering, but a premature oracle required the attribute before `className`.
It is excluded from acceptance. The subsequent paired-body trial covers one two-file TSX change.

The first accepted guided multi-file authored body task reuses the same pinned MIT React archive.
The `source-bodies` goal selects `Layout` and `Header` as distinct declarations, reveals each under
the source limit and previews one batch. A fresh Codex CLI 0.155.1 agent on `gpt-5.6-luna` at low
effort submitted both complete bodies, reviewed the two-file diff and executed all eight checked
delivery stages. The receiver confirmed exact source insertions, patch replay, the TypeScript/Vite
build and both accessible landmark labels for mobile, responsive and omitted layout types. No
direct project commands, failed commands or human corrections occurred. The accepted run used
118,226 input tokens, including 101,376 cached, and 1,405 output tokens; billed quota was
unavailable. The [accepted manifest](../tests/agent-eval/results/2026-09-21-upstream-multibody-acceptance/manifest.json)
binds this one two-file result. The [diagnostic](../tests/agent-eval/results/2026-09-21-upstream-multibody-diagnostic-1/manifest.json)
records a refused attempt that included declaration signatures in body fragments. The guide now
names that boundary explicitly.

The first accepted guided cross-crate Rust body task reuses the pinned `rust-lang/regex` archive.
The `source-bodies` goal selects `regex_syntax::escape_into` and the public `regex::escape` facade
by exact file scope. A fresh Codex CLI 0.155.1 agent on `gpt-5.6-luna` at low effort revealed both
declarations, authored both complete bodies, reviewed one two-file diff and passed all eight
delivery stages with upstream and minimal-feature checks. A fresh receiver replayed the patch and
compiled offline. Its independent allocator oracle passed 64 behavior combinations, preserved
append semantics and observed one allocation for each nonempty facade case. The agent made seven
instrumented calls with no failures, direct project commands or human corrections. CLI usage was
164,475 input tokens, including 131,328 cached, and 1,485 output tokens; billed quota was
unavailable. The [accepted manifest](../tests/agent-eval/results/2026-09-21-upstream-cross-crate-bodies-acceptance/manifest.json)
binds this one result. Three preceding diagnostics passed delivery and the independent oracle but
were rejected by source predicates tied to harmless expression structure or local variable names.
Their frozen evaluators document why implementation-shaped source checks were replaced by the
compiled behavioral and allocation boundary.

The first accepted guided application migration uses a pinned Express route with validated path,
query and JSON body inputs. A fresh Codex CLI 0.155.1 agent on `gpt-5.6-luna` at low effort used
five instrumented calls to select the source-free application IR route, preview a Go HTTP adapter,
review the complete creation patch and execute all eight delivery stages. The Express source stays
unchanged for coexistence. A fresh receiver replayed the patch and passed seven real HTTP cases,
including canonical accepted values and five validation refusals. The agent made no failed or
direct project commands and needed no correction. CLI usage was 91,325 input tokens, including
62,976 cached, and 1,062 output tokens; billed quota was unavailable. The trial exposed a Python
SDK verifier omission for the writable `application-migration` union variant; the regression now
drives the real guide, review and execution path. The
[accepted manifest](../tests/agent-eval/results/2026-09-21-application-migration-acceptance/manifest.json)
binds this result.

The first accepted guided proof-authoring task uses a generated Lean 4.28 package with one
source-anchored identity theorem and explicit proof debt. A fresh Codex CLI 0.155.1 agent on
`gpt-5.6-luna` at low effort used seven instrumented calls to select the source-free proof route,
inspect the exact theorem and contract, author and check `rfl`, preview the proof-region patch,
review it and execute all eight delivery stages. A fresh receiver replayed the patch and strict
`fr spec verify` reported one fresh source anchor, zero obligations, zero debts and a successful
Lean build. The source stayed unchanged, the agent made no failed or direct project commands and
needed no correction. CLI usage was 116,527 input tokens, including 104,448 cached, and 1,031
output tokens; billed quota was unavailable. The
[accepted manifest](../tests/agent-eval/results/2026-09-21-proof-authoring-acceptance/manifest.json)
binds the evaluator, transcript and result.

## Find the underlying evidence

| Topic | Retained data or evaluator |
|---|---|
| Completion workflows | [`completion-workflows.json`](../tests/agent-eval/completion-workflows.json), `tools/completion-workflows.py` |
| Guided live-agent completion | `tools/completion-agent-eval.py`, `tests/agent-eval/results/2026-09-17-completion-acceptance/` |
| Matched SDK and direct-file context | `tools/matched-agent-context.py`, `tests/agent-eval/results/2026-09-17-matched-context-acceptance/` |
| Matched SDK and direct-file source writing | `tools/matched-agent-source.py`, `tests/agent-eval/results/2026-09-18-guided-delivery-acceptance/` |
| Representative cross-language acceptance | `tools/representative-acceptance.py`, `tests/agent-eval/representative-acceptance.json` |
| Pinned guided upstream read/trace | `tools/upstream-read-agent.py`, `tests/agent-eval/results/2026-09-20-upstream-read-acceptance/` |
| Pinned guided upstream multi-file rename | `tools/upstream-rename-agent.py`, `tests/agent-eval/results/2026-09-20-upstream-rename-acceptance/` |
| Pinned guided cross-crate Rust bodies | `tools/upstream-cross-crate-bodies-agent.py`, `tests/agent-eval/results/2026-09-21-upstream-cross-crate-bodies-acceptance/` |
| Guided Express-to-Go application migration | `tools/application-migration-agent.py`, `tests/agent-eval/results/2026-09-21-application-migration-acceptance/` |
| Guided Lean proof authoring | `tools/proof-authoring-agent.py`, `tests/agent-eval/results/2026-09-21-proof-authoring-acceptance/` |
| Pinned guided React/Tailwind surface edit | `tools/upstream-react-agent.py`, `tests/agent-eval/results/2026-09-20-upstream-react-acceptance/` |
| Pinned guided standalone CSS edit | `tools/upstream-css-agent.py`, `tests/agent-eval/results/2026-09-20-upstream-css-acceptance/` |
| Pinned guided authored TSX body | `tools/upstream-tsx-body-agent.py`, `tests/agent-eval/results/2026-09-21-upstream-tsx-body-acceptance/` |
| Pinned guided paired TSX bodies | `tools/upstream-multibody-agent.py`, `tests/agent-eval/results/2026-09-21-upstream-multibody-acceptance/` |
| Pinned guided Mermaid node edit | `tools/upstream-mermaid-agent.py`, `tests/agent-eval/results/2026-09-20-upstream-mermaid-acceptance/` |
| Agent skill reading | `tests/agent-eval/skill-context.json`, `tools/skill-context.py` |
| Compact context protocols | `tests/agent-eval/context-protocol.json`, `tests/agent-eval/context-protocol-v3.json` |
| Semantic bodies, deltas and intents | `tests/agent-eval/semantic-*.json`, `tools/semantic-*.py` |
| Workflow and task packets | `tests/agent-eval/workflow-context.json`, `tests/agent-eval/task-change-context.json` |
| Progressive disclosure | `tests/agent-eval/progressive-disclosure.json`, `tests/agent-eval/progressive-evidence.json` |
| Project caches and construction | `tests/agent-eval/project-*.json`, `tools/project-*.py` |
| External source cohorts | [`PROVENANCE.md`](../tests/agent-eval/PROVENANCE.md), `tests/agent-eval/results/` |

Paths in this table identify families. The manifest for a claimed run is the authority for its
exact files and digests.

## Reproduce and audit

Deterministic reports can regenerate locally. Live Codex runs spend quota and require the evaluator's
explicit confirmation flag. Replay and score retained evidence before considering a fresh run.

```sh
cargo build --features cli
python3 tools/completion-workflows.py --fr target/debug/fr --output /tmp/completion.json
python3 tools/completion-workflows.py --audit /tmp/completion.json
python3 tools/matched-agent-context.py replay tests/agent-eval/results/2026-09-17-matched-context-acceptance
python3 tools/matched-agent-source.py audit tests/agent-eval/results/2026-09-18-sdk-release-acceptance
python3 tools/representative-acceptance.py audit
python3 tools/representative-acceptance.py replay
python3 tools/upstream-read-agent.py audit tests/agent-eval/results/2026-09-20-upstream-read-acceptance
```

Use the [Codex runner guide](agent-codex-runner.md) for live-run isolation and the current economical
model configuration. Treat token, byte and call counts as properties of the retained run. Recompute
them after changing prompts, skills, fixtures, model settings or tool output.

## Deterministic investigation acceptance

The [2026-09-23 investigation manifest](../tests/agent-eval/results/2026-09-23-investigation-acceptance/manifest.json)
pins the evaluator and fixture at revision `27a33953`. A symptom-driven semantic query discovers
the negative-total repair target. The evaluator reviews the repair and quote insertion separately,
executes reversal and redo, and replays both patches in a fresh receiver. Six checkout cases and
two quote cases pass their independent Python oracle. This is deterministic workflow evidence;
it does not measure live-agent investigation success.

Cold/warm and single-edit/clean-rebuild relationship reports agree on this small fixture. The
[retained result](../tests/agent-eval/results/2026-09-23-investigation-acceptance/result.json) records
latency and context bytes; memory and token measurements remain unavailable. The implementation
[diagnostics](../tests/agent-eval/investigation/diagnostics.json) retain failed attempts, including
a negative nonmember input that exposed an incomplete first repair.

Reproduce with `python3 tools/investigation-acceptance.py --fr target/debug/fr --output /tmp/investigation`.
The [investigation contract](agent-investigations.md) states the analysis, persistence and proof boundaries.

## Fixed-point and reuse acceptance

The [flow corpus](../tests/agent-eval/flow-fixed-point/task.json) pins a source that reaches its sink
only after several loop iterations. An independent Python execution checks that case, overwrites,
breaks, continues and raises. Native tests also cover unbound names, shadowing, unsupported effects,
budgets and tampered retained reports. SDK tests compare reused results with clean analysis after
unrelated edits and force rebuilds after source, rule, configuration, budget or context changes.

Run `python3 tools/flow-acceptance.py --output /tmp/flow-result.json`, then audit with
`python3 tools/flow-acceptance.py --audit /tmp/flow-result.json`. The evaluator binds its sources,
binary and repository revision. Isolated workers measure wall time, peak child/worker RSS, native
response bytes and executed transfer steps. Tokens remain unavailable; this is not a live-agent trial.
Whole-file reuse is opt-in because small analyses can cost less than restoration and validation.
The [retained result](../tests/agent-eval/results/2026-09-23-flow-fixed-point/result.json) records the
pinned run. [Implementation diagnostics](../tests/agent-eval/flow-fixed-point/diagnostics.json) retain
the context-envelope refusal and the observed cache-overhead boundary.

## Semantic provenance and checked plans

The [semantic evidence task](../tests/agent-eval/semantic-evidence/task.json) pins repeated Unicode
calls, an augmented assignment and a synthesized null. An independent Python AST oracle supplies
coordinates and executes finite behavior cases. Native and SDK tests cover normalized and shadowed
bodies, absent origins, page truncation, stale cursors and cross-revision link refusal.

`tools/semantic-evidence-acceptance.py` compares ordinary AST/source inspection, the existing semantic
plus source-reveal workflow, and paged origin lookup. It records raw report bytes and local latency.
The two native routes use the same current binary. This is a workflow comparison without a live agent.
The result also retains passing and failing check reports, verified Merkle roots, completed plans
and invalidation after a source edit. Executable hashes identify commands; their outputs retain
explicit coverage. Neither implies complete compiler semantics or trusted execution attestation.
Run with `--output FILE`, then use `--audit FILE` to validate its source bindings and retained records.
The [retained result](../tests/agent-eval/results/2026-09-23-semantic-evidence/result.json) binds the
implementation, evaluator, fixture, independent coordinates and command outcomes to the recorded run.

## Recursive scalar flow

The [2026-09-24 recursive flow run](../tests/agent-eval/results/2026-09-24-recursive-flow/result.json)
compares ten pinned functions with the previous incomplete recursive analysis. An independent Python
oracle executes 44 cases and derives call coordinates from the AST. The new summary mode completes
all eight admitted cases. Unknown external calls and aliases remain incomplete. The record retains
return, sink and exceptional effects, input identities and exact witnesses with zero false claims
within this finite corpus.

Six isolated workers compare cold, warm, unrelated-edit and helper-edit results against clean analysis.
They record latency, response bytes, native work and peak child/worker RSS. Reuse validates the whole
file; these measurements do not establish finer cache granularity or a general speed improvement.
This is deterministic analysis evidence, not a live-agent investigation trial or a security proof.

## Bounded flow fact acceptance

The [2026-09-24 fact acceptance](../tests/agent-eval/results/2026-09-24-flow-facts/result.json)
retains two sink witnesses from a recursive Python fixture. An independent oracle executes nine
cases and checks UTF-8 AST coordinates. Eleven followed semantic links match exact occurrences;
negative, normalized, unknown and stale cases retain their distinct outcomes.

The first two fact headers use 5,421 bytes, compared with 56,565 bytes for the raw analysis.
All eight headers require four calls and 20,744 bytes. Both complete explanations require seven
more calls and 55,461 bytes, plus eleven separate semantic validation calls. Full provenance costs
more disclosure than the raw flow report. These fixture measurements support bounded initial
discovery, not a general latency or token claim. Source reveals are zero and tokens are unavailable.
The artifact retains the baseline, implementation bindings, paged evidence, semantic replies and
implementation diagnostics. Native tests separately exercise semantic query and origin-page limits.

## Compiler evidence acceptance

The [2026-09-24 compiler acceptance](../tests/agent-eval/results/2026-09-24-compiler-evidence/result.json)
retains real rustc and Cargo diagnostics for one Rust fixture. Default compilation passes; strict
configuration reports E0308 at the independently checked byte span. Six executable cases check default
behavior. The artifact preserves command outcomes, syntax acceptance, plan attachment and rejected
reuse after source, environment, external identity and newly added Cargo configuration changes.

The adapter adds bounded, typed diagnostic disclosure and input validation. It does not establish a
context or latency advantage: the small default check has no diagnostics, while each fact page repeats
its input scope. Measurements retain raw report bytes, page bytes, calls and elapsed time separately.
Declared compiler driver hashes contribute validation cost. Full compiler dependency discovery and
source proofs remain open. Synthetic malformed-protocol tests remain separate from real compiler evidence.

## Durable target resumption

The [correspondence fixture](../tests/agent-eval/resumable-correspondence/task.json) pins a Unicode declaration and an independent AST oracle.
The [baseline](../tests/agent-eval/resumable-correspondence/baseline.json) retains a changed original that the old matcher hid behind its unchanged copy.
The [evaluator](../tools/correspondence-acceptance.py) retains moves, identifier renames, body edits, copies, duplicates and deletion.
The [result](../tests/agent-eval/results/2026-09-24-resumable-correspondence/result.json) includes interrupted plan resumption and an independent satisfied step.
Explicit refresh clears stale evidence and actions; the old mutation review refuses.
A fresh review delivers a checked edit, reversal and patch. An independent receiver applies the patch and checks three finite outputs.

The evaluator records cold, warm and edited identity queries with latency, context bytes and isolated peak RSS.
Warm and clean reports agree. Every query recomputes correspondence; this slice adds no finer analysis cache.
The ordinary AST baseline reads the whole tiny source and excludes a native process launch.
These are deterministic fixture measurements, with no live-agent, token, semantic equivalence or general speed claim.
Audit the retained result with `python3 tools/correspondence-acceptance.py --audit RESULT`.

## Retained proof evidence

The [proof fixture](../tests/agent-eval/retained-proofs/task.json) pins an unchecked-module baseline and a finite Boolean source task.
Run `python3 tools/proof-evidence-acceptance.py --output PATH` to retain current execution evidence.
Run the same script with `--audit PATH` to check its source bindings, Merkle roots, invalidation outcomes and patch replay.
The evaluator retains old and fresh model reports, seven input changes, independent observations and stale review refusals.
A separate Python oracle checks both Boolean inputs after applying the retained patch in an independent directory.
The [current result](../tests/agent-eval/results/2026-09-24-retained-proofs/result.json) records its fixture timing and context bytes.
These deterministic results make no live-agent, general performance or source correspondence proof claim.

## Imported scalar flow

The [import fixture](../tests/agent-eval/imported-flow/task.json) pins transitive return and sink effects across five root-local Python modules.
Its independent runtime oracle executes ten source/sink cases in two sanitizer contexts. Python ASTs supply separate UTF-8 call coordinates.
The [evaluator](../tools/imported-flow-acceptance.py) records missing modules and members, package/stub conflicts, configuration changes and rule changes.
Retained plans invalidate dependent conclusions while preserving independent observations. Paged facts link witnesses back to each helper's syntax.

The [result](../tests/agent-eval/results/2026-09-25-imported-flow/result.json) includes cold, warm, unrelated-edit and helper-edit comparisons against clean analysis.
Isolated workers record latency, context bytes, transfer work and peak child/worker RSS. Whole-module closure reuse remains deliberately conservative.
The ordinary arm executes the runtime/AST oracle; it does not model an agent's search effort. Tokens and live-agent outcomes remain unavailable.
A reviewed helper-body change passes checks, apply, undo, redo and patch delivery. An independent receiver replays the patch and repeats the runtime oracle.
Run `python3 tools/imported-flow-acceptance.py --audit RESULT` to validate retained source bindings, outcomes and replay.

## Native host recovery

The [pinned task](../tests/agent-eval/host-recovery/task.json) starts from 22 passing history tests.
The [result](../tests/agent-eval/results/2026-09-25-host-recovery/result.json) retains 430 handled faults
and 430 process exits across apply, undo, redo and recovery. Seven focused tests include staged
content, mode and symlink changes, conflicting recovery and compound failures. Filesystem byte,
mode, kind and absence checks use direct host reads against independently specified fixtures.
Each fault case reopens the journal and resumes pending work before checking the expected state.

A separate Rust/Lean comparison checks 224 outputs for publication and recovery decisions.
The [contract](host-recovery.md) distinguishes model theorems, tested agreement and trusted host operations.
This is deterministic fault evidence. It does not measure agent tokens, task latency benefits or power-loss recovery.
The retained baseline guide and disclosure use a one-file scan with 1,935 unresolved references;
they support local discovery without establishing repository-wide absence claims.

Reproduce the evidence with:

```sh
python3 tools/host-recovery-acceptance.py --output /tmp/host-recovery.json
python3 tools/host-recovery-acceptance.py --audit /tmp/host-recovery.json
```
