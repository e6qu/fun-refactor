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
| Defects fixed | 691 |
| Defects open | 1 |

| Milestone | Status | Delivered foundation | Remaining outcome |
|---|---|---|---|
| M0 safe writes | Complete | Recoverable multi-file commits and structured failure evidence | Maintained as a shared write invariant |
| M1 undo and redo | Complete | Persistent transactions, recovery, conflict checks, apply, undo and redo | Retention and large-journal work continues in Git lifecycle work |
| M2 compact project understanding | In progress | Bounded maps, symbols, packages, dependencies, calls, routes, contracts, schemas, tests and Cargo workspace evidence | Lower repeated-query/context cost and broaden dependency and contract evidence |
| M3 Git integration | Complete | Patches, repository views, staging history, reviewed commits and durable owned-worktree lifecycle | Maintained as a shared repository invariant |
| M4 agent workflow | In progress | Portable skill, bounded authoring, multi-file batches, checks and twenty-two passing autonomous trials | Make context use competitive and extend authoring scope |
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
- Rust, Go, Java, TypeScript and TSX body authoring, Rust declaration replacement and Rust function insertion into files, inline modules, impls and traits.
- Multi-file authoring batches using one reviewed source-history transaction.
- Declared checks with reviewed configuration digests, bounded output and compact successful reports.
- Persistent native source history with checked apply, undo, redo, recovery and Git patch export.
- Bounded Git status, diff, changed-declaration and call-context views.
- Unix staging previews and writes, durable staging undo/redo, reviewed commits and owned worktree creation, recovery, removal, resumption and archive compaction.
- Native releases, a WASM API, a browser playground and patch downloads.
- Lean models for edits, positions, history, patch properties, pagination, confidence, workspace membership, revision buffers, declaration insertion placement and framework reporting policies.
- Source anchors, signature maps and shared Rust/Lean executable cases.
- External-project Lean initialization, anchored Rust model scaffolds, proof-preserving regeneration, named debt ceilings, generated CI and bounded verification evidence.
- Twenty-two passing autonomous trials across pinned strsim and regex snapshots, with replayable patches and independent behavioral oracles.

## Evidence baseline

The latest PR 7 evaluation changes two crates in the complete pinned regex workspace.
Both fresh Luna-low agents pass project checks, independent 1,060-input project and receiver oracles,
ordered original/final checks, exact reversal and index-preservation checks.
The `fr` arm uses 15,458 measured context tokens and 42 calls; ordinary files use 11,600 tokens and 20 calls.
This single pair shows a 33.3% `fr` context premium and makes no context-parity claim.
The `fr` agent uses the reviewed-plan and compact transaction workflow.
Seven refused or failed calls and repeated inspection identify discoverability and call count as the next practical bottlenecks.
The PR 8 prescribed-workflow projection exercises current handle selection against the pinned
workspace and retains every mutation and verification step. It reduces that trace to 29 calls
and 13,278 context tokens, 2,180 below the observed `fr` arm and 1,678 above the ordinary-file
arm. This one-trace counterfactual is separate from the retained fresh adoption pair below.

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

This projection explains the original aggregate result but does not predict how agents adapt to shared instructions.
It makes repeated skill, inspection and transaction output the immediate optimization target.
The [coordinated evaluation](docs/agent-coordinated-evaluation.md) contains the protocol, retained evidence and limits.

## Product contract

An agent should move from a project map to a module, symbol contract, relationships and selected implementation.
It should request full bodies only when needed.
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

The first packaged roadmap is complete. Nine merged pull requests established the product foundation and its first measured workflow reduction:

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

Git history and [development continuity](docs/continuity.md) retain checkpoint-level detail.

### PR 8. Agent Workflow Simplification

Status: merged as [PR 269](https://github.com/e6qu/fun-refactor/pull/269).

Goal: remove avoidable discovery and authoring calls exposed by the fresh PR 7 trace while retaining bounded evidence, independent validation, exact patch replay and undo/redo.

Measured baseline:

- The current checksum-bound projection uses 11,092 mean `fr` context tokens and 6,810 ordinary-file tokens. Its fixed action sequence has a 4,282-token or 62.9% `fr` premium.
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

Status: in progress on `project_context_v5`.

Goal: let an agent obtain several heterogeneous high-level project views from one immutable
snapshot without repeating project construction, revision coverage or process calls.

Deliverables:

- Run existing read-only `fr project` queries from one versioned manifest and one verified snapshot.
- Emit revision, handle prefix and coverage once, while retaining a digest for the complete manifest
  and each resolved request.
- Enforce request, argument and serialized-report budgets; omit only complete reports and state the
  exact omission instead of clipping JSON or facts.
- Allow later requests to consume bounded string results from earlier requests without copying
  revision-bound handles through an agent round trip.
- Teach the portable skill an exact broad-exploration route and retain executable examples.
- Compare separate invocations with the equivalent batch under fixed inputs, payload audits and
  cold/warm cache controls before making a context or latency claim.
- Run a fresh Luna-low paired task only after deterministic evidence shows that the batch targets
  calls a broader exploration task actually needs.

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
2. Add backward result references so one batch can discover and then inspect exact handles.
3. Add portable skill guidance, executable examples and complete CLI/formal documentation.
4. Retain a controlled separate-versus-batch measurement with immutable payload and binary digests.
5. Exercise the route in a fresh broad-exploration pair if the controlled evidence justifies quota use.

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
- Native history needs retention and large-journal scaling. Browser undo still restores a loaded workspace rather than individual transactions.
- Shared browser patch and transaction semantics remain pending.
- Strict signature maps currently accept Rust source declarations only.
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
