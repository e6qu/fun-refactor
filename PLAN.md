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
| Defects fixed | 682 |
| Defects open | 1 |

| Milestone | Status | Delivered foundation | Remaining outcome |
|---|---|---|---|
| M0 safe writes | Complete | Recoverable multi-file commits and structured failure evidence | Maintained as a shared write invariant |
| M1 undo and redo | Complete | Persistent transactions, recovery, conflict checks, apply, undo and redo | Retention and large-journal work continues in Git lifecycle work |
| M2 compact project understanding | In progress | Bounded maps, symbols, packages, dependencies, calls, routes, contracts, schemas, tests and Cargo workspace evidence | Lower repeated-query/context cost and broaden dependency and contract evidence |
| M3 Git integration | Complete | Patches, repository views, staging history, reviewed commits and durable owned-worktree lifecycle | Maintained as a shared repository invariant |
| M4 agent workflow | In progress | Portable skill, bounded authoring, multi-file batches, checks and eighteen passing autonomous trials | Make context use competitive and extend authoring scope |
| M5 Lean adoption | Complete | Pinned package initialization, anchored model scaffolds, regeneration, proof-debt ratchets, generated CI and bounded evidence reports | Maintain the trust boundary and extend the documented model subset as real projects require it |
| M6 framework transformation | In progress | Shared code IR, bounded Next.js/FastAPI/OpenAPI support and route-centered feature hierarchies | Complete framework semantics and verified migrations |

## What exists now

- Syntax trees, symbols, scopes, confidence tiers, byte edits and a content cache.
- Navigation, implementations, usages, call graphs, flow, impact and entry points.
- Rename, extract, inline, move, signature changes, imports, deletion and structural rewrites.
- Cross-language references, configuration provenance and configuration-to-code traces.
- A shared translation IR with Rust, Go, Java, Python, TypeScript, Zig, Bash and Lean readers and writers.
- Bounded project views for Cargo/npm packages, local dependencies, Cargo ownership, calls, tests, routes, request/response contracts and selected schemas.
- Next.js/FastAPI route conversion and OpenAPI service scaffolds within documented subsets.
- Local recipes, expectations, workspace previews and canonical formatting.
- Rust, Go, Java, TypeScript and TSX body authoring, Rust declaration replacement and Rust function insertion into files, inline modules, impls and traits.
- Multi-file authoring batches using one reviewed source-history transaction.
- Declared checks with reviewed configuration digests, bounded output and compact successful reports.
- Persistent native source history with checked apply, undo, redo, recovery and Git patch export.
- Bounded Git status, diff, changed-declaration and call-context views.
- Unix staging previews and writes, durable staging undo/redo, reviewed commits and owned worktree creation, recovery, removal, resumption and archive compaction.
- Native releases, a WASM API, a browser playground and patch downloads.
- Lean models for edits, positions, history, patch properties, pagination, confidence, workspace membership, revision buffers and declaration insertion placement.
- Source anchors, signature maps and shared Rust/Lean executable cases.
- External-project Lean initialization, anchored Rust model scaffolds, proof-preserving regeneration, named debt ceilings, generated CI and bounded verification evidence.
- Eighteen passing autonomous trials across pinned strsim and regex snapshots, with replayable patches and independent behavioral oracles.

## Evidence baseline

The latest structural-authoring evaluation changes two crates in the complete pinned regex workspace.
Both fresh Luna-low agents pass project checks, independent 1,060-input project and receiver oracles, exact reversal and index-preservation checks.
The `fr` arm uses 18,806 measured context tokens and 29 calls; ordinary files use 15,192 tokens and 20 calls.
This single pair shows a 23.8% `fr` context premium and makes no context-parity claim.

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

The foundation landed as PR 0. The remaining roadmap is packaged as six reviewable product PRs.
Each PR can contain internal checkpoint commits, but its description and acceptance evidence must describe one final outcome.

### PR 0. Agent-ready verified refactoring foundation

