# fun-refactor roadmap

`fr` is a deterministic tool for agents to understand and change projects through bounded,
high-level data. An agent should consume semantic structure, relationships, identities and exact
actions before it requests source text. Every write should remain reviewable, reversible and tied
to the evidence used to plan it.

This file is the active completion plan. It does not repeat finished pull-request narratives.
[Development continuity](docs/continuity.md), [CHANGELOG.md](CHANGELOG.md), [BUGS.md](BUGS.md) and
Git history retain that detail. [CLI.md](CLI.md) documents commands that exist today. Names in the
remaining-work sections are proposals until their pull requests merge.

## Finish line

The implementation is complete when an agent can:

1. Express a bounded structured goal without knowing the command tree, recipe grammar, semantic IR
   or proof protocol in advance.
2. Receive one language-aware sequence of exact actions. Each action states why it applies, what the
   agent must supply, what schema it returns, which evidence it establishes and what remains unknown.
3. Inspect only the relevant Merkle-addressed project and semantic subtrees. Source remains behind an
   explicit reveal when higher-level evidence is insufficient.
4. Choose or accept the smallest safe route among a direct refactoring, recipe, semantic edit,
   framework migration or formal proof task.
5. Preview a complete change, retain one review identity and execute only that unchanged review.
   Run declared checks, export a Git patch, and undo or redo the transaction.
6. Use Lean support without confusing model theorems, implementation correspondence, behavioral
   tests, compiler checks and syntax checks.
7. Obtain honest support and refusal reports for every advertised language, framework and operation.
8. Complete representative unfamiliar-project tasks with no exploratory help calls after the first
   guide response and no known actionable correctness defect.

Completion does not mean guessing runtime behavior from static source, accepting an unsupported
construct, or claiming general source equivalence from a translated Lean model.

## Current state

