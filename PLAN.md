# fun-refactor roadmap

`fr` helps an agent understand a project with little context, change its structure,
and inspect evidence that each change meets its requirements.
It also aims to help other projects adopt Lean specifications incrementally.

This document contains the active delivery plan. Git history and
[development continuity](docs/continuity.md) retain completed milestone detail.
[BUGS.md](BUGS.md) records defects, [CHANGELOG.md](CHANGELOG.md) records releases,
and [CLI.md](CLI.md) documents commands that exist today. Names proposed here remain design work until implemented.

## Current status

The core reads 19 languages. It supports 311 of 456 capability × language pairs.
Every remaining pair carries a reason in `fr capabilities`.
A supported pair describes the accepted operation scope; individual inputs can still require review or refuse.

| Measure | Current value |
|---|---|
| Query sets | 17 |
| Entry-point catalogs | 10 |
| Capabilities × languages | 24 × 19 |
| Supported pairs | 311 of 456, every other one carrying its reason |
| Defects fixed | 713 |
| Defects open | 1 |

| Milestone | Status | Delivered foundation | Remaining outcome |
|---|---|---|---|
| M0 safe writes | Complete | Recoverable multi-file commits and structured failure evidence | Maintained as a shared write invariant |
| M1 undo and redo | Complete | Persistent native transactions plus bounded in-memory browser transactions, conflict checks, apply, undo, redo and reviewed native retention | Maintain the shared history invariant |
| M2 compact project understanding | Complete | Bounded maps, symbols, packages, dependencies, calls, routes, contracts, schemas, tests, Cargo workspace evidence, source-free semantic IR and independently verifiable Merkle disclosure for code maps, traces, impact and value flow | Extend from measured agent needs |
| M3 Git integration | Complete | Patches, repository views, staging history, reviewed commits and durable owned-worktree lifecycle | Maintained as a shared repository invariant |
| M4 agent workflow | In progress | Portable skill, reviewed task changes, checked semantic intents, bounded authoring, checks and thirty passing autonomous trials | Make context use competitive across broader tasks and reduce repair calls |
| M5 Lean adoption | Complete | Pinned package initialization, anchored model scaffolds, regeneration, proof-debt ratchets, generated CI and bounded evidence reports | Maintain the trust boundary and extend the documented model subset as real projects require it |
| M6 framework transformation | Complete | Shared code IR, route/page feature hierarchies, Lean-backed framework policies, revision-bound migration, reversible registration, dependency and cutover edits, transaction-bound project checks, plus pinned two-way runtime fixtures | Maintain the bounded framework subset and add new pairs from evidence |

## What exists now

- Syntax trees, symbols, scopes, confidence tiers, byte edits and a content cache.
- Navigation, implementations, usages, call graphs, flow, impact and entry points.
- Rename, extract, inline, move, signature changes, imports, deletion and structural rewrites.
- Cross-language references, configuration provenance and configuration-to-code traces.
- A shared translation IR with Rust, Go, Java, Python, TypeScript, Zig, Bash and Lean readers and writers.
- Bounded project views for Cargo/npm packages, local dependencies, Cargo ownership, calls, tests, routes, request/response contracts and selected schemas.
- Next.js/FastAPI route conversion and OpenAPI service scaffolds within documented subsets.
- Revision-bound migration plans for one-file Next.js/FastAPI route features, with endpoint agreement, explicit dispositions and reversible source-history writes.
- Local recipes, expectations, workspace previews and canonical formatting.
- Rust, Go, Java, Python, JavaScript, TypeScript and TSX body authoring through complete semantic bodies and checked deltas.
- Rust declaration replacement and function insertion into files, inline modules, impls and traits.
- A source-free semantic IR contract and zero-dependency Python SDK with cross-runtime canonical validation.
- Merkle-committed progressive semantic and source disclosure with exact actions, strict
  per-response bounds and opaque scalar, typed-node and statement-position edit capabilities.
- Multi-file authoring batches using one reviewed source-history transaction.
- Declared checks with reviewed configuration digests, bounded output and compact successful reports.
- Persistent native source history with checked apply, undo, redo, recovery and Git patch export.
- Basis-bound native history compaction that retains audit summaries while bounding replay payloads.
- Bounded Git status, diff, changed-declaration and call-context views.
- Unix staging previews and writes, durable staging undo/redo, reviewed commits and owned worktree creation, recovery, removal, resumption and archive compaction.
- Native releases, a WASM API, and a browser playground with checked transaction undo/redo and shared Git patch downloads.
- Lean models for edits, positions, history, patch properties, pagination, confidence, workspace membership, revision buffers, declaration insertion placement and framework reporting policies.
- Source anchors, signature maps and shared Rust/Lean executable cases.
- External-project Lean initialization, anchored Rust model scaffolds, proof-preserving regeneration, named debt ceilings, generated CI and bounded verification evidence.
- Thirty passing autonomous trials across pinned strsim and regex snapshots and generic semantic fixtures, with replayable patches and independent behavioral oracles.

## Evidence baseline

The latest PR 7 evaluation changes two crates in the complete pinned regex workspace.
Both fresh Luna-low agents pass project checks, independent 1,060-input project and receiver oracles,
ordered original/final checks, exact reversal and index-preservation checks.
The `fr` arm uses 15,458 measured context tokens and 42 calls; ordinary files use 11,600 tokens and 20 calls.
This single pair shows a 33.3% `fr` context premium and makes no context-parity claim.
The `fr` agent uses the reviewed-plan and compact transaction workflow.
Seven refused or failed calls and repeated inspection identify discoverability and call count as the next practical bottlenecks.
The PR 8 prescribed-workflow projection exercises current handle selection against the pinned
workspace and retains every mutation and verification step. With the current portable skill, it
reduces that trace to 29 calls and 13,414 context tokens. This is 2,044 below the observed `fr` arm
and 1,814 above the ordinary-file arm. This one-trace counterfactual is separate from the retained
fresh adoption pair below.

The first fresh PR 8 pair produced correct patches and passed both 1,060-case behavior oracles.
Both agents edited before running the original-state checks, so neither passed acceptance. The
`fr` arm used 23,973 context tokens and 49 calls; ordinary files used 13,852 and 21. The retained
diagnostic now drives an execution-time original-check gate and narrower command guidance.

The second fresh PR 8 pair passes every gate and replays from retained patches. The `fr` arm
uses 13,949 context tokens and 30 calls; ordinary files use 12,815 and 23. The 1,134-token or
8.8% `fr` premium in this sample is below the PR 7 pair's 33.3% premium. Inspection favors
`fr`; authoring, delivery output and tool time remain the measured costs.

The file agents retained verbose successful check logs while the `fr` agents used compact output.
The checksum-bound M4ac projection applies the same output policy to both arms without altering prompts, requests, calls or other payloads.
With quiet successful streams and declarations retained, mean context is 14,194.5 tokens for `fr` and 7,726 for files.
With declarations omitted after review, the means are 13,278.5 and 6,810.
The normalized fixed action sequence therefore leaves a 6,468.5-token mean `fr` gap.

The PR 9 controlled broad-query fixture compares eight separate project calls, already reusing a
reviewed `context_basis`, with one query batch. Every normalized nested report matches. Including
the batch manifest, median counted context falls from 2,695 to 2,453 tokens, a 9.0% reduction, while
calls fall from eight to one. The current local rerun records median subprocess time falling from
1.803 to 0.231 seconds with the fact cache disabled. With separate prewarmed caches, it falls from
0.183 to 0.029 seconds. Token counts use
fixed representatives for opaque identities. This prescribed
three-repetition fixture supports a fresh workflow trial; it is not agent-success or population evidence.

The PR 10 controlled delivery fixture compares seven compact manual calls with a reviewed workflow
preview and write. It counts the 318-byte manifest. Median context falls from 2,047 to 1,880 tokens,
an 8.2% reduction, while calls fall from seven to two. The current local rerun records median time
falling from 0.284 to 0.262 seconds. Final stages, history, source and patch match
in all three repetitions. This fixed sequence supports an adoption trial and makes no agent claim.

The PR 12 controlled task fixture compares four separate discovery and contract calls with one
revision-bound task bundle. It counts the 446-byte task manifest and preserves identical normalized
query reports, exact target operation and selected-check evidence. Across three rotating repetitions,
median counted context falls from 1,802 to 1,514 tokens, a 16.0% reduction. Serialized context falls
from 6,662 to 4,758 bytes, a 28.6% reduction. Calls fall from four to one. Both arms stop before
fragment creation or mutation. This is
a fixed serialization and call-count comparison, not autonomous-agent or population evidence.

This projection explains the original aggregate result but does not predict how agents adapt to shared instructions.
It makes repeated skill, inspection and transaction output the immediate optimization target.
The [coordinated evaluation](docs/agent-coordinated-evaluation.md) contains the protocol, retained evidence and limits.

The PR 13 controlled change fixture compares the composed task, author and workflow route with one
reviewed task-change command. Calls fall from five to two. Median counted context falls from 4,627
to 3,740 tokens, a 19.2% reduction. Serialized context falls from 13,690 to 11,288 bytes, a 17.5%
reduction. Both arms resolve the same task, apply the same source change, run identical reversal
stages and emit the same patch. The task-change transaction also binds its required checks. This
fixed comparison makes no autonomous-agent or population claim.

PR 20's deterministic discovery fixture records one resolution owner, one waiter, byte-identical
cold reports, a two-stage profiled batch and stale-handle refusal. Repository dogfood returned a
name-only report in 1,675 bytes and its exact behavior continuation in 3,203 bytes. Earlier Luna-low
attempts found the right code but consumed excessive context; no later live-agent savings claim has
replaced that result.

PR 21's generic progressive-disclosure fixture uses an independent Python implementation of the
documented Merkle format. It verifies semantic, source, combined, root-hole, shortcut and child
identities; follows four exact actions; reconstructs source; detects a hidden change; and refuses a
stale action. Its complete semantic response is 5,381 bytes. The abridged initial response is 3,755
bytes and every response stays within the requested 4,096-byte conservative token upper bound. This
is deterministic protocol evidence without a live model or a cryptographic collision proof.

PR 22's generic disclosed-edit fixture independently calculates the two opaque capabilities for
equal integer literals and confirms that they remain distinct. It follows one capability without
source, compiles and runs the result, refuses the stale capability, validates forward and reverse
patches and exercises undo and redo. Three expanded disclosure responses total 13,577 bytes and stay
below their 16,384-byte bound; the author preview is 3,968 bytes. This is deterministic workflow
evidence, not a live-model result or a proof of the trusted parser, writer, compiler or hash.

