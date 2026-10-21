# What our tests and experiments tell us

Use this page to check the evidence behind a claim about `fr`. A scripted repair can test editing and recovery, while a live comparison asks whether an agent benefits.
For example, fewer report bytes can help a conversation, but they do not measure the total cost of fixing the bug. Read each result with its stated limits.

A fixture is a chosen test input. An evaluator runs a test procedure, and an oracle supplies an independent expected answer.
An attempt records one run. An arm is one compared tool setup; a cohort is the chosen group of attempts.
A manifest records input and output identities so another reader can verify which run a result describes.
For “does fr make repairs cheaper?”, the baseline must solve the same task with ordinary tools.
The [measurement guide](benchmarks.md) distinguishes that comparison from reply-size and tool-runtime measurements.

## Recorded experiments and their limits

The [two-question Python lesson](check-python-together.md) prepares shared startup once within one request.
For example, the same import exposes a helper passing `17` along and another returning `0`.
Forty-one new checks exercise the Python interface.
The behavior checks compare shared answers with separate analyses and ordinary Python results.
They cover changed files and import candidates, unknown startup effects, shared work limits and bounded location detail.
An unchecked write after the displayed locations must still leave both answers unfinished.
These checks establish agreement on the chosen examples; they measure no speed or agent savings.
Saved answers also need to change when the checking program changes.
For example, fixing how the checker reads Python must invalidate an older answer, even if the user's files stay the same.
A shared source-input list now covers the Rust checking code, compiled grammars, queries, dependency files and build settings.
The checker hashes these inputs when building; the result collectors record the same source files.
Tests change a rule, generated parser, scanner, query, dependency and build setting, and require a changed cache identity.
Missing declared inputs stop the build or collection. The tests also check added, removed and renamed source files.
Saving startup work for a later request remains open. The existing package questions remain unfinished.


The [Python package startup investigation](python-package-startup-results.md) observes what runs before four simple library helpers become available.
Three hosted captures each completed sixteen fresh processes and forty-eight integer calls, with ordinary and instrumented results agreeing.
The existing package answers remain incomplete: Toolz still runs an unchecked table builder, while the observed Funcy and Pydash source exceeds current single-analysis budgets.
The [method and runnable example](python-package-startup.md) explain import-time replacement, observation limits and the ordinary-Python timing baseline.
The first two captures included module-list copying in their internal helper-call timers.
The final capture fixes that measurement and preserves import-time file names before the calls can change them.
Sixteen controls also check first-failure preservation and strict saved-record validation.
All three raw captures remain saved. This work adds no analyzer behavior and establishes no agent savings.

Evaluation artifacts answer narrow questions about fixed revisions and fixtures. They are retained
evidence, not timeless product documentation. Live support comes from `fr audit`, `fr capabilities`
and the command schemas.

## Evidence classes

Use the [independent study planner and auditor](agent-study.md) for new matched comparisons.
It records planned cells, failed outcomes and complete declared agent usage without invoking models.
Its offline tests are infrastructure checks, not evidence of live-agent savings.

| Kind of evidence | Example question it can answer | What remains open |
| --- | --- | --- |
| Scripted workflow | “Can this known repair be applied, undone and exported?” | Whether an agent can discover the repair |
| Live agent attempt | “Did the agent solve this task with this setup?” | Whether that result repeats on other tasks |
| Paired comparison | “How did ordinary tools and fr perform on the same requested task?” | General savings beyond the measured attempts |
| Model proof | “Does the stated rule hold under these definitions and assumptions?” | Its connection to the actual implementation |
| Compiler or behavior check | “Did these commands and tested inputs pass?” | Untested inputs and undeclared dependencies |

For example, a scripted checkout patch may pass every delivery check because the script already
knows the edit. To learn whether `fr` helps an agent find it, compare discovery on an unfamiliar
task and count the whole attempt. A smaller caller report alone cannot answer that question.

Every retained run has a manifest under `tests/agent-eval/results/`. Manifests bind inputs,
evaluators, binaries and reports by digest. Diagnostic runs remain diagnostics after later fixes;
acceptance runs must satisfy their scorer at recording time.

## Retained Boolean model comparisons

The [current comparison](../tests/agent-eval/results/2026-10-21-distinct-release-delivery-model-comparisons/manifest.json)
checks a Rust function and its Python translation with three true-or-false inputs.
For example, renaming the Rust function and removing an unused argument must preserve its answer
for all eight input combinations. A separate test compiles and runs the Rust code and executes Python.
Both the ordinary edit and the edit made with `fr` must pass, undo exactly and work again in another checkout.

Lean is a proof checker. Here it checks three rules about simplified descriptions of the functions:
the Rust edit preserves answers, the Python translation agrees when arguments are reordered,
and a stricter acceptance rule never accepts something the original rule rejects.
The run retains an incorrect argument map that fails proof checking. Fresh processes restore the
snapshot and completed plan; changed source invalidates the model evidence. Receiver audits check
the retained models and proofs again. These are generated-model theorems, not source implementation
proofs, general translation correctness or a live-agent comparison.
The [September record](../tests/agent-eval/results/2026-09-28-virtual-model-comparisons/manifest.json) remains unchanged as history.

The [virtual workspace regressions](../tests/virtual_model_workspaces.rs) check byte limits,
UTF-8 (the text encoding used to count source bytes) errors, scoped restoration and native/virtual agreement between saved file states. Private model regressions
also reject forged models, stale source and altered generated context. Together with the VFS
architectural checks, the focused runner executes 13 regressions. The
[dogfood record](../tests/agent-eval/results/2026-09-28-virtual-dogfood/manifest.json) retains
this task's fr authoring receipts and 15 syntax-only translation preview outcomes.

Six reports tied to recorded source inputs are refreshed on GitHub: retained proofs, agent guidance, completion
workflows, intent actions, index resolution and host recovery. Their platform and timing fields
identify the remote runner; they should not be read as new measurements of this Mac.
The [dogfood record](../tests/agent-eval/results/2026-09-28-refinement-dogfood/manifest.json)
retains actual author previews, saved plans and applied repository edits, including the fixes for
unused formal parameters and retained source anchors. Direct-edit boundaries are explicit.

## Consumer scope and delivery

The [consumer-scope evaluation](../tests/agent-eval/results/2026-10-21-distinct-release-change-scope/manifest.json)
pins a four-file Python caller chain. Independent inspection of the parsed syntax tree and native discovery agree on all
three direct call sites. Both prescribed editing arms repair subtotal and add the API fee, then
pass 147 public results plus the discovered test. Separate receivers apply, reverse and reapply
each patch with exact source checks.

An incomplete scope refuses binding. A successful delivery cannot renew its stale discovery
prerequisite: the evaluator explicitly rediscovers before completing the outcome step. A fresh
process restores the completed plan; an added consumer then invalidates discovery and outcome while
preserving an independent observation. New consumers without check associations prevent readiness.
These scripted cases do not measure a live agent, token savings or runtime coverage.

The [repository dogfood record](../tests/agent-eval/results/2026-09-28-change-scope-dogfood/manifest.json)
retains 21 applied author batches on actual implementation, SDK, evaluator and test files.
It also retains bounded discovery against `src/`, the invalid Python indentation preview found
during editing, and its refusal after the parser fix. Reports identify intermediate source revisions;
the manifest distinguishes admitted edits from direct-edit boundaries.

## Checked delivery outcomes

The [current delivery evaluation](../tests/agent-eval/results/2026-10-21-distinct-release-delivery-checked-outcomes/manifest.json)
asks for a subtotal repair and a fee in two Rust files. A separate Python test compiles and runs
the public Rust functions with 49 input combinations. The script supplies each proposed edit;
it does not test an agent discovering the repair or establish savings in model usage.

| Case | Lifecycle checks | Requested behavior | Patch | Plan completion |
|---|---|---|---|---|
| Wrong change, compiler-only delivery | Pass | Fail | Delivered | Not established |
| Same wrong change, acceptance checks | Pass through apply | Fail | Withheld | Incomplete |
| Correct change, acceptance checks | Pass, including reversal | Pass after apply and redo | Delivered | Complete |

The accepted receipt records the checks and their results. It can be reopened in a fresh process
and used to complete the saved task. A later source edit
invalidates the outcome step while preserving an independent documentation observation. Separate
receivers replay both exported patches, reproduce their respective behavioral outcomes, reverse
exactly and reapply. Failed receipts remain retained beside the successful result.
The [September record](../tests/agent-eval/results/2026-09-28-checked-outcomes/manifest.json) remains unchanged as history.

