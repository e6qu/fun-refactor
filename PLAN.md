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

The project has merged PRs 0 through 35. The latest checkpoint is
[GitHub PR 303](https://github.com/e6qu/fun-refactor/pull/303), which binds reviewed changes to intents.

| Measure | Current value |
|---|---:|
| Parsed languages | 19 |
| Query sets | 17 |
| Entry-point catalogs | 10 |
| Capability and language cells | 456 |
| Supported cells | 311 |
| Fixed defects | 749 |
| Open defects | 1 |

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
- Native declarative `understand`, `trace`, `change`, `migrate` and `prove` intents over one immutable
  project snapshot.
- A zero-dependency Python SDK and runtime that retain complete reports locally, verify canonical
  identities and optionally persist Merkle objects outside the analyzed project.
- Recoverable source history, apply, undo, redo, compaction, Git patch export, bounded repository
  views, staging history, reviewed commits and owned-worktree lifecycle.
- Declared checks with reviewed configuration digests, original-state gates, bounded diagnostics and
  compact successful evidence.
- Lean kernels for critical admission and transition policies, strict Rust-to-Lean source anchors,
  signature maps, proof-debt ratchets and exhaustive shared Rust/Lean cases.
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
| Generated formalization | A pure, explicitly typed Rust function subset | General source/model equivalence and other source languages remain open work |

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

1. **Navigator delivery is not unified with execution.** PR 36 implements structured goal selection
   and language-aware read/preview guidance. Exact scalar goals with checks reuse task delivery;
   other routes still retain their existing authoring and review protocols until PR 37.
2. **Intent actions cover one direct task change.** They do not yet bind recipes, framework
   migrations or proof work. Multi-target selections also lack the same intent and review identity.
3. **Proof support is not language-wide.** Generated model and property plans currently cover a
   conservative Rust subset. Other languages can use manual Lean specifications and strict anchors,
   but do not receive the same source-free generated workflow.
4. **Implementation correspondence remains limited.** Lean proves properties of the model. Source
   anchors and signature maps identify the modeled declaration, while tests cover selected cases.
   The parser, IR extraction and lowering are still trusted or integration-tested boundaries.
5. **Framework transformation is not yet generic.** The project model covers the requested web stack,
   but checked migration focuses on one-file Next.js and FastAPI route features.
6. **Support is uneven across languages.** Basic structure is broad; semantic authoring, call
   analysis, translation, framework migration and formalization have smaller support matrices.
7. **The live-agent context target is unmet.** Agents still spend calls on route discovery and
   contracts. The tool can select those routes and join their delivery protocols deterministically.
8. **One source-evidence defect remains open.** B5 covers indirect calls and dead-code conclusions
   that source cannot settle. Actionable inference gaps should close; inherently dynamic cases must
   remain explicit uncertainty rather than false precision.

## Active delivery sequence

Large reviewable pull requests group the remaining work. Each PR must update this file,
continuity, the portable skill, command documentation, defect records and source-bound evidence when
its public contract changes.

| PR | Outcome | Status |
|---|---|---|
| PR 36 | Language-Aware Agent Workflow Navigator | Draft [PR 305](https://github.com/e6qu/fun-refactor/pull/305) |
| PR 37 | General Intent Actions and Proof Delivery | Planned |
| PR 38 | Cross-Language Formalization and Correspondence | Planned |
| PR 39 | Generic Hierarchical Framework Transformation | Planned |
| PR 40 | Completion Audit and Agent Validation | Planned |

### PR 36. Language-Aware Agent Workflow Navigator

Goal: let an agent state a structured outcome and receive the smallest valid end-to-end route. The
agent should not need to learn the command tree, DSL, semantic schemas or proof workbench first.

The implementation now includes native `guide`, Python goal/guide mirrors, source-free targeted
route contracts, fresh action following and direct checked scalar delivery. Generic framework
previews, actual Lean proof tasks and all parser-language structural guides have executable fixtures.
Rust, Python and Lean compare 32,256 admission cases and 336 lifecycle cases. The fixed complete
program comparison saves 115 exposed bytes against inline task discovery while adding two processes
and 8,984 internal bytes.
Broader agent-context and general delivery improvements remain PR 37 and PR 40 work.

Deliverables:

- Add `fr-agent-goal-1`. It carries a purpose, optional selector or full handle, desired operation,
  constraints, declared checks, proof expectation and context limits. It contains structured data,
  not free-form source or a model-dependent natural-language interpretation.
- Add `fr-agent-guide-1`, exposed through the CLI and Python SDK. It reports the current workflow
  state, detected language and technology surfaces, capability evidence, recommended route,
  alternatives, refusal reasons and remaining uncertainty.
- Represent every next step as exact argument arrays plus an expected output schema. Name only the
  fields the agent must author, such as a new scalar, typed node, recipe decision, property tree or
  proof tactics.
- Route across direct refactorings, recipes, semantic scalar intents, semantic changes, complete body
  authoring, surface edits, framework migrations and formalization tasks.
- Derive recipe and semantic forms from the live vocabulary and catalog. Derive operations and proof
  forms from current capability predicates and task schemas. Do not create a second manual registry.
- Prefer source-free routes and explain when exact source is required. Preserve Merkle identities,
  omissions, coverage and byte ceilings in every state.
- Return a verification ladder that distinguishes reparse, compiler, declared checks, behavioral
  oracles, model theorems and implementation correspondence.
- Extend the portable skill so an agent starts with the guide and loads a specialized reference only
  when the returned state requires agent-authored content.
- Model route admission and workflow-state transitions in Lean. Compare Rust, Python and Lean across
  every finite purpose, language class, target kind, route, support and lifecycle state.

Acceptance:

1. A guide for every supported capability emits a command that the same binary accepts for a matching
   fixture, or emits a precise unsupported state.
2. The guide never recommends a write before a complete preview and review basis.
3. A compact guide contains no source, full semantic body, full vocabulary or unrelated route.
4. Repeated input on one revision returns byte-identical semantic guidance. Changed capability,
   target, project or schema material changes its basis.
5. Representative Rust, Go, Java, JavaScript/TypeScript/TSX, Python, Zig, Bash, markup, stylesheet,
   configuration, Markdown and Lean goals reach the correct route without exploratory help calls.
6. Deterministic evaluation compares the guided route with the best existing documented route and
   counts agent-visible requests, responses, bytes and actions.
7. Default, WASM, playground, Python, skill, prose, strict Lean, deep and replay gates pass.

### PR 37. General Intent Actions and Proof Delivery

Goal: let `understand`, `trace`, `change`, `migrate` and `prove` intents carry the reviewed operation
appropriate to their purpose, including multi-target and proof workflows.

Deliverables:

- Replace the single untagged intent action with versioned tagged action variants for task changes,
  recipes, author batches, framework migrations, formal plans and proof submissions.
- Permit bounded project requests and multiple exact targets when the action contract needs them.
  Resolve every reference in the same immutable snapshot as the evidence packet.
- Reuse each existing planner and writer. The intent layer must not implement a second recipe,
  semantic writer, migration engine, proof checker or transaction journal.
- Give every preview one outer identity over intent, selected evidence, action plan, declared checks,
  proof expectations and delivery policy.
- Execute only the unchanged outer review. Preserve original checks, failure diagnostics, recovery,
  undo, redo and patch export where the action writes source.
- Let `prove` intents progress from candidate or property task through scaffold or proof submission.
  The agent still authors every property and tactic. Lean must accept generated modules and submitted
  tactics before history creation.
- Return one normalized result envelope with purpose-specific evidence and explicit claims. A theorem
  over a model must never set implementation correspondence to true without separate evidence.
- Mirror every action and result in the Python SDK without hiding the public IR shape.
- Formalize action-purpose admission, review completeness and lifecycle transitions in Lean. Add
  exhaustive shared cases and adversarial stale, tampered and cross-purpose tests.

Acceptance:

1. Each navigator route can become an intent action without reconstructing a different target or
   losing its evidence basis.
2. Multi-target and referenced requests remain bounded, deterministic and stale-safe.
3. Recipe, migration, task, scaffold and proof results match their standalone engines exactly.
4. Every source-writing action has checked preview, apply, undo, redo and patch evidence.
5. Every proof-writing action has checked goal, tactics receipt, strict correspondence and Lake build
   evidence, with remaining implementation obligations stated.
6. Wrong purpose, incomplete review, changed agent input, stale source or mismatched basis refuses
   before mutation.
7. All repository gates and source-bound parity evaluators pass.

### PR 38. Cross-Language Formalization and Correspondence

Goal: make Lean formal verification adoptable through one bounded workflow for each applicable
language. Tighten the connection between source, shared IR and generated models.

Deliverables:

- Define an executable semantics for a deliberately small shared IR kernel. Cover pure values,
  bindings, conditionals, returns, tuples, records, lists, options/results and selected arithmetic
  with explicit overflow and partiality policies.
- Prove general laws used by extraction and lowering, including name resolution, precedence,
  substitution, capture avoidance and deterministic evaluation for the admitted subset.
- Add source-free formalization candidates wherever an imperative parser can produce the kernel.
  Cover Rust, Go, Zig, Java, JavaScript/TypeScript, TSX/JSX, Python, Bash and Lean.
- Give declarative languages structural and provenance properties over their high-level models rather
  than pretending they share function semantics.
- Generate language-specific source anchors and canonical signature maps, then lower admitted
  semantic IR into one reviewed Lean model and property workflow.
- Report every excluded construct with its semantic reason. Effects, concurrency, reflection,
  dynamic dispatch, undefined behavior, exceptions and framework runtime behavior remain separate
  until modeled explicitly.
- Add differential execution for finite generated inputs where both the source toolchain and Lean
  model can run. Keep tests distinct from a proof of general implementation equivalence.
- Add correspondence proofs or a justified verified-generation path for the first complete kernel.
  Evidence reports must identify exactly which declarations reach that stronger level.
- Preserve custom properties, proof tasks, proof checking, proof-region writes, regeneration, debt
  ratchets, CI, undo, redo and Git patches across every admitted language.
- Extend the Python SDK with language-neutral formal plan and evidence types that mirror the public
  protocol.

Acceptance:

1. Every advertised formalization-language cell completes candidate, plan, scaffold, property,
   proof, drift, verify and reversal fixtures, or reports an exact unsupported boundary.
2. Generated Lean elaborates before source-history mutation and remains stable under unchanged input.
3. Source, shared IR, model, property and proof identities change independently when their own
   material changes.
4. Evidence distinguishes anchor freshness, signature correspondence, model theorem, differential
   execution and implementation correspondence.
5. Lean proves the shared semantic laws. Rust and Python agree with executable Lean policies and
   independent canonical identities.
6. No source language receives a general-equivalence claim from translation or testing alone.
7. The full native, language-toolchain, WASM, strict-proof and deep audit gates pass.

### PR 39. Generic Hierarchical Framework Transformation

Goal: translate supported application features through a common hierarchy rather than through
example-specific or pair-specific source templates.

Deliverables:

- Define a versioned application IR for packages, modules, routes, handlers, request and response
  schemas, middleware, dependencies, configuration, components, pages, styles and service calls.
- Derive the IR from the existing React, Next.js, Express.js, FastAPI and Go HTTP evidence. Preserve
  unknown runtime behavior and unsupported framework constructs as explicit dispositions.
- Replace direct pair logic with adapters that read and write the common hierarchy. Keep adapters
  independent of fixture names, endpoint names and domain examples.
- Support checked transformations among the advertised frontend and backend adapters when their
  feature subsets overlap. Never infer a pair from a shared host language alone.
- Plan registration, dependencies, imports, configuration and cutover as one revision-bound
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