The latest paired evaluation runs Codex CLI 0.154.0 with Luna at low effort against a frozen binary
and evaluator. Both arms pass the complete coordinated regex task, including 1,060 project and
receiver oracle cases, exact undo/redo, ordered checks and index preservation. The `fr` arm uses
17,711 measured context tokens and 42 calls; files use 11,182 tokens and 17 calls. This single pair
confirms workflow adoption after two retained diagnostic cohorts. Its 58.4% context premium remains
an optimization target and does not establish a population result.

## Product contract

An agent should move from bounded discovery to a Merkle-committed semantic skeleton, reveal only the
relevant IR hierarchy, and request exact source only when needed.
Every answer must state coverage, uncertainty, source basis and omitted results.

A change must retain one identity through preview, validation, patch export, apply, undo and redo.
Existing user changes outside the selected scope must survive.
Syntax validation, compilation, behavioral tests and formal proofs answer different questions, and reports must distinguish them.

The CLI and library remain the common interface, with portable agent skills teaching the workflow.
An additional transport can follow demonstrated integration needs.
The core stays independent of a running language server.
Lean and project compilers remain explicit verification dependencies.

## Engineering decisions

The identifiers remain stable for references in defect records.

| ID | Current decision |
|---|---|
| D1 | Keep the core standalone and independent of an LSP process. |
| D2 | Use byte-range splices and reparse changed source; preserve bytes outside selected edits. Explicit formatter commands validate their own formats. |
| D3 | Share symbol identity across reference, import, call, flow and provenance relationships. |
| D4 | Carry confidence with resolved references; weaker evidence does not grant rewrite permission. |
| D5 | Report unresolved flow boundaries and extend analysis only through explicit models. |
| D6 | Use substitution and override provenance for configuration languages where that model applies. |
| D7 | Use catalogs for declarative entry-point rules and explicit adapters for framework semantics. |
| D8 | Report unsupported inputs and partial coverage; refuse operations that cannot satisfy their contract. |
| D9 | Offer JSON and dry-run previews. Report completed commits and recovery limits as documented in CLI.md. |
| D10 | Maintain scope resolution in the project's query and index layers. |
| D11 | Declare supported toolchain versions and run validation with those versions. |
| D12 | Use the cheapest supported Codex CLI model at its lowest effort for routine real-agent evaluations. Record the exact configuration. |

## Delivery plan

The first packaged roadmap is complete. Twenty merged pull requests established the product foundation and its first measured workflow reduction:

| PR | Outcome | Status |
|---|---|---|
| [PR 0](https://github.com/e6qu/fun-refactor/pull/259) | Agent-ready verified refactoring foundation | Merged |
| [PR 1](https://github.com/e6qu/fun-refactor/pull/261) | Agent Context Protocol v2 | Merged |
| [PR 2](https://github.com/e6qu/fun-refactor/pull/262) | Generalized structural authoring | Merged |
| [PR 3](https://github.com/e6qu/fun-refactor/pull/263) | Durable Git workspace lifecycle | Merged |
| [PR 4](https://github.com/e6qu/fun-refactor/pull/264) | Lean adoption kit | Merged |
| [PR 5](https://github.com/e6qu/fun-refactor/pull/265) | Framework semantic model | Merged |
| [PR 6](https://github.com/e6qu/fun-refactor/pull/266) | Verified feature migration | Merged |
| [PR 7](https://github.com/e6qu/fun-refactor/pull/267) | Context-Competitive Agent Workflow | Merged |
| [PR 8](https://github.com/e6qu/fun-refactor/pull/269) | Agent Workflow Simplification | Merged |
| [PR 9](https://github.com/e6qu/fun-refactor/pull/270) | Bounded Project Query Batches | Merged |
| [PR 10](https://github.com/e6qu/fun-refactor/pull/271) | Verified Change Workflow | Merged |
| [PR 11](https://github.com/e6qu/fun-refactor/pull/272) | Browser Transaction History | Merged |
| [PR 12](https://github.com/e6qu/fun-refactor/pull/273) | Revision-Bound Agent Task Bundles | Merged |
| [PR 13](https://github.com/e6qu/fun-refactor/pull/275) | Reviewed Agent Task Changes | Merged |
| [PR 14](https://github.com/e6qu/fun-refactor/pull/276) | Semantic Agent Model and Authoring | Merged |
| [PR 15](https://github.com/e6qu/fun-refactor/pull/277) | Agent IR Contract and Python SDK | Merged |
| [PR 16](https://github.com/e6qu/fun-refactor/pull/278) | Checked Semantic Delta Authoring | Merged |
| [PR 17](https://github.com/e6qu/fun-refactor/pull/280) | Reviewed Semantic Intent Operations | Merged |
| [PR 18](https://github.com/e6qu/fun-refactor/pull/281) | Reviewed Semantic Edit Plans | Merged |
| [PR 19](https://github.com/e6qu/fun-refactor/pull/282) | Incremental Project Identity and Agent Query Latency | Merged |
| [PR 20](https://github.com/e6qu/fun-refactor/pull/283) | Bounded Agent Discovery and Concurrent Query Coalescing | Merged |
| [PR 21](https://github.com/e6qu/fun-refactor/pull/284) | Merkle-Committed Progressive Agent Disclosure | Merged |
| [PR 22](https://github.com/e6qu/fun-refactor/pull/285) | Disclosure-Bound Semantic Editing | Merged |
| [PR 23](https://github.com/e6qu/fun-refactor/pull/287) | Disclosure-Bound IR Structure Editing | Merged |
| [PR 24](https://github.com/e6qu/fun-refactor/pull/288) | Content-Addressed Progressive Project Evidence | Merged |
| [PR 25](https://github.com/e6qu/fun-refactor/pull/290) | Agent Formalization Workbench | Merged |
| [PR 26](https://github.com/e6qu/fun-refactor/pull/291) | Agent Proof Companion | Merged |
| [PR 27](https://github.com/e6qu/fun-refactor/pull/292) | Agent-Authored Formal Properties | Merged |
| [PR 28](https://github.com/e6qu/fun-refactor/pull/293) | Cross-Stack Agent Coverage | Merged |

The second package applies the public semantic representation through checked, source-free
operations. PRs 14 through 18 established the IR, SDK, delta, intent and direct unique-scalar routes.
PRs 19 through 21 made repeated discovery faster, bounded and progressively disclosed. PR 22 joins
the revealed hierarchy directly to exact scalar authoring, including repeated values. PR 23 extends
that capability boundary to typed node replacement and statement-list structure. PR 24 applies the
same traversal to source-free project evidence and gives every subtree a reusable object address.

Git history and [development continuity](docs/continuity.md) retain checkpoint-level detail.

### PR 8. Agent Workflow Simplification

Status: merged as [PR 269](https://github.com/e6qu/fun-refactor/pull/269).

Goal: remove avoidable discovery and authoring calls exposed by the fresh PR 7 trace while retaining bounded evidence, independent validation, exact patch replay and undo/redo.

Measured baseline:

- The current checksum-bound projection uses 11,109 mean `fr` context tokens and 6,810 ordinary-file tokens. Its fixed action sequence has a 4,299-token or 63.1% `fr` premium.
- The fresh passing pair uses 15,458 `fr` context tokens and 42 calls. Ordinary files use 11,600 tokens and 20 calls, a 33.3% context premium.
- The PR 8 passing pair uses 13,949 `fr` context tokens and 30 calls. Ordinary files use 12,815 tokens and 23 calls, an 8.8% premium.
- Seven refused or failed `fr` requests expose command-shape and artifact-path ambiguity. Repeated single-symbol inspection exposes a missing handle-aware batching route.
- These measurements diagnose the workflow. They do not establish population-level agent behavior or predict savings from unfinished changes.

Deliverables:

- Let one `project select` request accept exact names and full revision-bound handles, with explicit scope, declaration-kind and local-omission statuses.
- Give agents a compact, machine-readable authoring contract and executable transition templates without requiring exploratory help calls.
- Make the acceptance harness preserve CLI help and provide unambiguous artifact references for manifests and fragments.
- Tighten the portable skill around observed mistakes while preserving bounded task-specific reading routes.
- Publish a checksum-bound counterfactual workflow measurement that changes only calls made unnecessary by delivered behavior and labels its limits.
- Run a fresh paired task through ephemeral Codex CLI sessions with `gpt-5.6-luna` at low effort after deterministic gates pass.
- Retain every attempted trial's prompt, events, patches, source states, independent oracles, exact reversal and token audit.

Verification and acceptance:

1. Mixed name/handle selection preserves ordered pagination and reports each selector's outcome without treating omissions as absence.
2. Stale handles, stale bases, clipped plans and conflicting evidence refuse before persistence or source writes.
3. Authoring discovery and harness artifact guidance are executable, bounded and covered by refusal regressions.
4. Counterfactual measurements bind immutable transcripts and alter only requests made redundant by implemented commands; fresh trials remain separate evidence.
5. Both arms of the fresh comparison pass project and receiver oracles, ordered checks, exact undo/redo and index-preservation checks.
6. Context reports separate skill, inspection, checks, authoring, delivery, request and latency costs.
7. The complete native, WASM, prose, capability and strict Lean gates pass.

Planned checkpoints:

1. **Complete.** Freeze the trace-derived call-reduction contract and add handle-aware multi-selection with a Lean-backed status policy.
2. **Complete.** Add a compact authoring workflow description and exact transition templates.
3. **Complete.** Repair acceptance-harness help and artifact-reference boundaries, with adversarial regressions.
4. **Complete.** Update and remeasure the portable skill routes against the observed failures.
5. **Complete.** Publish the bounded counterfactual workflow report and document what it can and cannot claim.
6. **Complete.** Retain the first fresh diagnostic, enforce original checks before source
   mutation, clarify its observed command boundaries, and retain a passing second Luna-low pair.

### PR 9. Bounded Project Query Batches

Status: merged as [PR 270](https://github.com/e6qu/fun-refactor/pull/270).

Goal: let an agent obtain several heterogeneous high-level project views from one immutable
snapshot without repeating project construction, revision coverage or process calls.

Deliverables:

- Run existing read-only `fr project` queries from one versioned manifest and one verified snapshot.
- Emit revision, handle prefix and coverage once, while retaining separate digests for the complete
  manifest and the ordered resolved request set.
- Enforce request, argument and serialized-report budgets; omit only complete reports and state the
  exact omission instead of clipping JSON or facts.
- Allow later requests to consume bounded string results from earlier requests without copying
  revision-bound handles through an agent round trip.
- Teach the portable skill an exact broad-exploration route and retain executable examples.
- Compare separate invocations with the equivalent batch under fixed inputs, payload audits and
  cold/warm cache controls before making a context or latency claim.
- Run a fresh Luna-low paired task only after deterministic evidence shows that the batch targets
  calls a broader exploration task needs.

Verification and acceptance:

1. Every returned nested report reconstructs the byte-identical standalone report from the shared
   envelope, and all queries observe one revision that is reverified before emission.
2. Invalid schemas, duplicate IDs, recursive batches, unsupported arguments, stale cursors and
   source drift refuse without returning misleading partial evidence.
3. Global budgets never emit a partial nested report; omitted reports retain identity, query kind
   and the complete byte requirement.
4. References are backward-only, string-valued, bounded and bound into the resolved request digest.
5. The Lean model proves the report-admission budget law and exhaustive shared machine-sized cases
   agree with Rust.
6. Native, WASM, documentation, skill, capability and strict Lean gates pass.

Planned checkpoints:

1. **Complete.** Add manifest-driven query batches, a common response envelope, whole-report
   budgeting, adversarial CLI regressions and a Lean-backed admission predicate.
2. **Complete.** Add backward result references so one batch can discover and then inspect exact handles.
3. **Complete.** Add portable skill guidance, executable examples and complete CLI/formal documentation.
4. **Complete.** Retain a controlled separate-versus-batch measurement with immutable payload and binary digests.
5. **Complete.** Exercise the route in a fresh broad-exploration pair after the controlled evidence
   justifies quota use, and retain both successful sessions without intervention.

### PR 10. Verified Change Workflow

Status: merged as [PR 271](https://github.com/e6qu/fun-refactor/pull/271).

Goal: carry one reviewed source-history transaction through declared validation, reversible
exercise and Git patch delivery without repeated command discovery or duplicated successful output.

Deliverables:

- Add one versioned, bounded manifest for a saved transaction, its reviewed transaction basis,
  an exact declared-check selection and an optional patch artifact.
- Preflight the complete request before source mutation. Bind the transaction, check configuration,
  patch representability and artifact destination to the report.
- Apply with compact transition evidence. Run checks against the exact resulting source and record
  their receipt on the transaction.
- Optionally undo, check the restored source, redo and check the reapplied source in one explicit
  lifecycle. Stop on failure and report the durable state reached.
- Write the Git patch only after every requested stage passes. Return its byte count and digest
  instead of copying reviewed source into the final report.
- Model the allowed workflow states and stages in Lean. Exercise every shared finite state case
  against the Rust policy.
- Teach the portable skill the route and retain a controlled comparison before another agent trial.

Verification and acceptance:

1. Unknown manifest fields, stale bases, invalid check selections, unsafe artifact paths and
   unrepresentable patches refuse before source changes or artifact creation.
2. Every mutating stage uses durable history transitions and preserves their conflict and recovery rules.
3. Checks bind their reviewed configuration and a stable source revision. Passing final checks
   record evidence; restored-state checks do not claim evidence for an applied transaction.
4. Reversal exercise starts planned, visits applied, restored and reapplied states in order, and
   finishes applied when all stages pass.
5. A failed check or transition stops later stages. The report names the completed stages and the
   current transaction status. The command writes no patch artifact.
6. Rust and Lean agree on every state and stage pair. The complete native, WASM, prose,
   capability and strict Lean gates pass.

Planned checkpoints:

1. **Complete.** Freeze the manifest, preflight rules and Lean-backed lifecycle policy.
2. **Complete.** Add compact execution, check receipts, optional reversal exercise and delayed patch delivery.
3. **Complete.** Add refusal and stopped-lifecycle regressions plus complete CLI and formal documentation.
4. **Complete.** Add the portable workflow route and executable examples within its current byte budgets.
5. **Complete.** Retain a controlled call, context and state-equivalence measurement. Its measured
   reduction warrants one fresh Luna-low adoption trial.
6. **Complete.** A fresh frozen Luna-low pair passes the coordinated workflow, project and receiver
   oracles, exact reversal and index-preservation gates. Two preceding failed diagnostics remain
   retained and identify the evaluator fixes that made the accepted rerun valid.

### PR 11. Browser Transaction History

Status: merged as [PR 272](https://github.com/e6qu/fun-refactor/pull/272).

Goal: give the in-memory WASM workspace the same transaction identity, checked stack ordering and
Git-compatible delivery semantics that an agent receives from native source history.

Deliverables:

- Record each successful browser refactoring as a bounded in-memory transaction.
  Keep a stable basis, exact before/after existence and text snapshots, and applied, undone or abandoned status.
- Undo and redo one named stack-top transaction, preflight every selected snapshot before any write,
  and preserve unrelated paths and conflicting later edits.
- Remove created files on undo, recreate them on redo, and reindex the exact changed path set.
- Export individual forward/reverse patches and one cumulative patch through the shared Rust Git
  text renderer used by native history.
- Replace the playground's whole-workspace reset and TypeScript patch renderer with WASM transaction
  controls and test the exported artifact with real `git apply`.
- Anchor the transition policy in Lean, prove the finite lifecycle and atomic multi-snapshot model,
  and compare every status/action/top-of-stack case with Rust.

Verification and acceptance:

1. Non-top, abandoned, unknown and conflicting transitions refuse before any selected write.
2. Undo/redo round trips exact text and existence, including generated files; a new edit abandons redo.
3. Cumulative exports fold applied transactions from the loaded basis and pass `git apply --check`.
4. History is bounded by record, changed-path, per-record byte and total retained-byte limits.
5. Full and minimal WASM feature sets compile; native, browser, prose and strict Lean gates pass.

Planned checkpoints:

1. **Complete.** Share the native/browser Git text renderer and repair PR 10's cold-CI evaluator stderr handling.
2. **Complete.** Add bounded checked in-memory transaction identity, stack transitions and WASM APIs.
3. **Complete.** Wire one-step undo/redo and Rust patch download into the playground; remove the duplicate renderer.
4. **Complete.** Add the anchored Lean lifecycle model and exhaustive Rust correspondence cases.
5. **Complete.** Publish the contract and continuity docs and pass the complete repository gate.

### PR 12. Revision-Bound Agent Task Bundles

Status: merged as [PR 273](https://github.com/e6qu/fun-refactor/pull/273).

Goal: let one bounded, immutable request reveal project evidence, exact edit targets, the authoring
route and the declared verification contract for an agent task.

Deliverables:

- Extend the project batch manifest into a versioned task manifest. It runs heterogeneous read
  queries against one verified revision and resolves backward references to string results.
- Resolve named task targets to exact full handles. Report their language, declaration kind, path
  and candidate high-level operations without claiming syntax preflight for unwritten fragments.
- Select declared project checks in the same request and return their exact reviewed basis,
  coverage and compact execution arguments.
- Emit bounded author-batch and verified-workflow templates whose remaining placeholders are
  explicit, along with the exact command sequence that turns fragments into a reviewed transaction.
- Bind the complete task request, resolved queries, target decisions and check selection to stable
  digests. Refuse stale handles, invalid references and unsupported requested operations.
- Teach the portable skill this route. Retain a controlled comparison with equivalent separate
  discovery, help and check-list calls before spending quota on another agent cohort.

Verification and acceptance:

1. Task queries reconstruct the same standalone reports and observe one revision reverified before emission.
2. References are backward-only, string-valued and bounded; unknown, omitted, stale or non-handle targets refuse.
3. Authoring eligibility is a conservative target-level claim. Existing author preview remains the
   syntax and overlap authority once real fragment bytes exist.
4. Check names are unique configured declarations and the returned basis is usable directly by
   `checks --run` and `workflow`.
5. Output budgets admit complete sections only and state every omission; no clipped JSON is presented as evidence.
6. Lean models the finite target-eligibility policy and report admission, with exhaustive Rust/Lean cases.
7. Native, WASM, documentation, skill, capability and strict Lean gates pass.

Planned checkpoints:

1. **Complete.** Freeze the manifest and response contract; add shared-revision queries,
   backward target references and adversarial CLI regressions.
2. **Complete.** Add conservative authoring eligibility, reusable author/workflow templates and exact declared-check selection.
3. **Complete.** Anchor eligibility and section admission in Lean and compare exhaustive machine-sized cases with Rust.
4. **Complete.** Update CLI, portable skill and continuity documentation; retain an executable generic fixture.
5. **Complete.** Measure the equivalent separate and bundled flows. The controlled result reduces
   calls by 75% and counted context by 3.0%. The complete repository gate passes. This deterministic
   evidence makes no fresh agent claim.

### PR 13. Reviewed Agent Task Changes

Status: merged as [PR 275](https://github.com/e6qu/fun-refactor/pull/275).

Goal: let an agent preview and execute one task change without manually translating task evidence
into separate author and workflow manifests.

Deliverables:

- Accept one versioned task-change manifest containing exact targets, fragment paths, postconditions,
  declared checks, reversal policy and optional patch delivery.
- Resolve every target and author all fragments against one verified project revision.
- Preview the complete source diff, validation, check selection, reversal stages and patch destination
  under one review basis without changing source or history.
- Execute only the unchanged reviewed plan. Record one transaction, run the existing checked lifecycle,
  exercise requested undo and redo, and delay patch creation until all checks pass.
- Bind source, fragments, target resolution, check configuration and delivery choices into the review
  basis so any stale input refuses before persistence or mutation.
- Model the finite prepare, execute and delivery policy in Lean and compare its complete state space
  with the Rust decision function.
- Teach the portable skill the shorter route and measure it against the equivalent task, author and
  workflow sequence with a generic fixture.

Verification and acceptance:

1. Preview performs no source, history, check or patch writes.
2. Write requires the complete preview basis and refuses source, fragment, target, check or destination drift.
3. All authoring operations retain existing syntax, overlap, preservation and postcondition checks.
4. Failed checks return structured stage evidence and never create the requested patch.
5. Requested reversal verifies the changed, restored and reapplied states in order.
6. The delivered patch, applied source and recorded transaction describe the same reviewed edits.
7. Native, WASM, documentation, skill, capability and strict Lean gates pass.

Planned checkpoints:

1. **Complete.** Freeze the combined manifest, preview basis and lifecycle boundaries.
2. **Complete.** Add atomic reviewed execution with stale-input and failure-state regressions.
3. **Complete.** Add the Lean policy and exhaustive Rust correspondence cases.
4. **Complete.** Update CLI, portable skill and continuity documentation with an executable generic fixture.
5. **Complete.** Retain a controlled comparison and pass the complete repository gate.

### PR 14. Semantic Agent Model and Authoring

Status: merged as [PR 276](https://github.com/e6qu/fun-refactor/pull/276).

Goal: let an agent inspect and change supported program behavior through versioned semantic data
without reading or writing language-specific source text.

Deliverables:

- Publish the existing cross-language module, declaration, type, statement and expression IR as a
  versioned JSON contract with canonical encoding.
- Add a revision-bound project query for a file or declaration. Omit bodies by default, admit a
  complete requested body under a node budget and redact unsupported source unless explicitly requested.
- Give returned declarations and nested semantic nodes stable revision-bound JSON-pointer addresses.
  Report coverage, unsupported constructs, omissions, fidelity and exact semantic identity.
- Identify generic functional and effect patterns already represented by the IR, including
  comprehensions, optional binding, variant matching, propagation, deferred cleanup and exception regions.
- Accept a typed semantic body for existing multi-language body targets and render it through the
  target writer. Reuse the established parser, splice, preservation and transaction checks.
- Carry semantic body replacement through author batches and the reviewed `task-change` lifecycle,
  binding the semantic payload, rendered body, project revision, checks and delivery choices.
- Add Lean-backed semantic report and author-admission policies with exhaustive Rust correspondence.
- Teach the portable skill the source-free route and measure it against bounded source-fragment authoring
  on generic fixtures. Run a fresh economical-agent comparison only after deterministic evidence passes.

Verification and acceptance:

1. Default semantic reports contain no source body, source fragment or unsupported source text.
2. Every returned body is a complete IR subtree. A budget refusal reports the required node count
   instead of clipping JSON or silently dropping children.
3. Semantic handles and pointers refuse after source, scan-option, selection or payload drift.
4. Semantic authoring accepts only the declared IR schema and supported target languages, refuses
   unsupported nodes, and reparses the rendered body in its unchanged destination context.
5. Bytes outside the selected body remain identical. Preview remains read-only; write requires the
   unchanged complete basis and retains apply, check, undo, redo and patch identity.
6. Pattern reports describe syntax-derived semantic shapes and make no unproved behavior claim.
7. Lean models the finite report and author admission boundaries, with complete Rust/Lean cases.
8. Native, WASM, documentation, skill, capability and strict Lean gates pass.

Planned checkpoints:

1. **Complete.** Freeze the public semantic schema, source-redaction boundary, addressing and budgets.
2. **Complete.** Add bounded semantic project reports and generic pattern evidence.
3. **Complete.** Add typed semantic body rendering and author-batch support.
4. **Complete.** Integrate semantic targets with reviewed task changes and lifecycle drift checks.
5. **Complete.** Add Lean policies and exhaustive Rust correspondence.
6. **Complete.** Update CLI, portable skill and continuity documentation; retain controlled context evidence.
7. **Complete.** Pass the complete repository gate and publish the large PR for review.

### PR 15. Agent IR Contract and Python SDK

Status: merged as [PR 277](https://github.com/e6qu/fun-refactor/pull/277).

Goal: make semantic IR discoverable and safe to construct without asking an agent to memorize raw
JSON. Keep Python objects visibly aligned with the public IR hierarchy.

Deliverables:

- Dogfood Rust-to-Python translation of the real IR as the SDK scaffold and retain its fidelity
  report. Repair SDK concerns that ordinary code translation cannot infer, including tagged-enum
  JSON, defaults, validation and canonical serialization.
- Add bounded CLI discovery for the body, type, statement, expression, template and operator
  vocabulary. Let an agent request one category or one variant without loading the whole contract.
- Add project-independent semantic payload validation and canonical identity reporting before an
  agent selects a project or target language.
- Ship a zero-dependency Python package with typed constructors that mirror `Type`, `Stmt`, `Expr`,
  supporting records and enum variants. Emit exact `fr-semantic-body-1` JSON.
- Keep the SDK source-free by construction. Reject category mismatches, unknown fields,
  unsupported variants, source-bearing values, invalid operators and non-finite nesting.
- Derive executable constructor examples from the SDK and check every one through Python, Rust
  deserialization and Rust canonical serialization.
- Model the finite category, kind and author-admission boundary in Lean. Compare all supported and
  refused category-kind combinations with Rust, then check the Python catalog against the same set.
- Teach the portable agent skill both direct JSON and Python routes. Retain a controlled comparison
  of correctness, repair behavior, payload size and required contract context on generic fixtures.

Verification and acceptance:

1. CLI schema pages and Python constructor catalogs name the same versioned IR variants.
2. Every SDK example round-trips through Python serialization and Rust canonical serialization.
3. A node cannot cross type, statement or expression categories through an SDK constructor.
4. Semantic authoring rejects unsupported, source-bearing, oversized, excessively nested and
   unknown inputs before reading a project.
5. Lean proves finite catalog uniqueness and author-admission properties; exhaustive Rust cases
   and Python-generated fixtures agree with the model.
6. Measure the translated scaffold's gaps. Documentation distinguishes translation output
   from the checked SDK adapter rather than claiming automatic SDK generation.
7. Native, Python, documentation, skill, capability, strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Run `fr` Rust-to-Python translation on the actual IR and record its limits.
2. **Complete.** Freeze the discoverable semantic catalog and project-independent validator.
3. **Complete.** Build the typed Python SDK and exhaustive cross-runtime conformance suite.
4. **Complete.** Add Lean catalog and admission proofs with Rust and Python correspondence.
5. **Complete.** Teach both agent routes and retain deterministic and fresh Luna/low comparisons.
6. **Complete.** Pass the complete repository gate and publish the large PR for review.

### PR 16. Checked Semantic Delta Authoring

Status: merged as [PR 278](https://github.com/e6qu/fun-refactor/pull/278).

Goal: let an agent make a small typed semantic change without reproducing an entire function body
or reading language-specific source.

Deliverables:

- Add a versioned `fr-semantic-change-1` contract bound to the canonical identity of one source-free
  semantic body. Keep operation order explicit and cap input bytes, operation count and result size.
- Support typed node replacement for types, statements, expressions and template parts, plus
  statement insertion and deletion. Resolve RFC 6901 pointers against the current result and refuse
  missing paths, category mismatches, no-ops and invalid intermediate bodies.
- Add project-independent change validation and application. Return canonical input and result
  identities without scanning a project or reading implementation source.
- Report body identity with declaration semantic queries. Apply a semantic delta directly to an
  exact function handle, then reuse writer fidelity, reparse, byte-preservation and transaction checks.
- Carry semantic deltas through author batches and reviewed task changes, including source, fragment,
  base-identity, result and delivery drift checks.
- Extend the Python SDK with change objects that mirror the public operation hierarchy and accept
  only the corresponding typed IR nodes.
- Model operation admission, sequential bounds and statement-list size transitions in Lean. Anchor
  the critical Rust predicates and compare their complete bounded state spaces.
- Teach the portable skill the delta route and compare it with whole-body replacement on generic
  fixtures. Run a fresh economical-agent pair when deterministic evidence passes.

Verification and acceptance:

1. Every accepted operation changes exactly its selected semantic subtree or statement-list slot.
2. The base body identity must match before any operation; every intermediate and final body remains
   strict, source-free `fr-semantic-body-1` within the existing statement and node limits.
3. Project-independent application performs no project scan. Project authoring reads no fragment
   containing source code and preserves bytes outside the selected function body.
4. Missing and escaped pointers, wrong node categories, invalid indices, duplicate/no-op changes,
   unsupported nodes and stale body identities refuse before source or history mutation.
5. Preview remains read-only. Batch and task-change writes retain atomic apply, checks, undo, redo
   and Git patch identity.
6. Lean states the finite admission and size laws. Exhaustive Rust correspondence and
   Python-generated fixtures connect the model, SDK and implementation without proving Serde or Python.
7. Native, Python, documentation, skill, capability, strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Freeze the semantic-change contract, pure application engine and Python builders.
2. **Complete.** Add body identity reporting and direct semantic-delta body authoring.
3. **Complete.** Integrate author batches, reviewed task changes and lifecycle drift checks.
4. **Complete.** Add Lean models, source anchors and exhaustive correspondence tests.
5. **Complete.** Teach the agent workflow, retain controlled and fresh comparisons, expose exact
   body pointers, and pass the complete repository gate.

### PR 17. Reviewed Semantic Intent Operations

Status: merged as [PR 280](https://github.com/e6qu/fun-refactor/pull/280).

Goal: let an agent state common exact changes with typed semantic roles and scalar values. The agent
does not need to construct a complete replacement node or navigate serialization-only `value` fields.

Deliverables:

- Add a versioned, source-free `fr-semantic-intent-1` contract bound to one canonical semantic-body
  identity. Cap input bytes, intent count, locator depth, resolved targets and generated delta
  operations.
- Publish a finite semantic-role vocabulary over the existing type, statement, expression and
  template hierarchy. Role locators carry expected category, kind and selected scalar evidence;
  missing, ambiguous and mismatched steps refuse instead of choosing an implicit first match.
- Add shape-preserving scalar intents for integer, float, string and Boolean literals. Cover name
  expressions, field and keyword names, binary and unary operators, template text and comments.
  Keep binding-wide rename and inferred behavior changes outside this contract until the IR carries
  the identity needed to state them honestly.
- Compile every accepted intent into ordered `fr-semantic-change-1` replacements and run the existing
  checked delta engine. Compare the compiler result with direct typed intent interpretation before
  returning it, retaining resolved pointers and both identities as audit evidence.
- Add project-independent compile/apply commands and direct project authoring. Carry intent inputs
  through author batches, project tasks and reviewed task changes with the existing checks, undo,
  redo and Git patch lifecycle.
- Mirror locators and intents in the zero-dependency Python SDK while keeping direct JSON first-class.
  Teach the portable skill to choose among whole bodies, typed deltas, Python construction and
  semantic intents from measured evidence.
- Model locator determinism, exact resolution, scalar locality, shape preservation and compiler
  refinement in Lean. Anchor small Rust admission and transition kernels, exhaust their bounded
  domains and compare generated typed-tree cases with Rust.
- Compare whole-body, pointer-delta, Python-SDK and intent routes on generic fixtures. Run fresh
  rotated Luna-low trials only after the deterministic intent route reduces total counted context
  and repair calls.

Verification and acceptance:

1. A resolved role locator identifies exactly one typed node under the supplied body identity.
   Missing roles, ambiguous selectors, invalid indices and category, kind or scalar mismatches refuse.
2. Every accepted scalar intent changes one declared scalar slot while preserving the target node's
   category, kind and other children. Ordered intents see the result of every preceding intent.
3. Direct typed interpretation and application of the compiled semantic delta produce the same
   canonical body and result identity. Generated pointers are distinct or explicitly ordered, and
   total generated operations remain within the public bound.
4. Stale bases, unsupported nodes, malformed scalar values, invalid operators, no-ops and input,
   locator, match or result limit breaches refuse before project or history mutation.
5. Project authoring preserves bytes outside the selected function body. Batch and task-change
   writes retain atomic checks, exact undo and redo, and forward and reverse Git patch identity.
6. Lean proves the abstract locator and scalar-compiler laws. Anchors and exhaustive generated cases
   connect those models to Rust while keeping Serde, SHA-256, parsers, writers and Python outside the
   proof boundary.
7. Deterministic comparisons, Python conformance, portable-skill execution, native, strict Lean and
   WASM gates pass before any fresh agent run is retained.

Planned checkpoints:

1. **Complete.** Freeze the role, locator, intent, budget and receipt contracts and repair stale
   PR 16 roadmap and evaluation metadata.
2. **Complete.** Implement typed role resolution, direct intent interpretation, checked delta compilation and the
   project-independent CLI with adversarial refusal coverage.
3. **Complete.** Add project authoring, compact reviewed receipts, author-batch, project-task and task-change
   integration through the complete source-history lifecycle.
4. **Complete.** Add the Python mirror, generated cross-runtime fixtures, semantic catalogs and executable skill
   routes.
5. **Complete.** Add Lean models, source anchors, refinement theorems and exhaustive Rust correspondence.
6. **Complete.** Cross-language and four-route controlled evidence is retained. A fresh
   Luna-low pair passes exact semantics and behavior in both arms. Semantic intent uses 470 payload
   bytes and seven commands, versus 1,930 bytes and nine commands for complete-body authoring. It
   uses fewer total input and output tokens, while input excluding reported cache hits is 18.8%
   higher. The complete native, strict Lean and WASM gate passes. PR 280 carries the completed checkpoint.

### PR 18. Reviewed Semantic Edit Plans

Status: merged as [PR 281](https://github.com/e6qu/fun-refactor/pull/281).

Goal: let an agent discover and apply one exact scalar semantic edit without guessing a file path,
constructing a JSON manifest or reading source. Preserve the reviewed semantic-intent contract as
the audit representation used by every shorter route.

Deliverables:

- Let a semantic declaration query resolve one exact eligible name below a file or directory target.
  Report the selected revision-bound handle and refuse zero or multiple matches without choosing by
  traversal order.
- Add a bounded edit-plan request over operation, exact current scalar and requested scalar. Resolve
  exactly one role locator, validate the generated `fr-semantic-intent-1` payload and return its
  canonical body basis, target and intent identity as review evidence.
- Add direct project authoring for the same request. Preview and write compile the generated intent
  through the existing checked delta engine. Preserve body fidelity, surrounding bytes, history,
  undo, redo and Git patch behavior.
- Carry the plan through author batches, project tasks and reviewed task changes where the shorter
  form remains complete. Keep the explicit semantic-intent manifest route for multi-operation and
  ambiguous edits.
- Mirror the plan request in the zero-dependency Python SDK without hiding the public intent shape.
  Update the portable skill with one exact route and explicit fallbacks.
- Model unique candidate selection, scalar request admission and generated-intent equivalence in
  Lean. Anchor the critical Rust predicates and compare their complete bounded state spaces.
- Compare the direct plan with the PR 17 manifest route on generic cross-language fixtures. Retain
  a fresh Luna-low pair only after deterministic evidence reduces commands and counted context.

Verification and acceptance:

1. Directory and file discovery returns one eligible exact declaration or a structured zero/many
   refusal; source and unsupported semantic text never enter the report.
2. Plan admission requires valid operation and scalar encodings. One generated locator must resolve
   to the exact current scalar under the reported body identity.
3. Applying the plan produces the same canonical intent, compiled delta, semantic body and source
   bytes as the explicit PR 17 route.
4. Preview changes no files. Stale revisions, changed bodies, ambiguity, no-ops, unsupported writers and
   invalid values refuse before source or history mutation.
5. Direct, batch and task-change writes retain checks, exact undo and redo, and forward and reverse
   Git patch identity.
6. Lean proves unique-selection and admission laws. Exhaustive Rust correspondence and generated
   cross-language cases connect the model to the implementation while retaining the documented
   parser, writer, Serde, SHA-256 and filesystem trust boundaries.
7. Native, Python, documentation, portable-skill, capability, strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Freeze the source-free discovery, plan, ambiguity and lifecycle contract.
2. **Complete.** Implement workspace-scoped declaration resolution and project-independent planning.
3. **Complete.** Add direct authoring plus batch, task and task-change integration.
4. **Complete.** Add Python builders, Lean models, anchors and exhaustive correspondence.
5. **Complete.** The deterministic comparison saves one command and 21.2% of counted bytes. The
   corrected fresh Luna-low pair passes both arms without source reads. The direct route removes the
   query and payload, with lower total, non-cached input and output in this one pair.
6. **Complete.** The complete native, browser, documentation, capability and Lean gate passes.

### PR 19. Incremental Project Identity and Agent Query Latency

Status: merged as [PR 282](https://github.com/e6qu/fun-refactor/pull/282).

Goal: keep revision-bound project queries responsive across the repeated inspect, edit and verify
loop that an agent performs. Preserve complete workspace identity, stale-handle refusal,
coverage and deterministic output while avoiding work whose inputs are already known unchanged.

The initial dogfood profile queried one exact declaration in this repository. A populated fact and
resolution cache still spent 10.56 seconds in the debug build. Reference serialization took 7.85
seconds, symbol serialization took 0.93 seconds and the second source hash took 0.72 seconds.
Changing only the development profiler invalidated the workspace resolution snapshot and made the
next call spend 66.05 seconds rebuilding resolution before another 9.64-second project construction.
After ignore rules exclude generated evidence, the profile contains 957 indexed files and 435,556 references.
These are single-host diagnostic measurements, not release latency claims.

Deliverables:

- Define a versioned project-revision material contract from selected scope, scan policy, captured
  manifests, source identities, extraction semantics and reported gaps. Revision construction must
  avoid serializing derived symbols and references when those inputs already determine them.
- Compute cryptographic content identities once during parallel indexing and reuse them for
  workspace cache keys, project revisions and source-race guards. Remove the current 64-bit cache-key
  collision boundary without weakening exact stale-handle refusal.
- Reuse cached resolution after edits whose extracted resolution inputs are unchanged. Make cache
  admission explicit, deterministic and fail closed when file order, fact shape, candidate sets or
  target identities differ.
- Give long JSON project calls bounded machine-readable progress for fact extraction and reference
  resolution. Successful stdout remains the same report contract.
- Make cache placement work in restricted agent environments through an explicit, inspectable
  fallback that does not add project files or silently disable reuse.
- Repair and extend the development profiler so its public command line is executable and reports
  the files and languages responsible for reference cost. Retain before/after cold, warm and
  post-edit measurements with byte-identical query results.
- Add documented ignore rules for immutable evaluation transcripts and generated site payloads.
  Retain their replay and build use outside the code index.
- Model revision-material sufficiency, ordered identity changes and cache-admission guards in Lean.
  Anchor the critical Rust predicates and compare bounded generated cases with independent oracles.

Verification and acceptance:

1. Any captured source, manifest, scan-policy, extraction-semantic or reported-gap change alters the
   project revision; unchanged material produces the same revision regardless of cache state.
2. A source edit cannot reuse a resolution snapshot unless the complete resolution projection and
   stable target mapping still match. Corrupt, partial, reordered and stale snapshots refuse reuse.
3. Cached and uncached reports remain byte-identical. Old handles and context bases refuse after
   every revision-relevant change, including changes that preserve file length.
4. The repeated exact-declaration dogfood query completes without source reads by the caller and
   materially reduces warm and post-scalar-edit latency on this repository.
5. Progress records are valid standalone JSON lines on stderr, monotonic within each phase and absent
   from stdout. Short calls may complete without emitting progress.
6. Native, Python tooling, documentation, portable-skill, capability, strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Freeze the revision material, cryptographic content identity, cache-admission and
   progress contracts; retain the initial dogfood profile and fix its broken invocation.
2. **Complete.** Implement shared content identities and the versioned project revision without
   derived-fact serialization; prove and test revision sensitivity.
3. **Complete.** Add conservative incremental resolution reuse, an integrity envelope and
   adversarial snapshot tests.
4. **Complete.** Add private restricted-environment cache fallback, ordered phase progress and
   portable-skill guidance.
5. **Complete.** Retain deterministic cold, warm and post-edit comparisons on a generic fixture and
   dogfood the same exact-declaration query on this repository.
6. **Complete.** Pass the complete repository gate and a fresh Luna-low dogfood trial before review.

### PR 20. Bounded Agent Discovery and Concurrent Query Coalescing

Status: merged as [PR 283](https://github.com/e6qu/fun-refactor/pull/283).

Goal: make the efficient project-query route hard for an agent to accidentally bypass. Bound every
discovery response, reuse one warmed project view across a task, and coalesce concurrent cold work
whose complete inputs match.

The PR 19 Luna-low dogfood run found the correct Rust and Lean boundary, but the first attempt used
239,321 input tokens. It issued independent cold queries in parallel, widened limits to 100 and read
far more structure than the task required. A second attempt repeated exact queries, requested a
500-row map and broad source bodies, so it was stopped before completion. These runs establish
functional discoverability and an efficiency failure. They do not establish context savings.

Deliverables:

- Add one task-scoped query session or equivalent batch surface. It captures one revision and serves
  bounded follow-up selections without rebuilding the project.
- Coalesce concurrent cache misses for identical resolution inputs across processes, with bounded
  waiting, stale-owner recovery and no partial snapshot admission.
- Put server-enforced row and source-byte budgets on agent discovery profiles. Report truncation and
  the exact continuation action in the same machine-readable response.
- Provide a compact behavior-discovery route that starts with one name-only search. It can expand to
  an exact declaration, relationships and only the needed source.
- Retain prescribed and fresh-agent evidence that separates correctness, context, calls, latency and
  cache state. Failed and interrupted trials remain part of the result.
- Model lock ownership, completed-snapshot publication and bounded pagination transitions in Lean;
  connect critical predicates to Rust with anchors and shared cases.

Verification and acceptance:

1. Parallel identical cold queries perform one resolution build and return byte-identical reports;
   killed owners cannot permanently block later work.
2. Agent-profile limits stay within their reviewed ceiling unless an explicit mode change records
   the wider value in the response.
3. A fresh Luna-low behavior task finishes through the documented route without direct file reads,
   broad maps or repeated cold queries. Retain its complete context.
4. Stale handles, incomplete snapshots, changed sources and mismatched resolution material continue
   to refuse before a result or mutation is accepted.
5. Native, WASM, strict Lean, portable-skill, documentation and deterministic replay gates pass.

Planned checkpoints:

1. **Complete.** Add compact and explicitly expanded `project explore` profiles with fixed row,
   relationship, source and report ceilings. Return exact argument arrays for every available
   continuation.
2. **Complete.** Admit only bounded exploration inside profiled project batches, bind the selected
   profile into its manifest identity and reuse one constructed project across referenced stages.
3. **Complete.** Give each resolution input one filesystem-backed owner, wait for atomic
   publication, recover stale owners and bound live-owner waits. Recheck completed snapshots after
   taking ownership so a publication race cannot start a duplicate build.
4. **Complete.** Model discovery budgets, mode transitions, waiting actions and owner-only snapshot
   publication in Lean, with 3,472 shared Rust/Lean cases.
5. **Complete.** Retain deterministic and repository dogfood evidence, update the portable skill
   and command documentation, and fix defects exposed by the new route.
6. **Complete.** Pass the complete native, WASM, strict Lean, capability, skill and documentation
   gate before review.

### PR 21. Merkle-Committed Progressive Agent Disclosure

Status: merged as [PR 284](https://github.com/e6qu/fun-refactor/pull/284).

Goal: let an agent inspect and manipulate a declaration through the smallest useful semantic
hierarchy. Commit every hidden part to one revision-bound view, reveal source only through an
explicit action, and enforce a conservative token ceiling on every response.

Deliverables:

- Add `project disclose` over a full declaration handle. Its initial response returns separate
  semantic-IR and exact-source holes under one SHA-256 Merkle commitment, without either payload.
- Reveal one semantic JSON level at a time. Inline bounded scalars, keep composite values as
  individually addressed holes, and paginate large child sets and strings through exact actions.
- Reveal exact source only through its separate hole, in UTF-8-safe pages that reconstruct the
  committed declaration byte for byte.
- Enforce compact and explicitly expanded per-response ceilings. Report serialized UTF-8 bytes
  including the newline as a conservative upper bound for byte-fallback tokenizer tokens.
- Compose discovery, frontier creation and semantic reveal in one profiled project batch by allowing
  references to earlier handles and holes.
- Publish the protocol in the CLI, portable skill and a verifier-oriented document. Retain a generic
  agent-style fixture with a separate Python Merkle oracle.
- Model budget admission, reveal extent and completed frontier replacement in Lean, with strict
  source anchors and shared executable correspondence cases.

Verification and acceptance:

1. Initial and semantic responses contain no exact source. Every composite or paged omission has a
   machine-copyable action bound to revision, target, view, profile, limit and offset.
2. Every emitted JSON line stays within its declared upper bound. Compact accepts at most 4,096 and
   expanded at most 16,384; invalid or incompatible continuations refuse.
3. Semantic child digests, semantic root, source root, combined root and root-hole identity agree
   with an independent implementation. A hidden change alters the commitment.
4. Source pages preserve UTF-8 boundaries and reconstruct the exact declaration. Source changes
   invalidate the old handle, holes and cursors before disclosure.
5. Profiled batches perform discovery, frontier selection and a referenced reveal in one verified
   project view. Exact actions carrying a leading `project` remain valid batch arguments.
6. Lean builds warning-free, strict source correspondence passes, Rust agrees on all shared boundary
   cases, and native, Python, skill, documentation, capability and WASM gates pass.

Planned checkpoints:

1. **Complete.** Implement versioned Merkle roots, opaque semantic/source holes, revision-bound view
   identity and a source-free initial frontier.
2. **Complete.** Add one-level semantic reveals, cursor-bound child and string pages, UTF-8 source
   reconstruction and strict compact/expanded response budgets.
3. **Complete.** Compose discovery and disclosure through referenced profiled batches; accept exact
   emitted project actions inside batch manifests.
4. **Complete.** Prove numeric ceilings, admitted offsets and exact completed-frontier replacement;
   add shared Rust/Lean boundary execution.
5. **Complete.** Adversarial CLI coverage, independent-oracle evaluation, portable skill, protocol,
   roadmap, continuity and defect records are current.
6. **Complete.** The complete local repository gate and every PR 284 CI job pass on the reviewable
   implementation, verification, evaluator, skill and documentation commits.

### PR 22. Disclosure-Bound Semantic Editing

Status: merged as [PR 285](https://github.com/e6qu/fun-refactor/pull/285).

Goal: let an agent turn one progressively revealed scalar into an exact reviewed change without
reading source, reconstructing a pointer or writing a semantic-intent locator. Preserve the typed
intent compiler and complete history, check and Git lifecycle as the authority for the change.

Deliverables:

- Attach an opaque `frde1:` edit capability and exact preview template to every revealed scalar.
  Emit them only where the current source-free reader and body writer can safely author.
- Bind each capability to the project revision, full declaration handle, canonical body identity,
  exact scalar pointer, operation, current value and typed role locator. Distinguish equal values at
  different locations and refuse malformed, unknown, stale, ambiguous and unchanged requests.
- Compile the selected capability through the existing semantic-intent and checked-delta path.
  Preserve bytes outside the body and carry preview, plan-basis write, undo, redo and Git patch
  behavior unchanged.
- Carry the two-field `{edit,to}` request through author batches, project tasks and reviewed task
  changes. Mirror that shape in the zero-dependency Python SDK.
- Keep large current and replacement scalars as tagged commitments in disclosure and author
  receipts. Add an editable-descendant count to shortcuts so an agent does not reveal unrelated
  semantic nodes.
- Model exact edit admission and the expanded task-target policy in Lean. Connect them to Rust with
  strict anchors and exhaustive finite execution.
- Retain a generic independent evaluator that calculates capability identities without calling the
  implementation's hash path. Demonstrate ambiguity resolution, runtime behavior, patches and reversal.

Verification and acceptance:

1. Two equal scalars at distinct typed locations receive different capabilities. Selecting either
   changes only that scalar and preserves every byte outside the selected function body.
2. Capabilities admit exactly one current candidate and a changed, operation-valid replacement.
   Malformed, unknown, stale, ambiguous and no-op requests refuse before history or source mutation.
3. Initial disclosure and semantic navigation remain source-free and within their declared bounds.
   Long scalars remain commitments in both capability and author receipts while staying editable.
4. Direct, batch and task-change writes retain semantic-intent refinement, declared checks, exact
   undo and redo, and forward and reverse Git patch identity.
5. Lean proves the finite admission implications and task-target equivalence. Rust agrees on all
   144 admission states and the complete 1,782-case target matrix. Hashing, parsers, writers,
   compilers and filesystem behavior remain explicitly trusted or integration-tested boundaries.
6. The deterministic evaluator, Python SDK, portable skill, docs, native tests, strict Lean build,
   capability matrix and WASM/playground gates pass before review.

Planned checkpoints:

1. **Complete.** Implement exact disclosed-scalar identities, duplicate-value selection, direct
   author preview/write and stale/no-op/malformed refusal.
2. **Complete.** Carry disclosed requests through batch, task and task-change transactions with
   checks, reversal and patch delivery.
3. **Complete.** Add long-value commitments, editable shortcut counts and the Python request mirror.
4. **Complete.** Prove admission and task-target policy, anchor the Rust predicates and exhaustively
   compare the finite models.
5. **Complete.** Retain the independent generic evaluator and refresh the progressive-disclosure
   evidence after the response-shape extension.
6. **Complete.** Prose, portable-skill and defect records are current. The full native, WASM,
   playground, strict Lean, capability, Python and documentation gate passes locally.
7. **Complete.** Rebase onto release 0.19.0 and make retained context evidence stable across
   first party version-only lockfile changes while preserving third party dependency identities.
8. **Complete.** Repair optional callback precedence and qualified callback parameters found by
   the post-release deep audit. A default regression and the complete repository round trip pass.

### PR 23. Disclosure-Bound IR Structure Editing

Status: merged as [PR 287](https://github.com/e6qu/fun-refactor/pull/287).

Goal: let an agent replace a typed semantic node or change statement-list structure from a
progressively revealed IR region. The route avoids reconstructing a JSON pointer or reading source.
Keep the existing checked semantic-change engine, body writer, transaction history and Git
lifecycle as the authority for every source edit.

Deliverables:

- Attach opaque `frdi1:` capabilities to authorable typed nodes and statement-list positions.
  Support same-category node replacement, statement deletion, insertion before an existing
  statement and append to any statement list, including an empty one.
- Bind every capability to the project revision, full declaration handle, canonical body basis,
  operation, typed category, exact path/index and current semantic Merkle commitment. Refuse
  malformed, unknown, stale, ambiguous, mismatched, oversized and no-op requests before mutation.
- Accept only `{edit,value?}` at the capability boundary. Direct CLI reads an optional bounded IR
  node file; author batches, project tasks and reviewed task changes carry the same inline shape.
- Mirror the request in the zero-dependency Python SDK and point agents to the existing semantic
  catalog and constructors for the required node category.
- Preserve progressive disclosure's source-free reports and explicit response budgets. Report
  structural-edit counts on shortcuts and keep large current or replacement nodes as commitments.
- Retain a generic deterministic evaluator that derives capability identities independently and
  exercises all four edit shapes. Compile behavior and validate stale refusal, patches, undo and
  redo. Compare its route with an explicit semantic-change manifest before making a context claim.
- Model capability admission, request shape, operation/category policy and statement-position
  boundaries in Lean. Add strict Rust anchors and exhaustive shared finite cases.

Verification and acceptance:

1. Two same-shaped nodes at different locations receive distinct capabilities. One selected
   capability changes only its target and preserves every byte outside the selected function body.
2. Replace accepts exactly one source-free node in the bound category. Delete accepts no value.
   Insert accepts one source-free statement at its bound position; empty-list append works.
3. A changed body, node/list commitment, operation, category, position, declaration or revision
   invalidates the capability before history creation. Malformed and unchanged requests refuse.
4. Direct, batch and task-change writes retain semantic-change validation, declared checks, exact
   undo and redo, and forward and reverse Git patch identity across supported body languages.
5. Every disclosure response remains within its requested token ceiling. Structural descriptors
   contain no source or hidden node value, and committed large values do not reappear in receipts.
6. Lean proves the finite admission and operation implications. Rust agrees on every shared Boolean,
   category, operation and statement-boundary case; hashing, parsers, writers, compilers and
   filesystems remain explicit trusted or integration-tested boundaries.
7. The deterministic evaluator, Python SDK, portable skill, docs, native tests, strict Lean build,
   capability matrix and WASM/playground gates pass before review.

Planned checkpoints:

1. **Complete.** Fix B859, found by the first self-disclosure: source-bearing bodies remain readable
   and expose no edit capabilities instead of failing typed-IR reconstruction.
2. **Complete.** Implement structural target enumeration, identities, bounded descriptors and direct preview/write.
3. **Complete.** Carry structural requests through author batches, project tasks and reviewed task changes.
4. **Complete.** Add the Python request mirror, semantic-catalog guidance and portable skill route.
5. **Complete.** Prove admission and position policies, anchor Rust and exhaustively compare the finite models.
6. **Complete.** Retain the generic independent evaluator, compare explicit/capability routes and refresh affected
   context evidence.
7. **Complete.** Close adversarial coverage, defect and continuity records. The complete default,
   WASM, strict-proof, skill and evidence gates pass locally. Hosted CI builds and exercises the
   browser playground before review.

### PR 24. Content-Addressed Progressive Project Evidence

Status: merged as [PR 288](https://github.com/e6qu/fun-refactor/pull/288).

Goal: make one bounded progressive-reveal protocol the agent-facing navigation layer for semantic
IR, project hierarchy, call traces, impact and value-flow endpoints. Agents fetch only the relevant
branches; clients can retain those branches in local or remote object storage by stable digest.
Optional proofs and formal models test the content-addressing and traversal implementation without
adding proof material to ordinary agent context.

Deliverables:

- Add an explicit source-free evidence view to `project disclose`. Commit declaration context,
  bounded code-map hierarchy, incoming and outgoing call traces, impact candidates, and local
  value-flow origins and destinations in one deterministic Merkle tree. Preserve confidence,
  omissions and analysis boundaries as data.
- Give every semantic and evidence subtree a reusable content digest. Define a canonical,
  deduplicated object-pack shape in the zero-dependency Python SDK. A local or remote object store
  can retain and fetch unchanged branches independently.
- Offer Merkle inclusion paths explicitly with `--proofs` for protocol tests, cache-boundary audits
  and independent SDK verification. Keep them out of ordinary agent responses, where roots and
  child object digests are enough for progressive retrieval and consume much less context.
- Make shortcuts describe the evidence domains and their hidden descendant counts. Exact returned
  actions traverse hierarchy, trace branches, source and sink endpoints, and semantic nodes without
  reconstructing pointers or ingesting file text.
- Keep the existing semantic view and opaque scalar/structural edit capabilities compatible. The
  evidence view exposes no source and permits no edits. The exact-source frontier remains a last
  resort bound to the same declaration snapshot.
- Model proof-step admission, view/domain transitions and frontier arithmetic in Lean. Anchor the
  Rust predicates and compare exhaustive finite cases plus independent hash vectors. Use these
  checks as implementation tests; agents normally consume object addresses and exact reveal
  actions. Hash collision resistance, parser correctness and analysis completeness remain named
  trust boundaries.
- Run the parser and complete translation corpus across every advertised language pair. Fix every
  reproducible parser or writer regression exposed by the audit, record unsupported constructs
  precisely, and add round-trip or compile/runtime regressions for repaired forms.
- Repair B862 from post-merge deep validation. A `#[path]` attribute may invalidate only the Rust
  module it remaps.
- Retain a generic evaluator that independently verifies inclusion proofs and follows every
  evidence domain under small budgets. Detect tampering and stale snapshots. Compare the
  progressive route with direct bounded project reports.

Verification and acceptance:

1. A client can verify a fetched object from its value and content digest. It can also check an
   opt-in reveal from its inclusion proof and advertised tree root alone. A
   changed key, index, sibling digest, direction, domain, target or snapshot fails verification.
2. Compact evidence starts without source lines. Its hierarchy, callers, callees, impact rows,
   value origins and destinations are source-free projections of the direct analyzers and retain
   their confidence and explicit incomplete boundaries.
3. Every response and continuation stays within the requested compact or expanded ceiling. An
   opt-in proof that cannot fit refuses without silently omitting authentication data.
4. Existing semantic disclosure identities, authoring capabilities, checked changes, undo/redo and
   Git patch behavior remain unchanged.
5. Lean establishes the finite proof and transition laws; Rust agrees on the complete shared case
   sets. Independent Python hashes and verifies real response proofs without calling Rust helpers.
6. The complete parser/translation sweep has no newly carried construct or advertised pair that
   refuses. Each repaired syntax form has a focused regression and generated output reparses.
7. Default, deep, WASM/playground, strict Lean, capability, Python SDK, portable-skill, prose and
   retained-evidence gates pass before review.

Planned checkpoints:

1. **Complete.** Repair the deep-validation Rust module-path false refusal. Pin unrelated and
   remapped cases.
2. **Complete.** Add content-addressed object packs and opt-in inclusion proofs. Test independent
   Python verification, tampering, stale state and budgets.
3. **Complete.** Add the source-free project-evidence tree. Provide progressive shortcuts for
   hierarchy, calls, impact, origins and destinations.
4. **Complete.** Formalize proof reconstruction and evidence-view transition policy. Add strict
   anchors and exhaustive Rust/Lean correspondence.
5. **Complete.** Run all parsing and translation audits. Fix concrete regressions and record
   remaining evidence limits.
6. **Complete.** Retain the independent progressive-evidence evaluator. Update SDK and skill
   guidance, refresh context evidence, and pass all native, browser, proof and documentation gates.

### PR 25. Agent Formalization Workbench

Status: merged as [PR 290](https://github.com/e6qu/fun-refactor/pull/290).

Goal: let an agent discover conservative formalization candidates and inspect a source-free semantic
plan. It can create a Lean kernel and solve one exact proof goal without ingesting or rewriting whole
source or model files. Preserve explicit evidence levels so generated syntax never becomes a claim
of general implementation correspondence.

Deliverables:

- Discover top-level Rust functions whose explicit types and pure bodies fit a deterministic Lean
  subset. Report every exclusion as a concrete boundary and return exact planning actions.
- Emit `fr-formal-plan-1` with typed bindings, semantic IR, generated Lean, reviewed properties,
  assumptions, obligations and separate correspondence claims. Bind every semantic field to a
  Merkle object digest and regenerate the plan from current source before accepting it.
- Scaffold generated definitions and named theorem regions from the plan. Preserve existing proof
  regions and handwritten extensions across regeneration while source history keeps preview,
  reviewed plans, undo, redo and Git patch behavior.
- Disclose a compact goal catalog and one selected theorem context by content digest under an exact
  serialized-byte ceiling. Return proof and verification commands without including unrelated
  model bodies.
- Replace one named proof region from a tactics-only file. Reject traversal, ambiguous markers,
  placeholder/marker injection and invalid Lean syntax before preparing a transaction.
- Mirror the formal plan shape in the zero-dependency Python SDK. Validate exact fields and the
  independent Merkle content address before agents consume or store a plan.
- Model candidate and property admission in Lean, anchor both Rust predicates and compare every one
  of the 48 Boolean inputs with the Lean executable.

Verification and acceptance:

1. Candidate discovery admits pure typed functions and excludes an effectful function without
   exposing source text. Unsupported syntax refuses instead of producing an approximate kernel.
2. A saved plan scaffolds only while every source, IR, type, property, obligation and digest field
   matches deterministic regeneration. Source drift or JSON tampering refuses before mutation.
3. Generated Lean builds after its disclosed obligations receive valid proofs. Regeneration retains
   those exact proof bytes; strict source anchors and signature maps remain the verification gate.
4. Goal catalogs and selected details fit their byte ceiling, use content addresses and disclose
   only the requested theorem context plus exact continuation actions.
5. Proof writes change one named region and retain exact undo/redo behavior. Invalid targets,
   duplicate regions, placeholders and malformed Lean refuse before history creation.
6. Lean proves the finite admission policies and Rust agrees on all shared cases. Python recomputes
   real plan addresses and rejects tampering. General Rust/model equivalence remains explicitly
   unproved outside separately generated executable or correspondence evidence.
7. Native, Python, documentation, portable-skill, capability, strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Add conservative candidate discovery and content-addressed formal plans.
2. **Complete.** Generate checked Lean definitions and proof-preserving named theorem regions.
3. **Complete.** Add Merkle-addressed, budgeted goal disclosure and exact proof-region writes.
4. **Complete.** Mirror and validate plans in Python; formalize admission and exhaust finite cases.
5. **Complete.** Cover the complete workflow, stale refusal and history reversal in integration tests.
6. **Complete.** Refresh agent guidance, continuity and evidence; pass the complete repository gate.

### PR 26. Agent Proof Companion

Status: merged as [PR 291](https://github.com/e6qu/fun-refactor/pull/291).

Goal: give an agent a small, revision-bound Lean proof task and a deterministic feedback loop. The
agent authors every proof tactic. `fr` supplies context, empty templates, checking, diagnostics and
the existing reversible write path.

Deliverables:

- Build one content-addressed proof task from an exact generated obligation. Include the theorem,
  anchors, proof input contract, empty structural templates and exact next actions under a byte
  ceiling.
- Check an agent-written tactics file with the package's pinned Lean toolchain before any workspace
  mutation. Return structured, bounded diagnostics and a receipt bound to the goal and proof bytes.
- Require the same Lean check when `spec prove` previews, saves or applies a proof. Refuse stale
  goals, invalid tactics and toolchain failures before history creation.
- Mirror and validate proof tasks and receipts in the Python SDK so an agent can retain their exact
  shapes without reading Lean files.
- Model proof-submission admission in Lean and exhaust the shared finite policy against Rust.
- Cover failed attempts, correction, successful application, regeneration, undo, redo and strict
  verification in the end-to-end workflow.

Verification and acceptance:

1. The task contains no completed proof or source body. Templates reserve positions for tactics the
   agent must write.
2. A wrong proof returns bounded structured Lean diagnostics and changes no workspace file or
   history state.
3. A successful receipt binds the current goal digest and normalized tactics digest. Any changed
   inserted proof or goal produces a different receipt.
4. `spec prove` cannot prepare a change unless Lean accepts the exact updated module.
5. Lean proves the finite admission policy and Rust agrees for every input.
6. Native, Python, documentation, strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Add proof task and proof check protocols with exact content identities.
2. **Complete.** Make proof writes require Lean verification and return the checked receipt.
3. **Complete.** Mirror the protocol in Python and formalize proof admission.
4. **Complete.** Cover failed and corrected attempts, checked proof writes, regeneration, undo,
   redo and stale refusal in the end-to-end workflow. Agent guidance and continuity are current;
   the complete default and WASM repository gates pass locally.

### PR 27. Agent-Authored Formal Properties

Status: merged as [PR 292](https://github.com/e6qu/fun-refactor/pull/292).

Goal: let an agent state a project-specific property over a generated Lean model without reading or
editing the generated module. The agent authors both the proposition IR and every proof tactic.
`fr` supplies a revision-bound task, validates the typed proposition, renders a theorem scaffold and
connects it to the existing checked proof workflow.

Deliverables:

- Disclose a bounded, content-addressed property task containing the model signature, admitted
  proposition grammar, empty source-free templates and exact next actions.
- Accept a strict `fr-formal-property-1` tree authored by the agent. Bind it to the current task,
  validate identifiers, variables, model arguments, types, node/depth ceilings and theorem-name
  collisions before creating a formal plan.
- Render only the validated theorem statement and an empty proof region. Use the initialized pinned
  Lean package to elaborate the complete generated module before any scaffold write.
- Preserve agent-authored property trees across deterministic plan regeneration. Make source or
  property-task drift refuse before history creation.
- Mirror constructors, validation and Merkle identity in the zero-dependency Python SDK.
- Model property admission and the abstract term/proposition type rules in Lean, then compare the
  shared finite cases with Rust.
- Exercise a multi-input property through task creation, plan creation, scaffolding, failed and
  corrected proof attempts, apply, verify, undo, redo and stale-source refusal.

Verification and acceptance:

1. The property task contains no source body, completed theorem or proof tactics.
2. The plan contains exactly the agent-authored proposition tree and its deterministically rendered
   Lean theorem. Changed task identity, names, types, variables, arity or tree limits refuse.
3. Scaffold preview and write require successful Lean elaboration of the exact generated module.
4. The proof companion still accepts only agent-written tactics and changes one named region.
5. Python independently recomputes task and plan identities. Lean proves the finite admission and
   abstract typing policies; Rust agrees for every shared input.
6. Native, Python, documentation, strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Define the property task and typed agent property protocol.
2. **Complete.** Integrate custom properties into plans, scaffolding and Lean elaboration.
3. **Complete.** Add the Python mirror and Lean admission/type models.
4. **Complete.** Cover the full lifecycle and refusal matrix. The agent guidance and continuity
   are current, and the complete default and WASM repository gates pass locally.

### PR 28. Cross-Stack Agent Coverage

Status: merged as [PR 293](https://github.com/e6qu/fun-refactor/pull/293).

Goal: give agents one bounded high-level project model for the requested web and service stack.
It covers JavaScript, TypeScript, React, Next.js, Go, Python, FastAPI, HTML, CSS, Tailwind CSS,
Express.js, Markdown and Mermaid diagrams embedded in Markdown. Each surface must have an explicit
tested identity instead of hiding behind a parser language or an unrelated framework.

Deliverables:

- Add a revision-bound technology inventory that distinguishes language, dialect, framework,
  styling and embedded-diagram surfaces. Report bounded source-free evidence, counts, omissions and
  exact follow-up actions for every requested surface.
- Extend application and feature hierarchies to standalone React and Express.js projects while
  retaining the existing Next.js and FastAPI models. Preserve route, component, package,
  configuration, lifecycle and service-call gaps instead of inferring runtime behavior.
- Add a high-level style model for CSS and Tailwind utility use across HTML, JSX and TSX. Connect
  definitions, literal uses, configuration evidence and unresolved dynamic class construction.
- Parse fenced Mermaid blocks inside Markdown into bounded diagram, node and edge structures while
  preserving the Markdown document hierarchy and exact source locations.
- Carry technology, application, style and diagram objects through content-addressed progressive
  disclosure. Provide narrow next actions and enforce per-response byte and row ceilings.
- Expand source-free semantic authoring for JavaScript and Python functions. Route React, Next.js,
  Express.js and FastAPI handlers through their underlying typed function models. Add checked
  structural edits for HTML, CSS/Tailwind and Markdown/Mermaid nodes where their syntax gives an
  exact boundary.
- Model inventory admission, evidence limits, style-token locality and Mermaid graph bounds in Lean.
  Compare finite policies and generated cases with Rust, and keep parser correctness as an explicit
  tested boundary.
- Exercise one generic polyglot fixture containing every requested surface. Check map, trace,
  progressive reveal, safe edits, syntax, undo, redo and Git patch identity without fixture-specific
  framework rules.

Verification and acceptance:

1. Every requested surface has its own stable public identifier and source-free evidence. No row
   aliases JavaScript to TypeScript, React to TSX or a framework to its host language.
2. Counts and omissions cover the selected revision exactly. Changed source, manifests, selection,
   profiles or cursors refuse as stale.
3. Express.js, Next.js and FastAPI routes join application features with their exact handlers.
   React components remain usable with or without Next.js.
4. CSS and Tailwind class relationships retain literal evidence and label computed forms as gaps.
   Mermaid nodes and edges remain nested under the Markdown fence that declared them.
5. Progressive objects reconstruct from their content addresses. Narrow reveals never require an
   agent to ingest the complete project or source file.
6. Every write parses in its host grammar, changes only its selected structure and retains checked
   history, undo, redo and Git patch delivery.
7. Lean proves the bounded policies. Native, Python, documentation, portable-skill, capability,
   strict Lean and WASM gates pass.

Planned checkpoints:

1. **Complete.** Add the technology taxonomy, inventory, evidence bounds and polyglot fixture.
2. **Complete.** Extend Express.js and standalone React application/feature models. Express routes
   group under their nearest captured npm package. React packages outside Next.js form entry-rooted
   component features with bounded relative-import expansion. A source-anchored Lean predicate
   excludes Next.js packages and incomplete parser evidence from standalone admission.
3. **Complete.** Add CSS/Tailwind and Markdown/Mermaid high-level relationship models. CSS names
   link to literal HTML/JSX uses; Tailwind candidates require package evidence and computed classes
   remain gaps. Markdown headings own Mermaid fences, whose bounded nodes and edges retain exact
   source handles without labels or messages.
4. **Complete.** Join every new object type to progressive disclosure and the Python SDK. The
   project view commits technology, application, style and document/diagram domains beneath one
   Merkle root, exposes bounded exact actions and reuses the schema-independent Python object pack.
5. **Complete.** Expand checked semantic authoring across the requested host structures. Python
   functions, methods and decorated async FastAPI handlers now use relative-suite parsing, exact
   body splices, typed semantic edits, progressive capabilities, history and Git patches. Opaque
   surface capabilities add exact CSS definition, literal class token, Markdown heading and
   diagram-scoped Mermaid node edits with the same reversible lifecycle.
6. **Complete.** Add Lean policies, exhaustive correspondence and the complete repository gates.
   Executable anchors cover inventory evidence partitioning, standalone React admission, style
   resolution, generic surface omission bounds and surface-value size limits. Surface-edit admission
   requires a well-formed capability, one current candidate, a valid changed value and no collision.
   Rust and Lean agree across the generated finite domains. Native, portable-skill, strict proof and
   WASM gates pass with zero proof obligations or debts.

## Formal verification policy

Prioritize properties whose failure silently changes code or misleads an agent.
Extend edit and position kernels for new authoring scopes, transaction laws for Git lifecycle work and executable semantics for the first framework subset.

Every proof record must name:

- The property, domain and assumptions.
- The Lean declaration and checking toolchain.
- Source anchors and signature correspondence where available.
- The evidence connecting the model to the implementation.
- Remaining obligations and trusted components.

Shared corpora test correspondence only on their generated cases.
A general implementation claim requires a correspondence proof or a justified verified-generation path.
Translation to Lean alone establishes no source-level property.
Parser, compiler, runtime, Git and filesystem assumptions remain explicit.

Strict verification must reject stale source anchors, changed signatures, unbuilt Lean targets and proof obligations hidden by placeholders.
Proof-debt counts act as ratchets rather than claims of completeness.

## Validation

Use focused regressions during implementation and `tools/check.sh` for the complete native/WASM gate.
The playground CI job builds WASM, typechecks the UI and exercises exported behavior.
`tools/check.sh deep` runs repository-scale audits after merge and on demand.
Every PR preserves compile checks, refusal evidence, capability coverage and Lean kernel checks.

The end-to-end product scenario uses an unfamiliar project:

1. Inspect a compact map and find the relevant feature.
2. Retrieve its contracts, relationships and necessary source.
3. Plan and validate a structural change.
4. Export and apply a patch, then undo and redo it.
5. Introduce one Lean property and detect later drift.
6. Migrate one supported framework feature with explicit remaining work.

Evaluations record context, success, refusals, manual corrections, latency and verification coverage.
They use pinned real repositories alongside adversarial fixtures.
A capability predicate or clean syntax tree cannot complete a milestone without behavioral evidence.

Routine autonomous trials use the weakest economical model exposed by the installed Codex CLI at its lowest supported reasoning effort.
The current baseline is `gpt-5.6-luna` at `low`, with the default service tier.
Each cohort records the CLI version, visible model catalog entry, model, effort, service tier and authentication mode.
Availability and quota policy can change.
The harness retains every attempted trial and keeps infrastructure failures separate from agent failures.

## Known limits and deferred choices

- Complete dependency resolution, feature evaluation and package-manager semantics remain outside the current Cargo/npm subset.
- Framework readers recognize selected static patterns; whole-application dependency and runtime behavior remains pending.
- Native history has explicit, basis-bound replay-payload retention. Browser history remains
  session-only and records UTF-8 regular-file snapshots with mode `0644`; it does not provide crash recovery or persistence.
- Strict signature maps cover all readable code languages. Non-Rust maps use canonical shared-IR
  types; markup, stylesheets, configuration and Markdown have no function signature surface.
- Existing model proofs and executable comparisons do not establish general Rust implementation correspondence.
- LSP delegation remains excluded from the default engine. Reconsider it only for measured tasks that need it.
- Daemon/watch mode remains deferred until cache and repeated-query measurements justify it.
- Feature migration currently covers one route source file across Next.js App Router and FastAPI. Generated Next.js validation covers one direct body model built from primitive, optional, list, string-keyed map, tuple and acyclic local-record shapes. Explicit FastAPI registration checks one application file and its direct routes. Explicit cutover checks resolved external source references and keeps deletion reversible. Reviewed work includes framework coercion, aliases, custom validators, field constraints, strict and extra-field settings, composed-router conflicts, runtime imports and external callers. Projects provide the commands and assertions selected for transaction evidence.

## Further reading

- [CLI.md](CLI.md): implemented commands and write guarantees.
- [RECIPES.md](RECIPES.md): selection, operations and expectations.
- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md): current references and translation boundaries.
- [API_CONTRACTS.md](API_CONTRACTS.md): route conversion and HTTP contracts.
- [IR.md](IR.md): the existing code representation.
- [docs/lean-specs.md](docs/lean-specs.md): implemented checks and the adoption workflow.
- [docs/continuity.md](docs/continuity.md): completed milestone detail and the current implementation handoff.