[SDK and native tests](checked-outcomes.md#evidence) add configuration, checker and environment drift,
source-writing checks, redo failures, malformed receipts and stale prerequisites. A successful
delivery cannot refresh a changed prerequisite to force plan completion. The stage model admits
acceptance checks only after application; shared Rust/Lean cases cover that finite policy.
These checks establish the declared outcomes, not general behavioral correctness or source equivalence.

Eleven evaluations tied to recorded source inputs also rerun for this change: package flow, retained proofs, host recovery,
resolution, flow cache, workflow context, task-change context, completion workflows, agent guides
intent actions and SDK runtime context. Regression tests select the 2026-09-28 results.
Earlier dated measurements remain unchanged as historical evidence. Host recovery now measures with
one test worker, matching the serial compiler and Lean limits used for this validation.

## Unknown-target investigations

The [cohort](../tests/agent-eval/unknown-target/cohort.json) compares live discovery and authoring on
unmodified pinned strsim and regex sources. Prompts supply a Unicode bug symptom or cross-crate API
requirements, without edit paths. Each arm uses two fresh Codex processes, separated by an inert
source edit that invalidates the first review. Both arms automate compiler checks, exact reversal,
reapplication and independent patch application and checks in another checkout. The native plan retains an independent observation
while invalidating its diagnosis; final delivery has a separate grader.

| Task | Arm | Behavioral outcome | Calls | Visible bytes | Input tokens | Agent seconds |
|---|---|---|---:|---:|---:|---:|
| Unicode Dice | ordinary files | Pass | 17 | 27,604 | 326,494 | 126.8 |
| Unicode Dice | fr | Pass | 23 | 75,165 | 618,428 | 185.6 |
| Escape length/allocation | ordinary files | Fail: two escaping allocations | 22 | 59,707 | 644,478 | 223.9 |
| Escape length/allocation | fr | Pass | 33 | 70,857 | 763,904 | 307.0 |

These runs use Codex CLI 0.157.1, `gpt-5.6-luna`, low effort and the default service tier.
Input counts include cached tokens. Agent time sums both phases, including their tool work;
controller preparation and independent scoring are outside that time. Sampled process-group peaks
were 227–232 MiB. Sampling may miss peaks and descendants in other groups; it is not whole-tree RSS (resident set size, the measured memory held by a process).
The auditor prints cached/output/reasoning usage, requests, source reads and refusals as well.

Both bug patches pass 67,081 independent string pairs. The feature oracle checks 1,060 byte and
allocation cases across both crates. The ordinary feature trial completed the delivery protocol
and compiled, but its patch omitted the escaping allocation fix. Project and receiver agree on that
failure. It remains a comparison outcome and was not retried until success. The same trial attempted
one unsupported source-equivalence claim; the finish boundary refused it before accepting a corrected
statement. No trial establishes source equivalence.

Three additional attempts remain diagnostic: an extra heredoc terminator, an asynchronous tool-log
race, and a discovery process stopped before delivery while that shared race was fixed. The race
fix serializes requests within each session. Both feature arms then used the corrected protocol.
Seven live attempts are retained in total; deterministic rehearsals are separate.

The native arm used more calls, context and time on both tasks. It succeeded on the allocation
requirement where this ordinary-file attempt failed. One pair per task cannot establish a general
success or efficiency effect. See the [contract](unknown-target-investigations.md) for disclosure
budgets, cooperative command isolation and the distinction between plan evidence and behavioral checks.
`python3 tools/investigation-cohort.py --replay` checks all four patches, including reproduction of
the recorded behavioral failure. It makes no new model calls.

## Package investigation and delivery

The [package task](../tests/agent-eval/package-flow/task.json) pins a raw-input bug and a sink-free preview requirement without edit paths.
Its [baseline](../tests/agent-eval/package-flow/baseline.json) records incomplete dotted-import analysis before this extension.
The [acceptance](../tests/agent-eval/results/2026-09-26-package-flow/result.json) discovers the faulty helper through summaries,
keeps same-named package helpers separate and delivers both changes through reviewed checks, undo/redo and patches.
An independent receiver replays both patches and executes the public API. Python AST positions check cross-file call origins.
Restored plans invalidate dependent evidence after parent, helper and negative-candidate edits while preserving an independent step.

The ordinary arm discovers targets with inspection of the parsed syntax tree and edits source directly. The fr arm uses summaries and import records.
On this tiny fixture, ordinary discovery and editing read 417 source bytes; fr discovery disclosed roughly 103 KiB of structured reports.
The ordinary arm was faster in the retained run. These arms perform different operations and make no general performance claim.
Both are deterministic scripts, with no live-agent or token-saving claim. Declared delivery checks admit valid before/after states for reversal;
strict runtime checks enforce the repaired and feature required conditions after the edit after delivery and in the separate receiver.
The [re-export task](../tests/agent-eval/package-reexports/task.json) extends this corpus to explicit function aliases in package initializers.
Its [acceptance](../tests/agent-eval/results/2026-09-28-reexports-package-reexports/result.json) repeats diagnosis, dependency invalidation, repair and feature delivery.
The [baseline](../tests/agent-eval/package-reexports/baseline.json) records the existing local binary refusal and independent Python behavior.
That binary predates the pinned source revision; its identity is retained explicitly.
Package loading remains bounded to regular local packages with explicit local imports and inert declarations.
The [module-alias task](../tests/agent-eval/module-aliases/task.json) adds child-module fallback and renamed module re-exports (making an imported name available from another module).
Its baseline records CPython output and refusal by the existing local 0.35.0 binary. That binary
was not built from the pinned task revision. The evaluator repeats discovered repair, feature
delivery, dependency resumption and independent patch replay for the new import form.
The [retained run](../tests/agent-eval/results/2026-09-29-aliases-module-aliases/result.json) passes both outcomes and independent patch application and checks in another checkout.
The ordinary arm reads 413 source bytes across six files. Native discovery returns 110,505 bytes
across eight calls with no source-body reveals. These finite script measurements make no token-saving
or general efficiency claim.
That corpus excludes dynamic imports, namespace packages and initialization effects;
[single-root namespace coverage](namespace-packages.md) extends the later import contract.

## Current matched results

“Matched” means each comparison holds its declared task and required result fixed.
It does not mean all the experiments below have the same baseline or count the same costs.
The cache table compares two storage encodings on warm runs; later agent trials compare complete task attempts with different tools.
Read their setup and units separately. The [measurement guide](benchmarks.md) collects those boundaries and known overheads.

The [flow-cache comparison](../tests/agent-eval/results/2026-09-25-flow-cache/result.json.gz) records the September 25 implementation and contains 288 samples across six generated scalar workloads.
Pipelines, imported fanout and mutual recursion each have two sizes. Three repetitions rotate the
previous SDK cache and bounded report chunks through cold, warm and six single-edit scenarios.
Every retained answer agrees with fresh analysis. Independent Python execution, source digests and
syntax-tree source positions check the scenario; missing helpers and response-budget cutoffs remain incomplete.

| Workload | Previous warm median | Report-chunk warm median |
|---|---:|---:|
| Pipeline, 8 helpers | 1.374 s | 0.482 s |
| Pipeline, 16 helpers | 3.181 s | 0.709 s |
| Imported fanout, 8 helpers | 0.657 s | 0.413 s |
| Imported fanout, 24 helpers | 1.357 s | 0.504 s |
| Mutual recursion, 8 helpers | 1.537 s | 0.496 s |
| Mutual recursion, 12 helpers | 2.515 s | 0.599 s |

This selects whole-report storage while retaining whole-analysis dependency validation. Warmed
stores use 1.34–2.90 times the bytes of the previous per-node encoding. Native calls and disclosed
context stay unchanged. The run records isolated worker and child peak RSS, transfer counts,
store operations and bytes. Native fact caching is disabled; operating-system caches are uncontrolled.
The corpus represents these admitted task shapes on one host, without a production or population speed claim.
It contains no live-agent trial or token measurement. The pinned baseline SDK and diagnostic probes
are retained beside the [task](../tests/agent-eval/flow-cache/task.json).
New warm medians still exceed new cold medians on all six workloads. The result improves retained
storage overhead; it does not establish an overall latency benefit from enabling the optional cache.

Those historical medians exclude Python process startup. New collections also
record `process_seconds`: the time to start the Python worker, import its code,
perform the analysis, write its answer and exit. `elapsed_seconds` covers the
analysis and storage work measured inside that worker. The reports keep both
intervals and their native-analysis and storage components.
The whole-process interval also includes this benchmark's file counts and
metric collection. Saving more files makes that bookkeeping slower too, so
the difference between the two intervals cannot isolate Python startup cost.

Both cache arms run the same `fr` analyzer. Ordinary Python executions check
the answers; this benchmark does not time them as an alternative to `fr`.
The older cache stays pinned to its original source and API. The current cache
also checks initialization and model settings, so new timings compare two
complete cache implementations. They cannot isolate the effect of storage
layout or establish the language's overhead relative to Python execution.

Evidence identities ignore coordinated workspace release versions in the Cargo manifest and lockfile.
Dependency versions, features, paths and source changes remain bound.
The evaluator and normalization helper are also bound.

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
live matched cohorts to eight executable scripted cases and ten live trials for a pinned
unfamiliar upstream workspace, Rust multi-file and structural delivery, TSX/React body changes,
CSS/Tailwind/Mermaid surfaces, backend migration and agent-authored Lean tactics. It records each
fixture revision, independent oracle and exact required conditions after the edit. Audit metadata and real execution
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
five instrumented calls to select the application IR route without source text, preview a Go HTTP adapter,
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
`gpt-5.6-luna` at low effort used seven instrumented calls to select the proof route without source text,
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
breaks, continues and raises. Native tests also cover unbound names, shadowing (a nearer declaration hiding an earlier name), unsupported effects,
budgets and tampered retained reports. SDK tests compare reused results with fresh analysis after
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
plus source-reading workflow, and paged origin lookup. It records raw report bytes and local latency.
The two native routes use the same current binary. This is a workflow comparison without a live agent.
The result also retains passing and failing check reports, verified Merkle roots, completed plans
and invalidation after a source edit. Executable hashes identify commands; their outputs retain
explicit coverage. Neither implies complete compiler semantics or trusted execution attestation.
Run with `--output FILE`, then use `--audit FILE` to validate its recorded input fingerprints and retained records.
The [retained result](../tests/agent-eval/results/2026-09-23-semantic-evidence/result.json) binds the
implementation, evaluator, fixture, independent coordinates and command outcomes to the recorded run.

## Recursive scalar flow

The [2026-09-24 recursive flow run](../tests/agent-eval/results/2026-09-24-recursive-flow/result.json)
compares ten pinned functions with the previous incomplete recursive analysis. An independent Python
oracle executes 44 cases and derives call coordinates from the AST. The new summary mode completes
all eight admitted cases. Unknown external calls and aliases remain incomplete. The record retains
return, sink and exceptional effects, input identities and exact witnesses with zero false claims
within this finite corpus.

Six isolated workers compare cold, warm, unrelated-edit and helper-edit results against fresh analysis.
They record latency, response bytes, native work and peak child/worker RSS. Reuse validates the whole
file; these measurements do not establish finer cache granularity or a general speed improvement.
This is deterministic analysis evidence, not a live-agent investigation trial or a security proof.

## Bounded flow fact acceptance

The [2026-09-24 fact acceptance](../tests/agent-eval/results/2026-09-24-flow-facts/result.json)
retains two sink witnesses from a recursive Python fixture. An independent oracle executes nine
cases and checks UTF-8 syntax-tree source positions. Eleven followed semantic links match exact occurrences;
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
configuration reports E0308 at the independently checked source byte range. Six executable cases check default
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
The [current result](../tests/agent-eval/results/2026-10-21-distinct-release-delivery-resumable-correspondence/result.json) includes interrupted plan resumption and an independent satisfied step.
Explicit refresh clears stale evidence and actions; the old mutation review refuses.
A fresh review delivers a checked edit, reversal and patch. An independent receiver applies the patch and checks three finite outputs.

The evaluator records cold, warm and edited identity queries with latency, context bytes and isolated peak RSS.
Warm and clean reports agree. Every query recomputes correspondence (the stated connection between two descriptions); this slice adds no finer analysis cache.
The ordinary AST baseline reads the whole tiny source and excludes a native process launch.
These are deterministic fixture measurements, with no live-agent, token, semantic equivalence or general speed claim.
Audit the retained result with `python3 tools/correspondence-acceptance.py --audit RESULT`.

## Retained proof evidence

The [proof fixture](../tests/agent-eval/retained-proofs/task.json) pins an unchecked-module baseline and a finite Boolean source task.
Run `python3 tools/proof-evidence-acceptance.py --output PATH` to retain current execution evidence.
Run the same script with `--audit PATH` to check its recorded input fingerprints, Merkle roots, invalidation outcomes and patch replay.
The evaluator retains old and fresh model reports, seven input changes, independent observations and stale review refusals.
A separate Python oracle checks both Boolean inputs after applying the retained patch in an independent directory.
The [current result](../tests/agent-eval/results/2026-10-21-distinct-release-delivery-retained-proofs/result.json) records its fixture timing and context bytes.
These deterministic results make no live-agent, general performance or source correspondence proof claim.

## Imported scalar flow

The [import fixture](../tests/agent-eval/imported-flow/task.json) pins transitive return and sink effects across five root-local Python modules.
Its independent runtime oracle executes ten source/sink cases in two sanitizer contexts. Python ASTs supply separate UTF-8 call coordinates.
The [evaluator](../tools/imported-flow-acceptance.py) records missing modules and members, package/stub conflicts, configuration changes and rule changes.
Retained plans invalidate dependent conclusions while preserving independent observations. Paged facts link witnesses back to each helper's syntax.

The [result](../tests/agent-eval/results/2026-09-25-imported-flow/result.json) includes cold, warm, unrelated-edit and helper-edit comparisons against fresh analysis.
Isolated workers record latency, context bytes, transfer work and peak child/worker RSS. Whole-module closure reuse remains deliberately conservative.
The ordinary arm executes the runtime/AST oracle; it does not model an agent's search effort. Tokens and live-agent outcomes remain unavailable.
A reviewed helper-body change passes checks, apply, undo, redo and patch delivery. An independent receiver replays the patch and repeats the runtime oracle.
Run `python3 tools/imported-flow-acceptance.py --audit RESULT` to validate retained recorded input fingerprints, outcomes and replay.

## Repeated consumer discovery

A consumer is code that uses a declaration. Discovery searches for those uses before the declaration changes.
For a renamed helper, compare the same expected callers and ordering with the earlier and revised search implementation.
The timings in this section measure that operation and its indexing work, rather than an agent finding and repairing an unfamiliar bug.

The [pinned workload](../tests/agent-eval/index-consumers/task.json) covers generated Python functions
with repeated assignments and a fixed repository snapshot: 98,147 symbols and 592,962 references.
The [baseline](../tests/agent-eval/index-consumers/baseline.json) precedes the lookup implementation.
The [result](../tests/agent-eval/results/2026-09-27-index-consumers/result.json) retains implementation identities,
latency, process RSS and output size. A separate linear membership oracle checks every returned occurrence and its order.

| Workload | Queries per batch | Baseline median | New first batch | New warm median |
|---|---:|---:|---:|---:|
| 128 functions | 428 | 1.065 s | 0.120 s | 0.11866 s |
| 512 functions | 466 | 18.042 s | 0.552 s | 0.54954 s |
| Repository snapshot | 512 | 2.725 s | 0.061 s | 0.00190 s |

Each workload runs in an isolated process with one worker and three consecutive query batches.
The first new batch includes lookup construction; the warm median uses the remaining two batches.
All builds use the unoptimized test profile. OS caches remain active. Generated cases exercise
large definition groups; their group construction still costs time after the reference scan disappears.

Initial repository indexing took 173.5 seconds before and 95.6 seconds with the current implementation. Whole-process peak
RSS changed from 105 to 112 MiB, 336 to 341 MiB, 732 to 781 MiB across the three workloads.
Those peaks include extraction, resolution and oracle storage; they do not isolate lookup allocations or establish a memory improvement.
The result establishes repeated-query speed on this corpus, with unchanged occurrence output size.
The current run includes the resolution change below; it does not isolate that change from the consumer lookup.
It does not establish lower full-task latency or complete dynamic consumer discovery.

Mutation regressions cover target/name edits, insertion, removal, reordering, replacement, serialization,
concurrent reads and symbol-group changes. Native and browser callers share the same lookup implementation.
Run `python3 tools/index-consumers-acceptance.py --verify RESULT` to check retained bindings and workload agreement.
Repository answer sizes include absolute temporary paths and cannot be compared across hosts.
The verifier compares portable workload counts and generated answer sizes; each measured query
also checks exact occurrence membership and order against its independent linear oracle.

## Fresh resolution

The [pinned resolution task](../tests/agent-eval/index-resolution/task.json) compares complete symbol and reference outputs
against the [pre-change baseline](../tests/agent-eval/index-resolution/baseline.json).
The [current result](../tests/agent-eval/results/2026-09-27-index-resolution/result.json) records two fresh processes per workload.
The repository snapshot contains 98,355 symbols and 594,288 references.
Every output digest agrees, including reference targets, confidence and UTF-8 spans. Paths are relative to the pinned snapshot.

| Workload | Baseline build median | Current build median | Baseline peak RSS range | Current peak RSS range |
|---|---:|---:|---:|---:|
| 128 method owners | 0.169 s | 0.107 s | 10.1–10.5 MiB | 10.4–10.5 MiB |
| 256 method owners | 0.681 s | 0.214 s | 12.2–12.9 MiB | 11.2–12.8 MiB |
| 128 assignments | 0.128 s | 0.116 s | 9.5–9.8 MiB | 9.0–9.0 MiB |
| Repository snapshot | 177.778 s | 96.243 s | 847.4–945.7 MiB | 944.4–982.0 MiB |

Both arms use the unoptimized test profile, one worker and fresh extraction without the facts cache.
OS caches remain active. Build timings include scanning, extraction and resolution; process RSS also includes output serialization.
Two repetitions on one host establish a finite comparison, not a production latency distribution or a memory improvement.

Ambiguity checks inspect the first definition group and stop when another entity remains.
During resolution, the complete name map also rejects absent receiver types without scanning every symbol.
Outside that immutable pass, type queries still observe public symbol mutations.
Full counts use hash membership while preserving the original greedy input order, including asymmetric groups.
Independent linear-count tests cover duplicate and missing IDs, overloads, repeated assignments, CSS and configuration groups, and symbol mutations.
No persistent group cache or public API change accompanies this optimization. Resolution coverage and milestone gates remain unchanged.
Run `python3 tools/index-resolution-acceptance.py --verify RESULT` to audit bindings and complete output agreement.

## Native host recovery

The [pinned task](../tests/agent-eval/host-recovery/task.json) starts from 22 passing history tests.
The [result](../tests/agent-eval/results/2026-09-25-host-recovery/result.json) retains 430 handled faults
and 430 process exits across apply, undo, redo and recovery. Seven focused tests include staged
content, mode and symlink changes, conflicting recovery and compound failures. Filesystem byte,
mode, kind and absence checks use direct host reads against independently specified fixtures.
Each fault case reopens the journal and resumes pending work before checking the expected state.

A separate Rust/Lean comparison checks 224 outputs for publication and recovery decisions.
The [contract](host-recovery.md) distinguishes model theorems, tested agreement and trusted host operations.
This is recorded deliberately interrupted operations. It does not measure agent tokens, task latency benefits or power-loss recovery.
The retained baseline guide and disclosure use a one-file scan with 1,935 unresolved references;
they support local discovery without establishing repository-wide absence claims.

Reproduce the evidence with:

```sh
python3 tools/host-recovery-acceptance.py --output /tmp/host-recovery.json
python3 tools/host-recovery-acceptance.py --audit /tmp/host-recovery.json
```

## Scoped structural delivery

The [pinned Rust task](../tests/agent-eval/structural-change/task.json) renames a free function and
removes its unused scalar parameter across 49 call sites. The
[retained result](../tests/agent-eval/results/2026-09-28-structural-change/manifest.json) compares
prescribed ordinary edits with native scope-bound delivery in a fresh process. Both arms preserve
49 compiled public outputs. Receiver replay checks the delivered patch, exact reversal and reapplication.
The native arm retains its scope, ordered delivery receipt, completed plan and stale-scope refusal.

The [dogfood record](../tests/agent-eval/results/2026-09-28-structural-dogfood/manifest.json) contains
reviewed source edits on this repository, including a helper rename through the new operation.
These records span intermediate revisions. They do not constitute a clean-checkout replay or a live-agent trial.
Updated source-bound measurements use the `2026-09-28-structural-*` prefix; earlier results remain intact.

The signature subset admits primitive scalar parameters and literal call arguments. The compiler and
public-output oracle check their executed cases. General source equivalence, external consumers,
dynamic dispatch (choosing a callable from runtime values) and inferred contracts remain open. The oracle records its rustc version separately;
its nested compiler process is outside the check runner's executable identity discovery.

## Expression control and checked delivery

The [expression task](../tests/agent-eval/expression-control/task.json) pins a leaking helper behind
short-circuit operators and a conditional expression. Its independent CPython oracle observes public
outputs, sink calls and AST source byte ranges. The baseline uses the existing local 0.35.0 binary and records
both its hash and its age relative to the pinned source revision. It refuses the expression forms.

The [retained evaluator](../tools/expression-control-acceptance.py) compares ordinary AST discovery
with native summary discovery, repairs the discovered helper, adds a sink-free preview and replays
both patches in an independent receiver. It checks cache invalidation, interrupted plans and stale
review refusal. [Focused runtime cases](../sdk/python/tests/test_expression_control.py) cover
operand order, skipped calls, recursion, explicit raises and conservative comparison alternatives.
These are finite scripted cases, with no live-agent, token-saving or feasible-path claim.

The [retained expression run](../tests/agent-eval/results/2026-09-29-expressions-expression-control/result.json)
passes both delivered outcomes and independent patch application and checks in another checkout. Ordinary discovery reads 471 source
bytes across six files. Native discovery returns 128,738 bytes across eight calls with no source-body
reveals. All observed results remain scoped to this finite scripted fixture.


## Required call binding and checked delivery

The [binding task](../tests/agent-eval/call-binding/task.json) pins a leaking helper behind named
arguments and required positional-only and keyword-only parameters (slots that calls must supply by name). Independent CPython observations
check public outputs, ordered effects and source byte ranges. The baseline records the existing local 0.35.0
binary and its source-revision gap; that binary refuses the new parameter and keyword forms.

The [retained run](../tests/agent-eval/results/2026-09-29-calls-call-binding/result.json) passes
ordinary and native repair, a sink-free preview, resumption in a new process, invalidation and independent
patch application and checks in another checkout. Ordinary discovery reads 444 source bytes across six files. Native discovery returns
117,022 response bytes across eight calls with no source-body reveals. This is a finite scripted
comparison, with no live-agent, token-saving, feasible-path or general source-equivalence claim.

The [runtime corpus](../sdk/python/tests/test_call_binding.py) covers parameter kinds, reordered
keywords, caller isolation, explicit raises, invalid binding, syntax refusal, Unicode boundaries,
report contracts and cached/clean agreement. The `2026-09-29-calls-*` reports refresh ten flow groups
and index resolution against current source. Six additional reports cover compiler evidence,
correspondence, semantic evidence, index consumers, retained proofs and host recovery after the
Unicode dependency update. Four context reports refresh project batching, task bundles, task changes
and workflows with the pinned tokenizer. Earlier reports remain historical records.

The [dogfood manifest](../tests/agent-eval/results/2026-09-29-calls-dogfood/manifest.json) binds
100 receipts from reviewed `fr` previews, saved plans and history applications on this task itself.
New declarations, tests, fixtures, metadata and documentation use direct edits as recorded there.
All builds, full test gates and large evidence regeneration run on GitHub; local checks use the
resource guard and the existing binary.

## Literal defaults and exact signatures

The [literal-default task](../tests/agent-eval/literal-defaults/task.json) pins a leaking helper behind
an omitted keyword-only default. Its baseline records the installed 0.35.0 binary separately from
the source revision. That binary leaves the defaulted signature incomplete; independent CPython
execution confirms the public leak.

The [runtime corpus](../sdk/python/tests/test_literal_defaults.py) compares omitted and explicit
arguments, positional-only and keyword-only slots, immutable literal forms, recursive calls and
imported helpers. Python ASTs independently check parameter coordinates. Tests retain default syntax
spans, compare cached and clean results after edits, renew occurrences after unrelated changes, and
reject forged signature contracts. Evaluated and mutable defaults remain outside the supported subset.

The [retained run](../tests/agent-eval/results/2026-09-29-defaults-literal-defaults/result.json) passes
discovered repair, sink-free preview delivery, resumption in a new process and independent receiver
patch replay and reversal. Deterministic AST discovery exposes 442 source bytes across six reveals;
`fr` discovery exposes 118,341 response bytes across eight process calls, with no source reads.
These are different disclosure measures, not a token-savings claim. The twelve refreshed
`2026-09-29-defaults-*` report groups bind that implementation revision. These results cover a finite
corpus and do not establish source equivalence, path feasibility or general efficiency.
The [dogfood manifest](../tests/agent-eval/results/2026-09-29-defaults-dogfood/manifest.json) records
actual guarded `fr` edits on this implementation. Compilation, full tests and report regeneration
run on GitHub under the existing resource policy.

## Single-root namespace packages

The [pinned task](../tests/agent-eval/namespace-packages/task.json) requires discovery and repair of a
leaking helper behind packages with no initializer. The installed 0.35.0 binary baseline refuses
those imports; its recorded binary identity differs from the pinned source revision. Independent
CPython execution confirms the original leak and the expected public behavior after delivery.

The [runtime corpus](../sdk/python/tests/test_namespace_packages.py) covers mixed namespace and regular
packages, source entries within namespaces, relative imports, aliases, child fallback and empty
directory dependencies. It compares cached and clean results after initializers, competing modules,
stubs, helpers and missing paths change. Symlinks, ignored sources, module budgets and forged records
have explicit negative cases. Multi-root namespace merging and general runtime imports remain open.

The [retained run](../tests/agent-eval/results/2026-09-29-namespaces-namespace-packages/result.json)
passes reviewed repair, a sink-free preview feature, resumption in a new process and independent patch
replay and reversal. Ordinary AST discovery exposes 347 source bytes across four reveals; `fr`
discovery exposes 116,687 response bytes across eight process calls, with no source reads.
These different disclosure measures do not establish token savings. Thirteen refreshed
`2026-09-29-namespaces-*` report groups bind that implementation revision. The
[dogfood manifest](../tests/agent-eval/results/2026-09-29-namespaces-dogfood/manifest.json) retains actual
`fr` previews and applications for this implementation. Full gates and evidence regeneration run
on GitHub; local work uses the existing resource guard.

## Ordered scalar assignments

The [pinned task](../tests/agent-eval/scalar-assignments/task.json) requires finding and repairing a
leaking helper through unpacking, a swap and chained local assignments. Its installed-binary
baseline reports explicit cutoffs. The independent CPython oracle checks the original leak and
public behavior after repair, preview delivery, resumption in a new process, patch application and checks in another checkout and reversal.

The [runtime corpus](../sdk/python/tests/test_scalar_assignments.py) compares ordered scalar
assignments with CPython. It covers repeated targets, nested shapes, RHS effects, explicit raises,
lexical shadowing, exact definition spans, cache renewal and invalidation, and bounded refusal.
Branch, loop and recursive summaries retain the existing rules describing possible value connections. Typed readers reject
forged assignment contracts and require the complete current summary metadata.

The [import-search run](../tests/agent-eval/results/2026-10-15-shared-named-searches-scalar-assignments/result.json)
and its thirteen `2026-10-15-shared-named-searches-*` scalar groups record that implementation.
The [registry-startup receipt](../tests/agent-eval/results/2026-10-15-registry-startup-scalar-collection/manifest.json)
preserves the collection before import order was checked.
The [import-order receipt](../tests/agent-eval/results/2026-10-16-import-order-scalar-collection/manifest.json)
preserves the collection before startup reasons and per-file counts were added.
The [startup-diagnostics receipt](../tests/agent-eval/results/2026-10-16-startup-diagnostics-scalar-collection/manifest.json)
preserves the collection before unused function defaults were checked.
The [function-default receipt](../tests/agent-eval/results/2026-10-16-function-defaults-scalar-collection/manifest.json)
preserves the collection before flat module assignments were admitted.
The [parallel-assignment receipt](../tests/agent-eval/results/2026-10-16-parallel-bindings-scalar-collection/manifest.json)
preserves the collection before calls could use imported scalar defaults.
The [shared-default receipt](../tests/agent-eval/results/2026-10-16-shared-defaults-scalar-collection/manifest.json)
preserves the collection before aliases of checked values were admitted.
The [value-alias receipt](../tests/agent-eval/results/2026-10-17-value-aliases-scalar-collection/manifest.json)
identifies the current collection and its exact inputs.
The [dogfood manifest](../tests/agent-eval/results/2026-09-30-assignments-dogfood/manifest.json)
retains actual guarded `fr` previews and applications for existing function edits. Compilation,
full gates and evidence regeneration run on GitHub. These finite deterministic results do not
establish source equivalence, path feasibility or general efficiency.

## Roadmap outcomes on pinned Python repositories

The [roadmap catalog](../tests/agent-eval/roadmap.json) now uses `agent-analysis-v2`. The
[generated status](roadmap-status.md) explains 18 technical acceptance items in terms of what an
agent needs to do. Missing, historical or outdated results cannot complete an item. The status
command checks recorded identities and assertions; independent behavior tests remain required in CI.

Version 2 changes the still-open requirements: general language behavior must be tested on unfamiliar
projects, and agent evaluation must include both Sonnet and Luna, complete costs and delegated work.
The [product review](product-review.md) specifies that future pilot. The seven previously demonstrated
items retain their original scope and evidence. These are revised requirements, not new live results.

The [repository task contract](../tests/agent-eval/python-repositories/task.json) pins full source
archives, revisions, SHA-256 identities, budgets and upstream licenses for two tasks:

- **boltons:** reproduce a historical singularization defect for already-singular double-s words,
  preserve case and irregular/plural conversion, and restore a deliberately perturbed case helper.
- **more-itertools:** extend public `take` with negative tail counts on finite iterables, preserve
  positive/zero/None/index-protocol behavior, standardize nonintegral-count errors on TypeError, and update
  the explicitly superseded negative-count test. Independent probes check consumption and that
  selecting three values from 100,000 stays below 256 KiB of traced Python allocations.

These are fixed regression scenarios: the driver contains the symbol queries and prescribed solutions.
It finds those symbols through bounded `fr` queries; the Python runtime definition is explicitly
selected over its ambiguous stub candidate.
The helper is discovered from revealed call syntax. Discovery is not claimed to be target-free
reasoning by a fresh agent. The feature changes a requested contract, rather than claiming an
upstream bug. Only the named upstream test files and independent behavior oracles are acceptance
checks; this is not a claim that either entire upstream suite passed.

Repository names, archived revisions and expected fixes belong in these fixtures. They must not
select production analyzer behavior, cache policy or edit semantics. Future value-tracing acceptance
uses independent examples and held-out repositories, rather than tuning analysis to these repairs.

The evaluator first checks the unmodified failing behavior and passing upstream regression file.
It persists an observation and an independent README or public-module observation, obtains a reviewed mutation,
then performs a semantic external edit. A fresh process reopens the plan; affected evidence becomes
stale while the independent observation survives. The more-itertools README uses unindexed RST;
its dependency attempt retains a refusal, and the unchanged public Python module supplies the independent observation. The old review must refuse. Fresh targets and a new
review deliver through syntax checks, explicit post-change behavior and upstream checks, undo,
redo and patch export. Immutable receipts and final plans are retained. Independent receivers
start from the interrupted source, apply the patch, verify public behavior, reverse exactly, and
apply it again. The receiver oracle does not import the evaluator's implementation of the repair.

Actual dataflow reports retain incomplete analysis and its reasons. Probe rules do not establish
useful task-level flow accuracy, so `B.repository-flow` remains open. Insertion into the public
wildcard-import module is separately attempted and must retain the explicit refusal. Existing-body
replacement does not establish that public API insertion or dynamic exports are supported.

Run the `python-repositories` group of **Refresh refinement evidence** on GitHub. Its dedicated
job has a 15-minute limit and omits the Lean toolchain. Full generation is disabled locally.
Retained evidence is audited with `tools/python-repository-acceptance.py --audit DIRECTORY`;
roadmap reports are regenerated with `tools/roadmap-status.py --write` after reviewing catalog changes.
The `roadmap-report` group of that same workflow regenerates and audits just the status reports
in a five-minute Python job, with no Rust build or Lean installation. Download its `roadmap-report`
artifact and review both files before committing them.
Use the workstation guard for local checks. Neither a status report nor a passing repair establishes
a source implementation proof, a live-agent efficiency claim, or complete roadmap delivery.

The [current repository acceptance](../tests/agent-eval/results/2026-10-04-focused-pages-python-repositories/result.json)
passed on [GitHub run 37163442730](https://github.com/e6qu/fun-refactor/actions/runs/37163442730).
Schema 2 ignores only the marked SDK release-version literal when comparing recorded input fingerprints.
Other source changes still invalidate the evidence. The
[original schema 1 receipts](../tests/agent-eval/results/2026-09-30-roadmap-python-repositories/result.json)
remain unchanged; use their recorded evaluator revision for historical replay.
It contains 102 singularization cases and 263 tail-count cases, including unbounded negative integers.
The declared upstream files pass 17 and 148 tests; the latter also reports 9,884 subtests.
Both plans invalidate analysis and outcome after the semantic edit, preserve their independent step,
and finish with passing immutable delivery receipts. Independent receivers verify patch application,
exact reversal and reapplication. The roadmap therefore demonstrates `A.repositories`, `C.resume`
and `D.checked` in their stated cases. The [current status](roadmap-status.md) owns the latest
counts and remaining requirements; those counts do not establish general agent savings.

The [dogfood manifest](../tests/agent-eval/results/2026-09-30-roadmap-dogfood/manifest.json)
binds the local reviewed edits used to build this work. Its generation-attempt ledger preserves
failed evaluator attempts and later successes. Local probes use the installed older binary and
have a separate scope from the current-source GitHub acceptance reports.

Historical compressed evidence has [verified restoration instructions](../tests/agent-eval/EVIDENCE-ARCHIVES.md).
## Python development history

This is a dated engineering record. A run identifies one execution of a GitHub workflow.
An artifact is a saved output; a fingerprint verifies which bytes it contains.
Keep failures and corrected attempts together so readers can see what the fix establishes.
For current limits, use the [Python guide](python-contract.md) and [contributor handoff](continuity.md).

These notes describe problems found from 2026-10-12 through 2026-10-16.
Read the [current Python guide](python-contract.md) and [current results](roadmap-status.md) for today's support and counts.
The original failed reports, validators and CI logs remain unchanged.

| Problem found | Response recorded during that work | Original record |
| --- | --- | --- |
| Function-initialization collection failed | Keep the generator result with its original validator; correct obsolete fixtures and report-size handling | [Generator failure](../tests/agent-eval/results/2026-10-12-declarations-history-python-contract-failure/manifest.json), [first gate](https://github.com/e6qu/fun-refactor/actions/runs/38247965263), [corrected gate](https://github.com/e6qu/fun-refactor/actions/runs/38248690526) |
| Class tests named four wrong call locations and one wrong boundary | Correct the expectations while preserving the original failed result and runtime outcomes | [Class collection failure](../tests/agent-eval/results/2026-10-13-class-initialization-history-python-contract-failure/manifest.json) |
| A stored trace report made the next function lookup ambiguous | Keep the report outside the analyzed sample project | [Initialization-location gate](https://github.com/e6qu/fun-refactor/actions/runs/38363037039) |
| Added contract text exceeded an 8 KiB report limit | Shorten the current contract text; keep the limit | [Annotation gate](https://github.com/e6qu/fun-refactor/actions/runs/38381948322) |
| An annotation span included the type when a trace named only the parameter | Check the parameter name's exact location and reject a substituted type span | [Recorded annotation notes](https://github.com/e6qu/fun-refactor/blob/166808fac5bc94a708daf1b20490033b2425f934/docs/continuity.md#how-to-keep-the-results-trustworthy) |
| A fixture still expected unused empty collections to block tracing | Check supported literal allocation and keep mutation unchecked | [First collection gate](https://github.com/e6qu/fun-refactor/actions/runs/38417041882) |
| The runtime observer supplied input/output functions only to the caller | Use the isolated import observer to supply them to imported modules too | [Second collection gate](https://github.com/e6qu/fun-refactor/actions/runs/38419813576) |
| Documentation exceeded its writing budget and assertions named an older case inventory | Split the sentence and match the independently checked inventory at that collection | [Documentation check](https://github.com/e6qu/fun-refactor/actions/runs/38430801404) |
| Two alias tests treated global declarations like local assignments | Compare lexical declarations and local shadowing separately with Python execution | [Function-alias gate](https://github.com/e6qu/fun-refactor/actions/runs/38440122902) |
| A later wording edit changed an input behind a saved report | Restore the collected guide and audit saved Python inputs during documentation review | [Built-in-reference PR check](https://github.com/e6qu/fun-refactor/actions/runs/38473217926) |
| GitHub cancelled a waiting job with an internal runner error before tests started | Retry the same checks with the original deadline; keep the failed attempt | [Function-documentation check, first attempt](https://github.com/e6qu/fun-refactor/actions/runs/38540355301/attempts/1) |
| A second import check still required every member to be a function or module | Validate declared values as a distinct category while keeping missing members unchecked | [First value-import check](https://github.com/e6qu/fun-refactor/actions/runs/38554935392) |
| A package-literal fixture expected an unavailable import after value imports were admitted | Expect a declared value and retain the checks that the child stays unloaded and its call remains unfinished | [Value-import SDK check](https://github.com/e6qu/fun-refactor/actions/runs/38555541573) |
| Five import examples counted calls inside unused methods or kept an old dotted name after renaming | Select the calls in the entry function and rename each part of the dotted name; preserve the failed collection and its unchanged runtime outcomes | [Value-import collection failure](../tests/agent-eval/results/2026-10-14-value-imports-history-python-contract-failure/manifest.json), [original run](https://github.com/e6qu/fun-refactor/actions/runs/38558837376) |
| Corruption tests exceeded their 120-second deadline while copying all 570 cases for each changed row | Copy only the changed row and report metadata; keep auditing every case and check that the original report stays untouched | [Value-import retention timeout](https://github.com/e6qu/fun-refactor/actions/runs/38574356001) |
| Two named-default tests used an observer that only caught exceptions from calling the entry | Use the existing isolated observer that also records module-loading failures; retain both `NameError` expectations | [First named-default check](https://github.com/e6qu/fun-refactor/actions/runs/38595108328) |
| One added scalar-default comment exceeded the fixed prose budget | Remove the redundant comment and keep the budget | [Scalar-default prose check](https://github.com/e6qu/fun-refactor/actions/runs/38666246115) |
| The scalar-default capture map omitted its Rust path-type import | Import the path type and rerun the same native controls | [Scalar-default compiler check](https://github.com/e6qu/fun-refactor/actions/runs/38666372468) |
| Longer scalar-default contract text exceeded existing report byte budgets | Shorten the repeated contract wording while retaining the rules and limits; recollect reports tied to recorded source inputs | [Scalar-default SDK check](https://github.com/e6qu/fun-refactor/actions/runs/38666586639), [collection precheck](https://github.com/e6qu/fun-refactor/actions/runs/38667422037) |
| A name-based SDK assignment grouped a long report audit with several proof checks and hit the 15-minute deadline | Use measured case durations to balance the same four groups; preserve every test, execution order and deadline | [Scalar-default full CI timeout](https://github.com/e6qu/fun-refactor/actions/runs/38670183873/job/116211233691) |
| Three main audit runners could not download the existing `actions/setup-go` commit and failed before tests started | Install the same Go 1.27.2 release from its official checksum-verified archive through the shared local action | [First setup failure](https://github.com/e6qu/fun-refactor/actions/runs/38684678523/job/116256708027), [round-trip setup](https://github.com/e6qu/fun-refactor/actions/runs/38684678523/job/116256707969), [translation setup](https://github.com/e6qu/fun-refactor/actions/runs/38684678523/job/116256708093) |
| A value-import test still treated an unused `object`-based class as unsupported | Keep the negative case with an unsupported `list` base; retain the positive built-in-base runtime comparisons | [First built-in-base SDK check](https://github.com/e6qu/fun-refactor/actions/runs/38697413065) |
| Compiler evidence repeatedly hashed large driver files with an unoptimized SHA-256 dependency and reached the 15-minute job deadline | Optimize that dependency in development builds, compare digest bytes independently and keep every check and deadline | [Partial original artifact](../tests/agent-eval/results/2026-10-15-import-search-compiler-timeout/manifest.json), [timed-out run](https://github.com/e6qu/fun-refactor/actions/runs/38713751219) |
| An older Pages build finished after the newer build and cancelled its deployment | Cancel older Pages workflows when a newer push arrives; retry the latest deployment with its original artifact and deadlines | [Newer revision, cancelled deploy](https://github.com/e6qu/fun-refactor/actions/runs/38725791903/attempts/1), [older build that finished later](https://github.com/e6qu/fun-refactor/actions/runs/38725757264) |
| A named-import byte test added JSON whitespace before measuring the compact response | Enforce the same 65,536-byte limit on the client's actual output and measure compact encoding | [First named-import gate](https://github.com/e6qu/fun-refactor/actions/runs/38727247627) |
| Review found that an over-limit named-import fixture expected its unvisited provider among the loaded files | Expect only the entry file; supersede collection before authored report generation and keep the 128-name limit | [Superseded collection](https://github.com/e6qu/fun-refactor/actions/runs/38730391730) |
| Two older tests classified an uncalled callback in a list as executed startup code | Move deferred callbacks into the positive cases; keep evaluated callbacks in the negative cases | [Registry startup checks](https://github.com/e6qu/fun-refactor/actions/runs/38776615039) |
| The writing checker joined embedded lambda test programs into a long prose sentence | Recognize Python lambda syntax as code, keeping the same writing limits | [Writing check](https://github.com/e6qu/fun-refactor/actions/runs/38777666719) |
| The longer registry contract description exceeded a fixed 8 KiB response allowance | Shorten the repeated metadata to the earlier byte count, preserving the rules and response limit; collect new results for the corrected source | [Registry contract precheck](https://github.com/e6qu/fun-refactor/actions/runs/38778457682) |
| Docker inspection failed and forced cleanup exceeded five seconds; the error discarded the original process result | Preserve process and inspection failures, collect bounded Docker service and host diagnostics, and rerun the same controls on a fresh worker with unchanged limits | [Dotenv control failure](https://github.com/e6qu/fun-refactor/actions/runs/38782595333/job/116586355773) |
| The native link checker read a callback-table example as a link to a document named `read_input(` | Ignore text between matching backticks and inside fenced examples; keep checking real links, including those beside code or unmatched backticks. Run both link readers in the hosted documentation review | [Native link-check failure](https://github.com/e6qu/fun-refactor/actions/runs/38786708443/job/116600749772) |
| Review of the import-order change found that an unresolved dependency could inherit a cycle-failure label | Distinguish a known unavailable cycle member from an unchecked dependency. Both leave the answer incomplete. Supersede the initial collection before retaining a new matching family | [First passing focused check](https://github.com/e6qu/fun-refactor/actions/runs/38823694034), [superseded Python collection](https://github.com/e6qu/fun-refactor/actions/runs/38824543553), [initial familiar-library comparison](https://github.com/e6qu/fun-refactor/actions/runs/38824562716) |
| Further runtime checks found that child loading precedes binding names from a from-import, and that written export order can decide which side of a cycle starts first | Check child loading before local bindings and retain the written export order for startup checks. Keep the public sorted name inventory separate. Supersede the earlier collection before retention | [Independent runtime controls](https://github.com/e6qu/fun-refactor/blob/038e2e3137aeb40aa9194317f02cb309c7890123/sdk/python/tests/test_import_order.py), [superseded collection](https://github.com/e6qu/fun-refactor/actions/runs/38825548634) |
| Startup diagnostics stopped at the writing check, leaving formatting and fresh result collection unfinished | Remove the redundant comment and filler, format on the runner, and generate matching results before the full audit | [Initial focused failure](https://github.com/e6qu/fun-refactor/actions/runs/38857468026) |
| A new test expected a two-file import report to fit in 8 KiB even though required metadata exceeded the cap | Check that refusal and verify preserved counts with the existing single-file 8 KiB omission case. Keep the same byte limits | [Original SDK failure](https://github.com/e6qu/fun-refactor/actions/runs/38857521254/job/116832209122) |
| The familiar-library collector still named the previous analyzer commit, which was absent from the runner's checkout after a squash merge | Pin the current source fingerprints, check both committed and uncommitted changes, and test identical code in a separate history without the original commit. Recollect one matching family | [Collector failure before replay](https://github.com/e6qu/fun-refactor/actions/runs/38861980286) |
| GitHub storage reset an artifact connection after the analysis and Python audits had passed | Retry the same artifact once into a clean file, sharing the existing 60-second deadline. Keep byte limits and digest checks, and test exhausted budgets and discarded partial bytes | [Interrupted download](https://github.com/e6qu/fun-refactor/actions/runs/38871928585) |

The 2026-10-15 reader review found another tracking mismatch: its Python requirement still expected 524 cases while the saved report already contained 694.
The [refreshed examples](../tests/agent-eval/results/2026-10-15-reader-review-python-contract/manifest.json) keep the same 347 programs, 14 rules, incomplete answers and refusals.
The tracker now checks that declared inventory. This changes the count of matching recorded requirements, not the supported Python behavior or real-package outcomes.

The registry-startup change adds 20 programs, each also checked after renaming and relocation.
They distinguish creating a callback table from executing a callback, a default or a dictionary value during import.
The [new Python collection](../tests/agent-eval/results/2026-10-15-registry-startup-python-contract/manifest.json)
covers 367 programs and 734 cases under the same fourteen rules.
Local dictionary writes need a previously created, unexposed dictionary and checked keys and values.
Imports, unknown calls and aliases end that allowance. Shared heap reads and calls through a stored callback remain unsupported.
The familiar full-package replay remains a separate result; these small examples cannot establish general package support or agent savings.

The import-order change adds 30 programs, each also checked after renaming and relocation.
For example, two files can import each other when the requested function already exists.
Moving its definition below the import makes Python raise `ImportError`; that answer must remain incomplete.
The checks follow Python's order: load the requested files first, then make the imported names available to the caller.

The [import-order examples](../tests/agent-eval/results/2026-10-16-import-order-python-contract/manifest.json)
cover 397 programs and 794 cases under the same fourteen rules.
They retain 308 incomplete cases, 18 refusals and 112 documented differences between the model and runtime observations.
The [focused check](https://github.com/e6qu/fun-refactor/actions/runs/38827471103) passed 63 command-line tests and 1,328 Python interface tests.
The [complete example collection](https://github.com/e6qu/fun-refactor/actions/runs/38828460880) passed all 794 cases and 55 checks rejecting altered reports.
The actual guide snippets also ran: 14 rename commands, two introductory traces, three callback-table examples and three import-order examples.

The startup-diagnostics change keeps those language rules and explains the remaining unchecked statements more precisely.
For example, a startup call and an initializer exceeding the syntax limit receive different reasons.
Counts for each loaded file include statements beyond the sixteen displayed locations.
The [targeted runner check](https://github.com/e6qu/fun-refactor/actions/runs/38867284098)
passed 64 command-line tests and 1,421 Python interface tests, including inconsistent-count refusals and the unchanged 8 KiB response checks.
The [startup-diagnostics collection](../tests/agent-eval/results/2026-10-16-startup-diagnostics-python-contract/manifest.json)
keeps 397 programs and 794 cases: 308 incomplete cases, 18 refusals and 112 documented model/runtime differences.
The guide's actual snippets include three startup-diagnostic examples alongside the earlier lessons.
The [combined audit](https://github.com/e6qu/fun-refactor/actions/runs/38873415835)
checked the fresh results and all 55 altered-report controls, with the existing deadlines unchanged.

A squash merge combines a branch's development commits into one final commit.
The library replay must keep the analyzer code unchanged while checking the selected libraries.
A source fingerprint is an identifier computed from files and their names, used to detect changes.
The collector now compares those identifiers for its chosen source directories and build files without needing the development commits locally.
Changing the checked source still prevents collection. The [regression tests](../tools/test-frozen-analyzer.py)
also check an independent history with identical source and no original commit object.
All four freeze-check tests passed before compilation in the same targeted run.

The function-default change checks object lookups while Python creates an unused function.
For example, `factory=dict` stores Python's dictionary type; `factory=dict()` creates a dictionary immediately and remains unchecked.
Earlier function definitions, checked aliases, named function imports and unchanged built-in names can supply defaults.
The [focused checks](https://github.com/e6qu/fun-refactor/actions/runs/38883873798)
passed 66 command-line regressions and 1,473 Python interface tests.
The [function-default collection](../tests/agent-eval/results/2026-10-16-function-defaults-python-contract/manifest.json)
checks 409 programs in 818 original or renamed cases under the same fourteen rules.
It retains 318 incomplete cases, 18 refusals and 116 documented differences between the model and runtime observations.
All 54 new default-capture interface tests passed, including changed imports and reused answers after edits.
The guide's three new default examples ran against both the analyzer and Python execution.

The [familiar-library replay](https://github.com/e6qu/fun-refactor/actions/runs/38885840574)
removes ten unchecked Toolz declarations: nine `factory=dict` defaults and one `map=map` default.
The original identity-helper question has 82 unchecked statements instead of 92 across the same loaded files.
Funcy and Pydash's identity-helper counts stay at 98 and 461. Full-package outcomes remain incomplete or deliberately refused.
These declaration checks do not add calls through callback arguments or establish general package support.

The Python collection's 55 altered-report checks took 122.642 seconds, beyond the combined audit's existing 120-second wrapper.
Their test setup repeatedly rebuilt and tokenized the same example list before checking each corrupted report.
The setup now builds that list once and supplies fresh copies; every recorded row still goes through the unchanged auditor.
A new test checks that changing a copied example cannot alter the expected answers for later checks.
The [combined audit](https://github.com/e6qu/fun-refactor/actions/runs/38890887619)
passed all 56 checks against the retained result in 81.249 seconds, inside the unchanged 120-second wrapper.
These timings describe separate hosted runs. The complete checks fit the existing deadline without omitting report validation.

The collector also rewrote its growing JSON file after every completed case.
Saving the 800th case encoded and rewrote the preceding 799 results, repeating work as the collection grew.
It now writes each new row during collection and performs one full save after the final audit.
Each completed append leaves readable, incomplete progress; command timeouts preserve earlier completed rows.
The [fresh collection](https://github.com/e6qu/fun-refactor/actions/runs/38899354891)
passed in 12 minutes 2 seconds under the unchanged 15-minute deadline.
It checked the same 409 programs and 818 cases, including 318 incomplete answers, 18 refusals and 116 documented differences.
All 56 altered-report checks passed in 110.057 seconds. Ten new writer checks cover progress, Unicode, refusals, failed audits and interruption between appends.
The [new receipt](../tests/agent-eval/results/2026-10-16-incremental-reports-python-contract/manifest.json)
records the fresh producer and inputs; the earlier collection remains available for its original claims.
Python behavior and report validation did not change.
The [saving comparison](benchmarks.md#saving-python-test-results) replays both methods on the same 818 observations on one hosted runner.
Report encoding and saving took 115.694 seconds before and 0.609 seconds after, including the final save.
The resulting files have identical bytes. Process writes fell from 19,344,364,657 to 97,882,761 bytes, counting overwritten data.
Read those saving-only timings separately from total job time: builds, analysis, runtime observations and audits have their own costs.
The environment-recording update superseded the [initial queued comparison request](https://github.com/e6qu/fun-refactor/actions/runs/38899322597) before its job ran.
That request produced no timing samples.

The parallel-assignment change reads every right-hand value before creating any destination name.
For example, `_map, _filter = map, filter` stores existing built-in functions without calling them.
Local function aliases, imports and reexports can preserve their defining functions in a trace.
Literal exports retain separate source positions for each destination.
The [corrected focused run](https://github.com/e6qu/fun-refactor/actions/runs/38926590392)
passed 68 native regressions and 1,560 Python interface tests, including 85 new cases.
It also checked 848 authored Python executions under the same fourteen declared rules.
The [lesson](python-contract.md#save-several-helper-names-in-one-assignment) supplies both files, the input/output rules and commands before showing the results.

The [first executed focused run](https://github.com/e6qu/fun-refactor/actions/runs/38924909991)
passed the native and new language tests, then failed an existing 8 KiB report test.
The expanded policy description appeared twice in the report and exceeded its metadata allowance.
The corrected description is 197 characters, shorter than the earlier 199-character text, while still identifying the new assignment rule.
The 8 KiB limit, omitted-location totals and every test assertion stayed unchanged.
The corrected run passed that test and the rest of the focused checks.
The [initial queued request](https://github.com/e6qu/fun-refactor/actions/runs/38924564547)
was superseded before execution when alias-budget coverage was added; it produced no test samples.

The [first broader Python run](https://github.com/e6qu/fun-refactor/actions/runs/38928709867)
passed 618 interface tests but found one earlier expectation that a flat literal assignment must remain incomplete.
The new rule intentionally supports `metadata, other = 0, 1`.
That case now has positive coverage for tuple/list spelling, local/imported helpers and helpers that pass on or discard the input.
The negative cases still check evaluated values, mismatched lengths, special metadata and rebinding.
The failed run stopped before collecting any analyzer reports; the complete evidence family is recollected from the corrected source.

The [corrected Python collection](https://github.com/e6qu/fun-refactor/actions/runs/38932167099)
passed 768 interface tests, all three parallel-assignment lesson variants and 56 corruption controls.
Those controls took 96.626 seconds within the unchanged 120-second limit.
The [retained Python results](../tests/agent-eval/results/2026-10-16-parallel-bindings-python-contract/result.json)
check 424 programs in 848 original or renamed cases under the same fourteen rules.
They preserve 330 incomplete cases, 18 refusals and 120 documented differences between model and runtime observations.
An incomplete case can pass this comparison because the declared expectation is that the tool leaves that question unanswered.
The complete collection passed within its existing fifteen-minute deadline.

The [fresh familiar-library replay](../tests/agent-eval/results/2026-10-16-parallel-bindings-fixed-arity-probes/result.json)
reduces unchecked startup statements for Funcy's original identity-helper question from 98 to 94.
The `seqs.py` count falls from 22 to 18; Toolz stays at 82 and Pydash at 461 for their separate identity-helper questions.
The original thirty cases still have 24 incomplete reports and six deliberate CLI refusals.
The one hundred expanded cases still include zero complete full-package answers, sixteen complete isolated-declaration answers and eight complete standalone-module answers.
These counts measure coverage of those selected questions, not general readiness, analysis speed or savings on an agent's complete task.
The separate expanded `inc` query falls from 80 to 76 unchecked statements across the same sixteen files and 75 import searches.
Toolz's `return_none` stays at 82 across fourteen files, and Pydash's `add` stays at 420 across eight files.
Funcy's sixteen-file cutoff and Pydash's 256 KiB source cutoff remain in place.
The [retention audit](https://github.com/e6qu/fun-refactor/actions/runs/38935883405)
rechecked the collected bytes and all 56 corruption controls; those controls took 83.917 seconds under the unchanged 120-second wrapper limit.

The imported-default change lets a function capture a scalar from another checked file.
For example, `settings.py` defines `LIMIT = 0`, and `helpers.py` imports it before defining `choose(value=LIMIT)`.
An omitted argument uses that stored zero; an explicit input keeps its own connection to the output.
The [three-file lesson](python-contract.md#share-a-default-value-between-files) supplies the setup before comparing those calls with lists and late imports.
Lists, object and function defaults, attribute expressions and ordinary global reads remain outside this call support.

The [focused hosted run](https://github.com/e6qu/fun-refactor/actions/runs/38949131554)
passed 69 native regressions, 1,763 Python interface tests and 872 authored Python runtime checks.
Before the broader collection, review corrected two model expectations for unsupported list defaults.
Those calls remain incomplete even with an explicit argument; an empty trace in an incomplete answer cannot establish that input is absent.
The corresponding runtime expectations and unsupported-case checks stayed in place.

The [broader Python run](https://github.com/e6qu/fun-refactor/actions/runs/38953427505)
passed 971 interface tests and all five new lesson variants against both Python and the analyzer.
The [retained results](../tests/agent-eval/results/2026-10-16-shared-defaults-python-contract/result.json)
check 436 programs in 872 original or renamed cases under the same fourteen rules.
They include 338 incomplete cases, 18 refusals and 126 documented differences between model and runtime observations.
The 56 altered-report checks passed in 108.815 seconds inside the unchanged 120-second audit limit.
The collection passed within its existing fifteen-minute deadline.
The [combined retention audit](https://github.com/e6qu/fun-refactor/actions/runs/38956879242)
rechecked the complete collected family and passed the same 56 altered-report checks in 111.797 seconds under that limit.

The [new familiar-library replay](../tests/agent-eval/results/2026-10-16-shared-defaults-fixed-arity-probes/result.json)
keeps the original thirty cases at 24 incomplete reports and six deliberate CLI refusals.
Its one hundred expanded cases still have zero complete full-package answers, sixteen complete isolated-declaration answers and eight complete standalone-module answers.
The new small-program support does not finish the libraries' remaining initialization and import behavior.

The value-alias change follows an earlier checked value through another name.
For example, `BASE = 0; LIMIT = BASE` can supply a captured default without turning a list or object into a scalar.
The [shared-setting lesson](python-contract.md#give-a-shared-setting-another-name) supplies three files, defines the terms and shows both argument forms before explaining the limits.
Import records retain the original declaration and separate value aliases from function aliases.
Missing alias inventories and contradictory records are rejected before saved answers are reused.

The [first focused run](https://github.com/e6qu/fun-refactor/actions/runs/39022266750)
found an older expectation that `sorted = 0; public = sorted` must remain incomplete.
The new rule deliberately follows that local constant instead of assuming Python's built-in `sorted` object.
The corrected case checks a complete answer with no input connection and records `public` as an alias of the local `sorted` declaration.
The separate unsupported-call test stayed in place.

The [second focused run](https://github.com/e6qu/fun-refactor/actions/runs/39022658793)
passed all 69 native checks, type checking and 525 Python interface cases, but one new altered-report test targeted the wrong field.
That fixture attempted to change a name list rather than a binding chain.
It now removes the actual value-binding chain; both the Python reader and native reuse command must reject the altered report after its digest is recalculated.

The [corrected focused run](https://github.com/e6qu/fun-refactor/actions/runs/39023267940)
passed 69 native checks, 1,941 Python interface tests and 896 independent Python runtime expectations.
The new interface suite contributes 184 cases covering original declarations, aliases across files, scalar defaults, shared limits, edited sources and rejected reports.
These runtime prechecks do not replace the separately collected analyzer comparisons.
The run passed within its existing fifteen-minute deadline; no timeout limit was raised.

The [first broader Python collection](https://github.com/e6qu/fun-refactor/actions/runs/39024232894)
passed 1,155 interface tests and all six new lesson variants.
It then found two copies of the same older expectation: the original and renamed `sorted = 0; stored = sorted` example.
The analyzer correctly completed that checked value alias; the authored expectation still required an incomplete answer.
The [failed report and its original validator](../tests/agent-eval/results/2026-10-17-value-aliases-first-python-contract-failure/manifest.json)
preserve that mismatch. The corrected expectations leave the Python programs and production code unchanged.

The [corrected Python collection](https://github.com/e6qu/fun-refactor/actions/runs/39025611731)
passed 1,155 interface tests, all six new lesson variants and the complete analyzer comparisons.
The [retained results](../tests/agent-eval/results/2026-10-17-value-aliases-python-contract/result.json)
cover 448 programs in 896 original or renamed cases under the same fourteen rules.
They retain 344 incomplete cases, 18 refusals and 130 documented differences between model and runtime observations.
An incomplete answer passes only when the example deliberately expects that question to remain unanswered.
All 56 altered-report checks passed in 106.492 seconds under the unchanged 120-second audit limit.
The complete collection passed within its existing fifteen-minute deadline.
The [combined retention audit](https://github.com/e6qu/fun-refactor/actions/runs/39026989443)
verified the collected files and source records, then passed the same 56 checks in 59.651 seconds under that limit.
These timings describe individual hosted checks, rather than analysis overhead or an agent's complete task.

The familiar-library replay still gives no complete full-package answers. Unknown startup operations, missing imports and exhausted limits remain separate obstacles.
The tool still needs to demonstrate complete traces in unfamiliar projects and savings on complete agent tasks.

The [first full PR check](https://github.com/e6qu/fun-refactor/actions/runs/39027802949/job/117382130062)
passed 824 selected Python interface cases but found two older list and class alias expectations.
Those tests still required an incomplete answer when a local declaration shadows a built-in name.
The corrected cases keep the same programs and check complete setup, Python execution and the original value's recorded kind.
The scalar counterpart and a checked import use the same rule.
A separate unknown native import keeps the answer incomplete, even when its name matches a built-in function.
These test corrections leave the collected analyzer, guide and example inputs unchanged.

The [next full Python check](https://github.com/e6qu/fun-refactor/actions/runs/39029039323/job/117385681317)
passed 826 selected cases but found a mistaken assertion in the new declaration check.
The [other Python job](https://github.com/e6qu/fun-refactor/actions/runs/39029039323/job/117385681445)
passed 824 cases and found the same assertion for the scalar setting.
The behavior and runtime assertions passed. The fixture imported only the helper, so its report needed no imported value record.
The corrected test keeps that case, then imports the alias and checks the original kind and full declaration chain.
Production code and collected example inputs remain unchanged.

The [scripted client job](https://github.com/e6qu/fun-refactor/actions/runs/39031180016/job/117391774611)
then found an unrelated memory stop during its intentionally unsuccessful review control.
The sampled process group reached 805,642,240 bytes against its unchanged 805,306,368-byte limit.
Only one response had completed, so the two-response assertion correctly failed.
The [original artifact and diagnosis](../tests/agent-eval/opencode/memory/2026-10-17-scripted-limit/README.md)
retain that failure and its partial work.
The corrected CI job supplies the existing small catalog and preserves its path in the scripted environment.
Catalog verification runs before the client. Memory, time and complete-work requirements stay unchanged.

The [corrected compiler run](https://github.com/e6qu/fun-refactor/actions/runs/38716582932)
measured the hashing cost on the same runner. Hashing 32 MiB took 0.830 seconds before optimization
and 0.021 seconds after it. Both digests matched Python's independent SHA-256 result.
Collecting compiler identities, diagnostics and changed-input refusals then took about 13 seconds.
The complete job passed in 9 minutes 55 seconds, within its unchanged 15-minute deadline.

That change's [Pages deployment](https://github.com/e6qu/fun-refactor/actions/runs/38725791903/attempts/2)
passed after retrying only the cancelled deploy job. It reused the successful build for the latest
merged revision and kept every deployment deadline.

The [earlier built-in-reference collection](../tests/agent-eval/results/2026-10-13-builtin-bindings-scalar-collection/manifest.json)
preserves the results before literal function documentation writes were admitted.
The [function-documentation collection](../tests/agent-eval/results/2026-10-14-function-docs-scalar-collection/manifest.json)
preserves the results before declared value imports were admitted.
The [value-import collection](../tests/agent-eval/results/2026-10-14-value-imports-scalar-collection/manifest.json)
preserves the results before known prior values could defer unused module function defaults.
The [named-default declaration collection](../tests/agent-eval/results/2026-10-14-named-defaults-scalar-collection/manifest.json)
preserves the results before calls could use captured local scalar defaults.
The [scalar-default call collection](../tests/agent-eval/results/2026-10-15-scalar-named-defaults-scalar-collection/manifest.json)
preserves the results before bounded built-in type maps could coexist with supported traces.
The [built-in type-map collection](../tests/agent-eval/results/2026-10-15-type-registries-scalar-collection/manifest.json)
preserves the results before classes could declare verified built-in bases.
The [built-in-base collection](../tests/agent-eval/results/2026-10-15-builtin-bases-scalar-collection/manifest.json)
preserves the results before wildcard exports shared their provider searches.
The [shared wildcard collection](../tests/agent-eval/results/2026-10-15-shared-star-searches-scalar-collection/manifest.json)
preserves the results before ordinary named imports shared searches within a statement.

For example, saving a report inside a sample project can change what the next code query sees.
The report itself may still contain correct bytes. The selected project's inputs have changed,
so the next lookup needs validation. Keeping reports outside the analyzed tree avoids that extra input.

The annotation work also found format validation after the freshness check. Current readers now reject
a noncurrent contract before deciding whether changed dependencies require fresh analysis.
A daily audit had cancelled an unfinished merge audit on the same commit. Scheduled checks now wait
for an active audit; a newer push can still replace older work.

The [call-binding guide](call-binding.md) keeps the built-in-name clarification outside the source-bound Python guide.
These changes address the recorded failures. They do not establish whole-package support or general agent savings.

### Replace a stored default with a supplied value

A helper can now receive an explicit value while leaving its checked object default unused.
For example, after `stored = []`, `choose(read_input())` can pass the input through `def choose(value=stored): return value`.
Calling `choose()` still leaves consuming the stored list unchecked.
The [complete lesson](replace-python-defaults.md) supplies the files and compares both calls with Python execution.
This closes an argument-checking gap; package startup and calls through supplied functions remain separate work.

The [first focused check](https://github.com/e6qu/fun-refactor/actions/runs/39051542846) found older expectations that rejected explicit object replacements.
It also found a mistaken new assertion: that example returned the input, but sent only zero to the configured output.
The corrected assertion checks no input-to-output connection and compares the exact Python events.
The omitted-object controls remain in place.

The [next focused check](https://github.com/e6qu/fun-refactor/actions/runs/39052096575) found that expanded mandatory rule text exceeded an existing 8 KiB report budget.
The corrected descriptions remove repeated wording while keeping every field and the original byte limit.
The [corrected focused run](https://github.com/e6qu/fun-refactor/actions/runs/39053130954) passed 70 native checks and 2,032 Python interface cases.
The unchanged output-budget cases also ran first and passed. The job kept its fifteen-minute deadline.

A [documentation check](https://github.com/e6qu/fun-refactor/actions/runs/39052366220) still expected an explicit replacement to remain incomplete.
It stopped before report collection began. The [original log and receipt](../tests/agent-eval/results/2026-10-17-default-replacements-reader-failure/failure.json) preserve that unsuccessful attempt.
The corrected example keeps both the supported replacement and the unfinished omitted-default case.

The [following collection](https://github.com/e6qu/fun-refactor/actions/runs/39053350792) passed all examples and 56 altered-record checks, then exceeded its fifteen-minute job deadline during cleanup.
Its audit took 112.208 seconds within the existing 120-second limit.
The [original archive and log](../tests/agent-eval/results/2026-10-17-default-replacements-deadline/failure.json) remain historical; the cancelled job cannot qualify as a successful collection.
Two independent interface-test groups had run serially for 202.60 and 149.53 seconds before report collection and auditing.
The corrected [runner](../tools/python-contract-runner.py) overlaps interface checks with collection, then requires both to pass before auditing.
The five failure controls check that either worker's error fails the job and that both errors remain available.
Job and audit deadlines remain unchanged. These timings describe CI scheduling.
Language-analysis overhead and complete agent costs remain unmeasured.

The [corrected collection](https://github.com/e6qu/fun-refactor/actions/runs/39055073592) passed 1,246 interface cases, including the new replacement lesson.
It checked 458 programs in 916 original or renamed cases and passed all 56 altered-record controls in 83.868 seconds.
The complete job took 8 minutes 38 seconds within its unchanged fifteen-minute deadline.
The earlier cancelled run checked 1,155 interface cases on another runner; these observations establish deadline compliance, not a controlled speed comparison.

The [retention audit](https://github.com/e6qu/fun-refactor/actions/runs/39057204807) kept the original reports and verified their recorded inputs against current files.
It passed the same 56 altered-record controls in 108.915 seconds under the unchanged 120-second limit.
That [collection receipt](../tests/agent-eval/results/2026-10-17-default-replacements-python-contract/manifest.json)
retains 344 incomplete answers, eighteen refusals and 126 documented differences between model and runtime observations.
The familiar-library replay keeps its previous full-package outcomes. No full-package answer becomes complete through this call rule.

A CI-only edit had created different commit IDs while leaving the analyzer unchanged.
Retention now checks complete analyzer source folders, local dependencies, parsing queries, the Python client, build script and dependency manifests against current code.
Each record still names its actual collection commit and passes its original input audit.
Twelve retention controls include changed code, dirty or untracked analyzer files, substituted archives and failed input audits.
Completed captures remain byte-for-byte unchanged; code or recorded-input changes require fresh results.

The [first full PR suite](https://github.com/e6qu/fun-refactor/actions/runs/39058153609) found fifteen native report links still pointing at the preceding value-alias collection.
The tracker had advanced to the new collection, but those tests had not. Their source checks correctly rejected the older reports.
Native tests now select the current collection through the tracker, check its receipt and report checksum, and then run the existing audit.
Four controls cover changed bytes, missing current receipts, a different collection and the complete set of current report groups.
The first helper check caught a checksum encoding type error; it was corrected to use the repository's existing encoding API.
That attempt produced no compiler report, and its now-stale Python collection was cancelled.
The edited test files are recorded inputs, so the compiler, Python and familiar-library reports require fresh hosted collections.
Unchanged captures retain their original names and bytes when assembled into the new current collection.

The [fresh compiler collection](https://github.com/e6qu/fun-refactor/actions/runs/39066090634) passed all four selection controls, 88 editing tests and 27 altered-result checks.
The [fresh Python collection](https://github.com/e6qu/fun-refactor/actions/runs/39066103722) passed 1,246 client cases, checked 916 examples and passed 56 altered-result checks in 108.586 seconds.
The [fresh library replay](https://github.com/e6qu/fun-refactor/actions/runs/39066248827) preserves the same unfinished full-package outcomes.
The [complete collection audit](https://github.com/e6qu/fun-refactor/actions/runs/39066978333) passed fourteen admission controls and the existing audits for all seventeen native report groups.
It also repeated the 56 Python controls in 108.699 seconds, within the unchanged 120-second audit limit.
The [default-replacement receipt](../tests/agent-eval/results/2026-10-17-default-replacements-ci-python-contract/manifest.json) keeps the same 344 incomplete cases, eighteen refusals and 126 documented differences.
These checks establish consistency with recorded inputs. They do not establish complete library support or a performance improvement.

## Check a small function that fills a table during import

Importing a helper can call another function before the application starts.
The [complete startup lesson](check-python-startup.md) first fills a local dictionary and then follows submitted input through a helper.
Its control changes startup to replace that helper: Python returns zero instead of the submitted value, and the analysis must remain unfinished.
The supported setup function writes only to dictionaries created earlier in its own file.
It can receive them as arguments or stored dictionary defaults; the lesson explains both using `register(labels)` and `output=table`.
The whole function must fit the existing 128-node syntax limit, where a node represents a piece of parsed code.
Unknown calls, imported shared dictionaries, unknown loops and helper replacement remain unchecked.
Changing the setup source invalidates the existing saved answer.
The [saved-startup lesson](save-python-startup.md) now shows reusing setup checks separately.

The [first focused check](https://github.com/e6qu/fun-refactor/actions/runs/39080162868) exceeded its fifteen-minute job deadline.
All native tests passed, followed by Python groups of 617, 722 and 815 passing cases.
Those three groups took 173.50, 223.47 and 249.54 seconds after compilation and setup, which left too much work in one serial job.
The import and saved-answer group now runs in a separate hosted job using the same test command.
Both jobs keep the fifteen-minute deadline. The fix removes no tests and increases no resource limits.
These job durations diagnose CI scheduling. They do not measure the analyzer's speed or the language's execution cost.

The [first Python capture](https://github.com/e6qu/fun-refactor/actions/runs/39080445641) checked all 928 authored report cases but failed one of its 1,368 interface tests.
The failing control meant to change `register()` into `register(0)` and compare Python's `TypeError` with an unfinished analysis.
Its text replacement also changed `def register():`, producing invalid syntax and a `SyntaxError` instead.
The fix edits exactly one standalone call and rejects analysis results containing an import syntax error.
The expected `TypeError` remains unchanged. The failed workflow does not qualify as passing evidence, and its final audit did not run.

Reader review also corrected both standalone lessons to print “Possible input connection” beside the completion result.
A connection from an unfinished analysis can differ from Python's actual behavior after startup changes a helper.
The [default-replacement lesson](replace-python-defaults.md) uses the same wording.
The extra analysis time and memory for startup registration remain unmeasured; these comparisons check specific behavior against Python.

The [corrected Python collection](https://github.com/e6qu/fun-refactor/actions/runs/39082855346) passed all 1,368 interface cases and checked 464 programs in 928 original or renamed cases.
It passed all 56 altered-record checks in 67.519 seconds, within the unchanged 120-second audit deadline.
Its [receipt](../tests/agent-eval/results/2026-10-18-startup-registration-compact-ci-python-contract/manifest.json)
preserves 352 incomplete cases, eighteen refusals and 130 documented differences between the model and Python.
The [split focused check](https://github.com/e6qu/fun-refactor/actions/runs/39082857741) passed both jobs, including every group from the timed-out attempt.
The [familiar-library replay](https://github.com/e6qu/fun-refactor/actions/runs/39080371185) keeps all forty full-package questions unfinished.
Copied declarations and standalone modules retain their earlier smaller successes; neither establishes complete package support.

The [complete report audit](https://github.com/e6qu/fun-refactor/actions/runs/39083435664) passed fourteen admission controls and the existing audits for all seventeen native report groups.
It repeated the 56 Python controls in 100.379 seconds, within the unchanged 120-second deadline.

GitHub warned that the new Python report exceeded its recommended file size because retention copied the fully indented JSON.
The [storage check](https://github.com/e6qu/fun-refactor/actions/runs/39084661457) retains compact JSON and the exact original downloaded archive instead.
The compact copy occupies 28,826,487 bytes; the archive occupies 5,248,180 bytes and preserves the original capture unchanged.
The receipt records separate fingerprints for the compact file, original report and archive, while keeping the actual collection commit and binary identity.
The current reader requires both files and rejects changed values, changed fingerprints and obsolete receipts.
All 23 retention controls and 56 altered-report controls passed; the latter took 83.427 seconds within the unchanged deadline.
This compares storage representations of the same data. It does not measure analysis speed or a reduction in complete agent cost.

## Ask several Python questions after checking startup once

The [complete two-question lesson](check-python-together.md) asks whether submitted input passes through a forwarding helper and a helper returning zero.
Startup is the code Python runs while importing their files.
The new interface checks that setup once within a request, then returns separate answers.
It accepts 2–16 functions from the same file and shares the existing tracing and output limits.
A later request checks startup again; saving checked setup for later requests remains unfinished.

The [focused hosted check](https://github.com/e6qu/fun-refactor/actions/runs/39110056871) passed 92 tests of the compiled checker and all selected Python interface tests.
The new interface contributes 41 cases covering answer agreement, changed imports, unknown startup effects, shared limits and altered reports.
Behavior checks compare shared answers with separate `fr` analyses and run Python to verify the forwarding and zero-returning examples.
The [full Python collection](https://github.com/e6qu/fun-refactor/actions/runs/39110451140) passed 1,409 interface cases and checked 464 programs in 928 original or renamed cases.
It retains 352 incomplete cases, eighteen refusals and 130 documented differences between the checker's model and Python's observed behavior.
All 56 altered-report controls passed in 93.551 seconds, within the unchanged 120-second deadline.
These are behavior comparisons, not timing benchmarks. Analysis overhead compared with executing Python remains unmeasured.

Review also found that editing parsing or grammar code could leave an old saved answer reusable.
The build now hashes all declared checking sources, generated grammar tables, dependencies, compiler identity and build settings.
Saved answers and saved information about functions and imports both depend on that result.
The report collectors record the same declared sources.
Seven Rust tests and six collector tests change files, names, settings and dependencies, and check that missing declared inputs stop the operation.
The [compiler check](https://github.com/e6qu/fun-refactor/actions/runs/39110392846) passed these controls and the existing compiler comparisons.

The first workflow-report check exposed a temporary test project missing the newly required source inputs.
The fixture now creates a complete project; the source checks still reject missing inputs.
The [corrected workflow check](https://github.com/e6qu/fun-refactor/actions/runs/39111437978) passed its source-change controls and existing workflow and recovery checks.
No analysis limit, CI deadline or audit deadline was increased.

When saving these captures, the [report-admission job](https://github.com/e6qu/fun-refactor/actions/runs/39114027198) exceeded the existing 120-second Python audit deadline.
The [focused measurement](https://github.com/e6qu/fun-refactor/actions/runs/39115312912) found 24,072 case checks across 56 tests.
A test damaging a late case rechecked every unchanged case before it.
The test fixture now puts its damaged case first while keeping the complete inventory and full audit.
Two new controls prove that this happens and that damage to another row is still caught.
The production validator continues to check the full report and its current source inputs.

The [fixed measurement](https://github.com/e6qu/fun-refactor/actions/runs/39115623782) passed all 58 controls in 42.609 seconds, within the unchanged deadline.
It performed 3,793 case checks against the same 928-case report, including the added controls.
The earlier measured run took 73.296 seconds; the two hosted runners may differ, so elapsed times describe those runs.
The call counts identify the repeated work removed. These are audit measurements, not a speed or overhead comparison between `fr` and Python.

The [next admission run](https://github.com/e6qu/fun-refactor/actions/runs/39115880371) passed all 58 Python controls in 66.471 seconds.
It then rejected the workflow and repair reports because they came from different commits after the temporary-project fixture fix.
The [replacement repair collection](https://github.com/e6qu/fun-refactor/actions/runs/39116213030) uses the workflow report's exact commit.
The requirement that this report pair share one source commit remains unchanged.

The [first complete admission](https://github.com/e6qu/fun-refactor/actions/runs/39116460591) passed all seventeen report groups and the workflow/repair checks, then saved that report set.
All 58 Python controls passed in 53.026 seconds, within the unchanged 120-second deadline.
The [Python receipt](../tests/agent-eval/results/2026-10-18-shared-startup-ci-python-contract/manifest.json) preserves the original collection commit, compiled-program identity and exact original archive alongside the compact report.

The [first full PR gate](https://github.com/e6qu/fun-refactor/actions/runs/39117090490) found that the filesystem boundary test treated the new build-time source reader as runtime analysis code.
The test now recognizes that one Cargo build helper and forbids runtime source files from importing it.
Runtime analysis still has to read project files through the shared filesystem layer used by the terminal and browser.

Another completed check still selected a compiler report from September after its inputs had changed.
The [compiler acceptance test](../tests/compiler_evidence.rs) now selects the current collection, checks its receipt, and passes its compiler observations through the same full audit.
The historical report remains unchanged.
The compiler comparison, Python examples and familiar-library probes record that test file as an input, so the correction required three fresh captures.
The checking program's source and the other five capture groups remain unchanged.

The [replacement report admission](https://github.com/e6qu/fun-refactor/actions/runs/39119891634) passed all report checks and saved [that collection](../tests/agent-eval/results/2026-10-18-shared-startup-checked-ci-scalar-collection/manifest.json).
Its 58 Python controls passed in 52.358 seconds within the unchanged deadline; the earlier report set remains recorded separately.

Full CI then found other saved reports tied to the changed checking tools.
The tests now use [fresh hosted reports](../tests/agent-eval/results/2026-10-18-ci-repairs-ci-collection/manifest.json), with the original reports preserved.
For example, recovery evidence must describe the tools being checked today; a passing older run cannot establish that result.
The source and behavior checks still reject changed inputs and incorrect results.
The cache test's small project now supplies the required workspace and source-input list, and still rejects missing inputs.

One compiler test read and rewrote a saved report through Rust before validating it in Python.
That numeric conversion broke the report's content check even though the original report passed.
The test now validates the original file directly and retains the full compiler audit.
Two added controls reject an altered nested report format or content check.
The [fresh Python run](https://github.com/e6qu/fun-refactor/actions/runs/39128833298) passed 1,409 interface tests.
Its 58 altered-report tests passed in 64.001 seconds, within the existing 120-second deadline.
No timeout, resource limit or writing budget increased.

The [familiar-library comparison](https://github.com/e6qu/fun-refactor/actions/runs/39110449189) still has no complete full-package answers.
Its 100 cases include sixteen complete copied-declaration answers and eight complete standalone-module answers.
Those smaller results do not establish support for complete Toolz, Funcy or Pydash imports.
Jaraco.functools was still reserved at the time of this capture. Its later unsuccessful
first admission is retained in the [package milestone record](package-milestones.md).

## Finish three original-package questions

The [package milestones](package-milestones.md) now pass for Boltons, dateutil and JDcal with one unchanged analyzer.
JDcal's README supplied its leap-year question before anyone opened its implementation for this work.
The [first evaluation](https://github.com/e6qu/fun-refactor/actions/runs/39388270081) passed after
[fresh familiar-package coverage](https://github.com/e6qu/fun-refactor/actions/runs/39382493915) at the same commit.
The retained records include original sources, Python outcomes, renamed callers and unfinished answers for unknown calls and exhausted search budgets.
The familiar packages also pass source-edit and import-replacement controls for saved answers.
These finite value-route questions establish neither general Python support nor lower agent costs.

The [formatting preflight](https://github.com/e6qu/fun-refactor/actions/runs/39373244802)
and [writing preflight](https://github.com/e6qu/fun-refactor/actions/runs/39376730312) failed before the new frozen comparison.
The fixes apply Rust formatting and improve the prose without increasing its budgets.
An explicit build now supplies the Python checks' executable, preventing secondary missing-binary errors when an earlier step fails.
The [broader recheck](https://github.com/e6qu/fun-refactor/actions/runs/39380170357) passed at the existing deadlines.
The freeze records the unsuccessful preflights; earlier Jaraco and fn.py outcomes remain in their original archives.

## Pass dictionaries to a startup function

Importing a helper can first call `register(labels, table)` to fill a dictionary.
The checker now follows those dictionary arguments, including when two parameters refer to one object.
A default such as `output=table` stores the earlier dictionary; an explicit argument can replace it for one call.
The [complete lesson](check-python-startup.md) introduces these terms and compares the result with Python.
Changed source contents, rebinding a stored default's name, imported shared dictionaries and unknown effects leave the answer unfinished.
All existing limits remain unchanged.

The [first collection](https://github.com/e6qu/fun-refactor/actions/runs/39261124279) passed 171 focused tests and 1,655 Python interface tests.
It failed two source-location expectations for an example and its renamed copy.
Python evaluated `source()` in a startup argument, then raised `TypeError` before entering the application.
The unfinished analysis reported a possible connection from the separate `source()` inside `entry`.
The new test expectation mistakenly selected the startup call instead.
Inspecting the retained trace confirmed the distinction; the correction selects the application's call and adds a runtime/location regression.
The required `TypeError`, incomplete answer and exact source-to-output check remain intact.
The failed collection is not passing evidence.

The [corrected collection](https://github.com/e6qu/fun-refactor/actions/runs/39263253753) passed 952 original and renamed report cases and 1,656 interface tests.
Its 172 focused cases comprise 96 argument/reference tests and 76 loop tests; all 58 altered-report controls passed too.
The [receipt](../tests/agent-eval/results/2026-10-19-registration-arguments-python-contract/manifest.json) preserves 476 programs,
364 incomplete cases, eighteen refusals and 134 documented differences from Python's observed behavior.
Those counts describe checked examples, not universal Python support.

The [fresh familiar-library replay](../tests/agent-eval/results/2026-10-19-registration-arguments-fixed-arity-probes/result.json) checked 100 helper cases and 200 Python executions.
All forty full-package cases remain unfinished or refused: 32 incomplete and eight deliberately invalid requests.
Sixteen copied-declaration cases and eight standalone-module cases complete in their smaller contexts.
Toolz still needs conditional imports, function-argument inspection and other setup this rule cannot establish.
There is no separate time or memory measurement for this language change.
The [continuation notes](continuity.md#continue-python-work-from-actual-library-failures) now lead with the user question and remaining package obstacles.

The [first full PR gate](https://github.com/e6qu/fun-refactor/actions/runs/39268191916) then found a separate CI timeout.
The accounting job finished its tests near the five-minute deadline, but upload and cleanup took the job to 302 seconds.
GitHub marked the job cancelled even though every test step passed; this is not a passing CI result.
Its work included a sixty-second checkout, about 84 seconds of accounting checks and 118 seconds of saved-code-change grading.
The independent grading checks now run in their own five-minute job, with the same commands, report and artifact.
The final gate requires both accounting and grading to pass.
Both five-minute deadlines remain unchanged, and every check remains required.
Use [PR #543's checks](https://github.com/e6qu/fun-refactor/pull/543/checks) for the corrected run.