The project has merged PRs 0 through 39. [GitHub PR 311](https://github.com/e6qu/fun-refactor/pull/311)
adds the bounded application hierarchy, Merkle disclosure, agent-authored HTTP IR and four backend
adapters. It also adds static React and Next.js conversion. Current `origin/main` is `37b35f0b`.
PR 40 is in progress on `completion_audit_agent_validation` from that exact main.

| Measure | Current value |
|---|---:|
| Parsed languages | 19 |
| Query sets | 17 |
| Entry-point catalogs | 10 |
| Capability and language cells | 456 |
| Supported cells | 311 |
| Fixed defects | 766 |
| Open defects | 0 |

`fr capabilities` is the authority for operation support. Each unsupported or inapplicable cell
carries a reason. A supported cell can still refuse a particular input when its syntax, identity,
confidence or requested effect falls outside the checked contract.

### Delivered foundation

- Syntax trees, symbols, scopes, types, references, implementations, calls, flow, impact, entry
  points, configuration provenance and source confidence.
- Bounded project models for packages, dependencies, applications, features, routes, contracts,
  schemas, tests, styles, Markdown documents and Mermaid diagrams.
- Rename, delete, move, extract, inline, signature, import, rewrite, restructure and feature-flag
  operations with reparse checks and byte preservation outside selected edits.
- A shared semantic IR, checked body authoring, typed deltas, scalar intents, opaque disclosed edits
  and multi-file authoring batches.
- A recipe DSL with live machine-readable vocabulary, selectors, expectations, explanation and
  canonical formatting.
- Next.js and FastAPI route conversion, revision-bound feature migration and OpenAPI scaffolding in
  their documented subsets.
- Content-addressed progressive disclosure for semantic IR, code maps, traces, impact, sources and
  sinks. Each continuation binds the revision, target, view, profile and byte ceiling.
- Language-aware structured goal navigation and declarative `understand`, `trace`, `change`,
  `migrate` and `prove` intents over one immutable snapshot, with unified reviewed operations.
- A zero-dependency Python SDK and runtime that retain complete reports locally, verify canonical
  identities and optionally persist Merkle objects outside the analyzed project.
- Recoverable source history, apply, undo, redo, compaction, Git patch export, bounded repository
  views, staging history, reviewed commits and owned-worktree lifecycle.
- Declared checks with reviewed configuration digests, original-state gates, bounded diagnostics and
  compact successful evidence.
- Lean kernels for critical admission and transition policies, strict Rust-to-Lean source anchors,
  signature maps, proof-debt ratchets and exhaustive shared Rust/Lean cases.
- Bounded executable pure IR and Python constructors, typed formalization across eight readers,
  and retained structural/provenance snapshots for ten declarative language classes.
- An external-project formalization workbench with source-free property plans and bounded goal
  disclosure. It provides proof-preserving scaffolds, agent-written tactics, checked proof writes,
  generated CI and verification evidence.
- Native, WASM and browser interfaces with shared checked history and patch behavior.

### Language and framework scope

The parser identities are Rust, Go, Zig, Java, TypeScript, TSX, Python, Bash, HTML, CSS, SCSS,
Sass, HCL, JSON, YAML, Helm, XML, Markdown and Lean. JavaScript uses the TypeScript grammar;
JSX uses TSX. React, Next.js, Express.js, FastAPI and Tailwind CSS are technology or framework
surfaces rather than parser-language aliases.

| Layer | Implemented scope | Current boundary |
|---|---|---|
| Structural analysis | All 19 parser identities | Dynamic and external behavior remains uncertainty |
| Capability operations | 311 checked cells | 145 cells refuse or are inapplicable with reasons |
| Semantic body authoring | Rust, Go, Java, Python, JavaScript, TypeScript and TSX | Other language writers use narrower structural operations |
| Shared executable translation IR | Rust, Go, Java, Python, TypeScript, Zig, Bash and Lean | Unsupported constructs carry explicit gaps |
| Framework features | React, Next.js, Express.js, FastAPI, CSS/Tailwind and Markdown/Mermaid project evidence | Checked feature migration is currently Next.js and FastAPI route focused |
| Generated formalization | Typed pure declarations from eight reader surfaces and structural snapshots from ten declarative classes | Numeric, dynamic and framework runtime semantics remain separate; source equivalence remains unproved |

### Current evidence

The structured runtime substantially reduces protocol text exposed to an agent. The retained fixed
runtime comparison reduces five direct JSON exchanges from 17,689 bytes to one 1,893-byte program
and result. The context workspace comparison reduces sixteen exposed exchanges from 59,825 bytes to
one 4,550-byte program and packet. Native intent compilation uses one process instead of 89
progressive subprocesses on its fixed trace fixture.

These deterministic measurements do not establish model-token or population behavior. The latest
retained live Codex comparison still shows an `fr` context premium. The `fr` route uses 17,711
measured context tokens and 42 calls; direct files use 11,182 tokens and 17 calls. PR 35 reduces an intent-bound change
from three processes to two, while its complete request and response traffic is 272 bytes larger.
The next milestones must improve route selection and exposed interaction count rather than infer a
saving from internal composition alone.

## Remaining gaps

1. **Formalization has an explicit semantic boundary.** The generated workflow now covers typed pure
   declarations and retained structural snapshots across the advertised language cells. Untyped,
   async, dynamic, effectful and unsupported numeric constructs receive exact refusals.
   Expanding those semantics requires separate reviewed models and correspondence evidence.
2. **Implementation correspondence remains limited.** Lean checks named Boolean IR/model relations
   and model properties. Source
   anchors and signature maps identify the modeled declaration, while tests cover selected cases.
   The parser, IR extraction and lowering are still trusted or integration-tested boundaries.
3. **Framework transformation has a deliberate semantic boundary.** The shared application IR
   normalizes and writes literal JSON/path behavior across four backend adapters.
   It also converts static intrinsic JSX between React and Next.js.
   Registration and guarded whole-file cutover are connected.
   Request bodies, query validation, middleware, authentication, service calls and dynamic rendering
   remain named unsupported features rather than inferred behavior.
4. **Support is uneven across languages.** Basic structure is broad; semantic authoring, call
   analysis, translation, framework migration and formalization have smaller support matrices.
5. **The live-agent context target is unmet.** The retained live comparison predates unified
   delivery. No live-agent result establishes the new routes' context benefit.
6. **Static source has a tested analysis boundary.** B5 now records indirect calls and dead-code
   conclusions that source cannot settle. The report preserves these cases as uncertainty.

## Active delivery sequence

Large reviewable pull requests group the remaining work. Each PR must update this file,
continuity, the portable skill, command documentation, defect records and source-bound evidence when
its public contract changes.

| PR | Outcome | Status |
|---|---|---|
| PR 39 | Generic Hierarchical Framework Transformation | [PR 311](https://github.com/e6qu/fun-refactor/pull/311) merged |
| PR 40 | Completion Audit and Agent Validation | In progress |

### PR 39. Generic Hierarchical Framework Transformation

Goal: translate supported application features through a common hierarchy rather than through
example-specific or pair-specific source templates.

Implemented foundation:

- `project application` preserves the complete bounded feature-fact hierarchy and exact identities.
  Reader gaps remain explicit; route/component semantics retain normalization boundaries.
- Application IR supports revision-bound Merkle reveal and existing object-store sessions.
- Agent-authored `fr-http-application-1` behavior has deterministic Next.js, FastAPI, Express and
  Go HTTP writers, matching Python constructors and checked new-file history delivery.
- Next.js, FastAPI, Express and Go standard HTTP readers conservatively normalize literal JSON and
  path-binding handlers through the shared semantic IR. Effectful handlers remain manual.
- A full `project application` report feeds `migrate application` directly after its object digest
  is rechecked; equivalent four-framework fixtures produce equal response IR.
- `migrate application --project` performs the same conversion within one project snapshot. The
  guide selects it for compatible portable backend targets.
- The tagged `application-migration` intent operation executes that selected common planner from
  the exact intent handle, with checks, delivery, patch output and guide-basis validation.
- FastAPI app registration and PEP 621 dependencies can join the generated files in one reversible
  transaction. Express app/router registration and npm dependencies use the same history path.
  Go generates an owning-package mount for an explicit ServeMux under a captured module. Captured
  Next.js App Router placement supplies checked integration evidence.
- React and Next.js normalize one bounded intrinsic JSX tree and write `App.tsx` or `page.tsx`.
  Props, hooks, events, styles, component calls and dynamic expressions remain manual.
- Application reports publish all 75 adapter-pair/feature cells with deterministic refusal reasons.
  Explicit cutover is reversible for one recognized whole-file route or static component after
  connected integration and external-reference checks; mixed application files refuse.
- Lean models cover admission, compatibility, JSON status safety, exact unique coverage and
  endpoint agreement. Parser/writer and pinned framework runtime tests remain separate.

PR 39 implementation and local acceptance are complete. Dynamic frontend behavior remains
explicit manual evidence. The bounded HTTP subset does not replace the richer Next.js/FastAPI
feature planner.

Deliverables:

- Define a versioned hierarchy that preserves package, module, route, handler, schema and dependency
  facts. Preserve component and configuration facts. Limit executable IR to admitted features.
- Derive the IR from the existing React, Next.js, Express.js, FastAPI and Go HTTP evidence. Preserve
  unknown runtime behavior and unsupported framework constructs as explicit dispositions.
- Replace direct pair logic with adapters that read and write the common hierarchy. Keep adapters
  independent of fixture names, endpoint names and domain examples.
- Support checked transformations among the advertised frontend and backend adapters when their
  feature subsets overlap. Never infer a pair from a shared host language alone.
- Plan registration, dependencies, imports and guarded cutover as one revision-bound
  multi-file transaction. Preserve unrelated files and existing user changes.
- Validate generated projects with pinned framework toolchains and generic runtime fixtures. Check
  route, method, path, schema and response behavior rather than only syntax.
- Publish capability rows per source adapter, target adapter and feature kind. Every unsupported pair
  must name the missing semantic feature.
- Make application IR progressively revealable and storable by Merkle object digest. Let the
  navigator select only the feature branch needed for the migration.
- Formalize adapter admission, feature compatibility, disposition completeness and endpoint
  agreement policies in Lean. Test parser, writer and runtime behavior separately.

Acceptance:

1. No transformation contains a pet-store or other fixture-specific rule.
2. Equivalent generic features produce equivalent application IR regardless of their source adapter.
3. Every input feature receives a migrated, preserved, external, manual or unsupported disposition.
4. Generated projects compile and pass generic runtime contracts under their pinned toolchains.
5. Preview, review basis, apply, undo, redo and Git patch identity cover the complete multi-file
   migration and optional cutover.
6. The navigator selects migration only for an advertised compatible pair and reports every gap.
7. Lean policy correspondence, independent fixture checks and all repository gates pass.

### PR 40. Completion Audit and Agent Validation

Goal: close remaining defects and prove that the finished guided workflow is usable, bounded and
honest across the advertised product surface.

Implemented on the current branch:

- `fr audit` publishes a bounded content-addressed summary with progressive capability, workflow,
  recipe, semantic, framework, proof and boundary sections.
- The report derives its 456 capability cells and 75 application cells from live predicates. It
  names executable acceptance targets without treating an unrun test as passing evidence.
- The report separates support, behavioral validation, model theorems, implementation
  correspondence and runtime framework evidence.
- A source-bound deterministic sweep exercises all seven required workflow families. Guidance
  removes fourteen exploratory calls and makes no token or quota claim.
- B5 now records its remaining runtime-only cases as a tested analysis boundary. No known
  actionable defect remains open.

Deliverables:

- Audit every capability predicate, language, technology surface, recipe verb, semantic schema,
  intent route, action variant, formalization cell and framework adapter from the implementing code.
- Run complete parser, round-trip, translation, migration, history, Git and proof sweeps. Fix every
  reproducible defect found. Do not add an untested exception or deferred issue.
- Resolve the actionable part of B5 with available type, hierarchy and callable-value evidence.
  Reclassify inherently dynamic cases as a tested analysis boundary only when no sound static
  conclusion is available.
- Remove stale schemas, compatibility branches, duplicated guidance, obsolete docs and unreferenced
  evaluation artifacts. Keep retained scientific evidence immutable and linked from continuity.
- Run deterministic guided-versus-manual comparisons for each workflow family. Require the guide to
  eliminate exploratory help, vocabulary and command-shape calls.
- Run fresh isolated Codex CLI tasks with the weakest economical available model at its lowest effort.
  Cover understanding, tracing, direct change, recipe, semantic edit, framework migration and proof.
- Retain prompts, events, reports, patches, source states, independent behavior oracles, exact
  reversal and context accounting for every attempt, including failures.
- Publish one compact support and trust report generated from the same predicates, schemas, anchors
  and evaluators used by the product.

Acceptance:

1. Every advertised route has a passing deterministic end-to-end fixture and a checked refusal
   matrix. No known actionable defect remains.
2. Fresh agents complete the representative workflow set through the guide without direct project
   file ingestion except an explicit source reveal recommended by the guide.
3. Every successful write passes original and final checks, exact undo and redo, patch verification
   and independent task-specific behavior oracles.
4. Context reports count all prompts, tool requests, responses, retained programs and final answers.
   They distinguish deterministic protocol bytes from tokenizer counts and billed quota.
5. Capability, proof and framework reports make no stronger claim than their evidence supports.
6. Default, WASM, playground, every pinned language toolchain, Python, strict Lean, deep audit,
   external replay, prose and documentation gates pass on the final review head.

## Product invariants

These constraints apply to every remaining milestone.

- All reads and writes stay inside the selected workspace unless a command explicitly names an
  external read-only toolchain or object store.
- A response reports coverage, omissions, confidence, source basis and uncertainty. Truncation never
  becomes absence.
- Source-free semantic and project evidence stays source-free. Exact source requires an explicit
  bounded action.
- A mutation has a complete preview. Its checked basis commits every input that can change the plan.
- Existing user changes outside selected snapshots survive apply, undo and redo.
- Syntax validation, compilation, behavioral checks, formal model proofs and implementation
  correspondence remain distinct evidence classes.
- Unsupported or ambiguous inputs refuse before history creation.
- Every failure preserves structured bounded diagnostics. Compact success may omit only fields whose
  identity the review basis already commits.
- Every content-addressed value has one canonical representation and independent verification tests.
- Python mirrors public IR shapes directly. The package root remains empty, with no `__main__.py` and
  no mutable `__all__` registry. Python tests use pytest and type-check with `ty`.
- Dependency upgrades use the newest stable release after it has been public for at least 24 hours.
  Real compiler or runtime fixtures must pass before merge.

## Formal verification policy

Formalize properties whose failure could silently change code, admit a stale action, corrupt history
or overstate evidence. Prefer small executable kernels with explicit assumptions over broad models
that lack implementation correspondence.

Every proof record names:

- The property, finite or general domain and assumptions.
- The Lean declaration and pinned checking toolchain.
- Source anchors and signature maps where source correspondence applies.
- The executable or proved bridge from the model to Rust and Python.
- Trusted parsers, serializers, hashes, compilers, runtimes, Git and filesystem operations.
- Remaining obligations.

Shared test cases establish correspondence only on those cases. Translation into Lean establishes no
source property by itself. A general implementation claim requires a correspondence proof or a
reviewed verified-generation path.

Strict verification rejects stale anchors, changed signatures, missing mappings, unbuilt packages,
warnings, hidden placeholders and proof debt above its reviewed ceiling.

## Validation policy

Use focused tests while implementing a change, then run the complete relevant gates before review:

- `tools/check.sh default` for formatting, lint, native tests, capability coverage, prose and kernels.
- `tools/check.sh wasm` for browser feature builds and tests.
- `tools/check.sh deep` for repository-scale agreement, conformance, round trips and exhaustive Lean
  comparisons.
- The playground CI job for the generated browser package and exported behavior.
- External replay for pinned consumer patches, independent oracles and exact reversal.
- Pinned language and framework compilers for generated or migrated artifacts.
- Python pytest and `ty` for the SDK and runtime.

A clean syntax tree or supported capability cell does not complete a feature. Each change needs the
behavioral, reversal, identity and refusal evidence required by its contract.

Routine autonomous evaluations use the weakest economical model exposed by the installed Codex CLI
at its lowest supported reasoning effort. Record the CLI version, model catalog entry, model,
effort, service tier and authentication mode. Keep infrastructure failures separate from agent
failures.

## Explicit boundaries

- `fr` remains standalone and does not depend on a running language server.
- Static analysis cannot settle runtime-generated names, external callbacks, reflection or every
  dynamic dispatch target. Reports preserve that uncertainty.
- Package-manager solving remains delegated to reviewed package-manager commands. Offline evidence
  verifies supplied bytes against captured integrity fields but does not authenticate lockfile authors.
- Framework facts describe observed static structure. Reviewed compiler and runtime commands provide
  execution evidence.
- Browser history covers bounded UTF-8 regular files with projected mode `0644`. Executable files,
  symlinks, staging, commits and worktrees require the native tool.
- A daemon or watch process remains out of scope unless a new measured workload shows that bounded
  batches, caching and concurrent-build coalescing are insufficient.
- Natural-language interpretation remains the agent's responsibility. `fr` accepts structured goals
  and returns deterministic guidance rather than embedding a model-dependent planner.

## Further reading

- [CLI.md](CLI.md): implemented commands and write guarantees.
- [skills/fr/SKILL.md](skills/fr/SKILL.md): the current portable agent workflow.
- [docs/agent-runtime-sdk.md](docs/agent-runtime-sdk.md): structured Python orchestration.
- [docs/agent-context-workspace.md](docs/agent-context-workspace.md): progressive disclosure and object storage.
- [RECIPES.md](RECIPES.md): recipe syntax, selection and expectations.
- [IR.md](IR.md): the semantic representation.
- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md): analysis and translation boundaries.
- [API_CONTRACTS.md](API_CONTRACTS.md): framework routes and HTTP contracts.
- [docs/agent-formalization.md](docs/agent-formalization.md): agent-authored properties and proofs.
- [docs/lean-specs.md](docs/lean-specs.md): proof evidence and external-project adoption.
- [docs/continuity.md](docs/continuity.md): completed milestone detail and current handoff.