Status: merged as [PR 259](https://github.com/e6qu/fun-refactor/pull/259).

Goal: establish the shared product and evidence base required by the remaining roadmap.

Delivered:

- Recoverable source transactions with checked apply, undo, redo, recovery and Git patch export.
- Bounded project maps and targeted evidence for packages, dependencies, symbols, calls, routes, contracts, schemas and tests.
- Bounded Git status, diff, changed-declaration, staging, commit and owned-worktree workflows.
- Structural authoring for selected Rust, Go, TypeScript and TSX declarations, including reviewed multi-file batches.
- A portable `fr` agent skill with executable workflows for exploration, authoring, checks, history, Git and Lean specifications.
- Lean models for source positions, edits, history, patches, pagination, project membership, revision buffers and insertion placement.
- Source anchors, signature maps, strict specification checks and shared executable Rust/Lean cases.
- Sixteen retained autonomous trials, independent behavioral oracles, exact patch replay and token audits.
- Native and WASM validation with measured capability coverage for every supported language-operation pair.

Acceptance evidence:

- The complete native/WASM gate passes with 311 supported capability-language pairs exercised.
- Strict Lean verification rejects stale anchors, signature drift, unbuilt targets and unresolved proof obligations.
- Agent evaluation records preserve prompts, events, patches, source states, checks, independent oracles and exact reversal evidence.
- Documentation states the supported subsets, refusal boundaries, proof limits and current context-cost findings.

PR 0 supplies the baseline that every later PR must preserve or improve.

### PR 1. Agent Context Protocol v2

Status: merged as [PR 261](https://github.com/e6qu/fun-refactor/pull/261).
The projection saves 2,049.5 mean fr tokens and leaves a 4,419-token normalized gap. Fresh low-effort attempts did not produce a complete passing post-fix pair, so this PR makes no context-parity claim.

Goal: make the structured `fr` workflow competitive on retrieved context while retaining its stronger evidence and reversal guarantees.

Deliverables:

- Measure repeated project, authoring and history metadata against the retained M4ab transcripts.
- Add revision-bound compact reports only where an earlier reviewed basis reconstructs the omitted fields exactly.
- Reduce repeated coverage, source-revision, handle and transaction serialization.
- Shrink the portable skill entrypoint and route agents to specialized references only when the task requires them.
- Preserve bounded results, uncertainty, source verification, check declarations and reviewable diffs.
- Keep fixed projections separate from autonomous outcomes and preserve all existing scores.
- Freeze one shared check-output policy and an explicit cache policy before the next paired cohort.
- Add an opt-in local `codex exec` runner with fresh ephemeral sessions, ignored user configuration and retained JSONL events.
- Use `gpt-5.6-luna` at `low` effort and the default service tier for the initial local baseline.
- Evaluate a larger task that requires coordinated changes and broader exploration.

Verification and acceptance:

- Reconstruction tests recover the full report from its compact form and reviewed basis.
- Missing, stale, truncated and conflicting bases refuse without writes.
- Fixed-transcript tests prove that projections change only declared payload fields.
- The existing sixteen trials retain exact token audits and behavioral replay.
- Fresh paired agents receive identical check-output rules, inherited model settings and independent correctness oracles.
- Normal CI replays retained evidence without spending agent quota; authenticated real-agent runs remain explicit local or scheduled jobs.
- A milestone cohort includes a small Terra or Sol calibration only when Luna failures could hide whether the workflow itself works.
- The report separates skill, inspection, checks, authoring, delivery, request and latency costs.
- Context improvement counts only when task success and evidence coverage remain intact.

Measured outcome:

- Revision-bound project and transaction reports reconstruct exactly, and malformed or stale bases refuse before writes.
- The fixed passing-cohort projection reduces mean fr context from 13,278.5 to 11,229 tokens, a 15.4% saving, while ordinary files remain at 6,810.
- The retained Terra-low diagnostic pair records 15,979 fr tokens and 11,025 file tokens. The fr change passes both independent oracles but fails delivery sequencing; the file change passes its workflow but fails both behavioral oracles.
- Codex launch metadata and raw JSONL are checksum-bound in recorded evidence, and the manifest states cohort acceptance directly.

This PR closes the scoped v2 implementation and measurement work. Context parity remains open: a future claim requires a fresh passing pair after a material reduction in skill and authoring/delivery context. See the [v2 evaluation](docs/agent-context-v2-evaluation.md).

### PR 2. Generalized Structural Authoring

Status: ready for review on `generalized_structural_authoring`.

Goal: let an agent perform broader high-level changes without replacing entire files.

Deliverables:

- Insert declarations into Rust `impl` and trait bodies and corresponding bounded scopes in selected languages.
- Extend initializer and expression-body support beyond the current TypeScript/TSX forms.
- Add the next authoring languages from measured project demand.
- Compose declaration, caller and import changes as one inspectable multi-file transaction.
- Add explicit postconditions to high-level authoring recipes.
- Reuse handles, revision guards, edit planning, syntax validation, history and patch export.

Verification and acceptance:

- Generalize insertion-position and disjoint-splice models for the added scopes.
- Compare executable Lean and Rust models across boundary, Unicode and overlapping-edit cases.
- Compile and run representative changes with warnings denied in every added language.
- Preserve exact bytes, modes and unrelated edits across patch application, undo and redo.
- Complete an autonomous coordinated change using only bounded source and the portable skill.

Measured outcome:

- Rust insertion now targets files, inline modules, impls and traits; Java methods, constructors and default methods use the same checked body path.
- TypeScript and TSX arrows move between expression and block bodies while preserving surrounding source.
- One batch can combine declaration, caller and conservative import edits with exact postconditions.
- The selection-conflict predicate has six Lean theorems and 6,084 shared Rust, Lean and independent-oracle cases, including UTF-8 boundaries.
- Representative Java and Rust-scope histories preserve mode `0640` through apply, undo and redo on Unix.
- A fresh `gpt-5.6-luna` low-effort pair passes the coordinated two-crate task. The `fr` arm uses one three-operation saved transaction, exact undo/redo and a clean receiver.
- Both arms pass all 1,060 independent byte and allocation cases. Offline replay and token auditing pass for the retained evidence.

### PR 3. Durable Git Workspace Lifecycle

Status: merged as GitHub PR 263. Eleven checkpoint commits complete the deliverables below; [continuity](docs/continuity.md) retains their detailed evidence.

Goal: finish the repository workflow needed for long-running agent changes and recovery.

Deliverables:

- Extend transaction patches to the required regular, executable and symlink modes; complete.
- Improve recovery for registration failures that Git does not retain; exact registered pre-receipt recovery is complete.
- Expose stale-lock and uncertain crash states with actionable inspection evidence; complete.
- Add staging-journal retention and checked compaction; complete.
- Add bulk retention for completed worktree-removal archives; complete.
- Extend selected index-flag replay where Git can preserve it safely; complete.
- Undo an `fr` source transaction without disturbing unrelated working-tree or index changes; complete.
- Keep Git optional for ordinary analysis and source history; complete.

Verification and acceptance:

- Exercise dirty files, staged entries, untracked content, linked worktrees, object pruning and injected failures.
- Prove abstract retention, compaction, transition and unselected-state preservation laws.
- Compare the guarded Rust predicates with Lean across complete bounded state domains.
- Keep filesystem durability, Git locking and implementation correspondence explicit where proofs do not cover them.

Measured outcome:

- Staging history has explicit retention, checked compaction, crash inspection and replay of supported index flags.
- Owned worktrees preserve repository-local configuration and raw symlink entries through creation, recovery, removal and archive resumption.
- A durable preparation recovers exact post-registration crashes before ownership-receipt publication, including after object pruning.
- Source transactions and Git patches preserve regular, executable and symlink kinds across apply, undo, redo and recovery.
- Source-history reversal leaves affected staging and unrelated Git state intact, while ordinary source workflows remain usable without Git.
- Thirty-five new Lean theorems cover selection, compaction, crash classification, entry policies, recovery evidence, snapshot modes and namespace preservation.
- Host tests cover linked and SHA-256 repositories, dirty and untracked files, staged entries, injected failures and byte-identical index preservation.

### PR 4. Lean Adoption Kit

Status: ready for review on `lean_adoption_kit`. Six product checkpoints cover initialization, scaffolding, regeneration, debt, CI and evidence. The Lake integration proves a useful property, refreshes its changed source identity, observes a passing build, then breaks the theorem and observes failure. Four Lean theorems cover the proof-debt ceiling, with 4,225 shared Rust and Lean cases. The complete native, Lean, prose, capability and WASM gate passes.

Goal: let an external repository adopt and maintain one useful verified property through `fr`.

Deliverables:

- Initialize a bounded specification package and its checked build targets; complete.
- Select Rust source declarations and generate anchored Lean model scaffolds and signature maps; complete for a documented type subset.
- Mark generated and handwritten regions and preserve handwritten proofs during regeneration; complete for generated scaffolds.
- Detect source drift and guide synchronization or repair; strict checks and scaffold refresh complete.
- Introduce named proof-debt records and ratchets; complete with strict names and explicit debt ceilings.
- Generate CI configuration for the selected Lean checks; complete for GitHub Actions.
- Report assumptions, axioms, trusted components, covered properties and remaining obligations; complete with an explicit syntactic-axiom-analysis boundary.

Verification and acceptance:

1. Initialize verification in a pinned external Rust repository.
2. Select and prove one useful property.
3. Change its source declaration and observe a drift failure.
4. Repair the source/model relationship through `fr`.
5. Preserve handwritten regions through regeneration.
6. Break the property and observe the checked build fail.

Every result distinguishes a proved model property, tested implementation/model correspondence and proved implementation correspondence.

### PR 5. Framework Semantic Model

Status: in progress on `framework_semantic_model`. Four committed checkpoints provide the hierarchy, package model and backend execution boundaries. The fifth checkpoint adds frontend-only Next.js page features and direct React component facts across server/client placement, props, state, effects, events, style shapes and render edges. Every fact retains its parent, source anchor, evidence, confidence and explicit gaps.

Goal: represent a feature across application structure rather than as isolated syntax nodes.

Deliverables:

- Model applications, packages, features, build settings and dependency boundaries; complete for npm-backed Next.js applications, with Python packaging still explicit as a gap.
- Model backend routes, schemas and handlers; complete for the first bounded Next.js/FastAPI subset.
- Model middleware and authentication boundaries; readers cover direct convention files plus application, route and parameter dependencies. Unsupported forms remain gaps.
- Model lifecycle and runtime configuration boundaries; implemented for direct FastAPI and Next.js declarations with runtime validation still open.
- Model service dependencies; readers recognize direct handler HTTP candidates. Client construction, non-HTTP services and runtime reachability remain open.
- Model frontend components, properties, events, state, effects, styles and rendering boundaries; complete for direct components in Next.js page files.
- Attach source anchors, confidence, unsupported constructs and validation evidence to every fact; complete for the first route-centered hierarchy.
- Strengthen the existing Next.js and FastAPI readers before adding another backend pair.
- Use Next.js React Server and Client Components as the first rendering-boundary pair; complete for the direct page component subset.
- Keep framework-specific behavior visible where the shared model cannot express it.

PR 5 completes each boundary against a named, versioned syntax subset. Executable fixtures validate runtime claims that source structure cannot establish.

Verification and acceptance:

- Display one feature hierarchy across routes, handlers, schemas, components and dependencies; complete for exact matching Next.js page and API paths.
- Retrieve that subtree without loading the whole application.
- Preserve ambiguity and unsupported middleware, authentication, lifecycle and runtime behavior as explicit gaps; complete for current dependency and middleware readers.
- Compare readers against pinned framework fixtures and real projects with independent contract checks.

### PR 6. Verified Feature Migration

Goal: migrate one real feature through an inspectable, reversible high-level transformation.

Deliverables:

- Move or translate a bounded feature using the framework semantic model.
- Update connected routes, handlers, schemas, callers, components, build settings and tests.
- Separate automatic steps, agent decisions and unsupported behavior in the migration plan.
- Allow source and destination frameworks to coexist during an incremental migration.
- Produce one reviewed source-history transaction and one Git patch.
- Preserve mixed-framework operation until the feature cutover completes.

Verification and acceptance:

- Compile and execute the source and migrated feature in pinned projects.
- Compare route and schema contracts with independent behavioral fixtures.
- Check middleware order, authentication, validation and lifecycle behavior wherever the migration claims preservation.
- Keep every unsupported construct visible in the final plan.
- Apply the patch to a clean receiver and reproduce the result through exact undo and redo.

## Delivery order

PR 0 supplies the verified refactoring, repository and evaluation foundation.
PR 1 establishes the response and evaluation protocol used by every later agent workflow.
PR 2 and PR 3 can then proceed independently.
PR 4 depends on the stable authoring workflow but does not require the framework model.
PR 5 depends on compact project evidence and supplies the semantic input for PR 6.
PR 6 depends on PR 2, PR 3 and PR 5.

```text
PR 0  Agent-ready verified refactoring foundation
  └── PR 1  Agent Context Protocol v2
  ├── PR 2  Generalized Structural Authoring ──┬── PR 4  Lean Adoption Kit
  │                                            └──┐
  ├── PR 3  Durable Git Workspace Lifecycle ─────┼── PR 6  Verified Feature Migration
  └── PR 5  Framework Semantic Model ────────────┘
```

PR 4 may run alongside framework work after PR 2 provides the required external-project authoring path.

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
- The first frontend migration pair remains open until PR 5 defines its bounded subset.

## Further reading

- [CLI.md](CLI.md): implemented commands and write guarantees.
- [RECIPES.md](RECIPES.md): selection, operations and expectations.
- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md): current references and translation boundaries.
- [API_CONTRACTS.md](API_CONTRACTS.md): route conversion and HTTP contracts.
- [IR.md](IR.md): the existing code representation.
- [docs/lean-specs.md](docs/lean-specs.md): implemented checks and the adoption workflow.
- [docs/continuity.md](docs/continuity.md): completed milestone detail and the current implementation handoff.
