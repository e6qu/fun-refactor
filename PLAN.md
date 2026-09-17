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

The original numbered roadmap through PR 40 is merged. [GitHub PR 313](https://github.com/e6qu/fun-refactor/pull/313)
adds the source-derived completion audit and passing seven-family guided cohort. Current
`origin/main` is `d4b81046`. The active matched-context milestone is on
`matched_agent_context` from that exact main.

| Measure | Current value |
|---|---:|
| Parsed languages | 19 |
| Query sets | 17 |
| Entry-point catalogs | 10 |
| Capability and language cells | 456 |
| Supported cells | 311 |
| Fixed defects | 768 |
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

The older live comparison shows an `fr` context premium: 17,711 measured context tokens and 42
calls versus 11,182 tokens and 17 direct-file calls. PR 40's first passing guided cohort establishes
route completion through twenty exposed actions, with no matched direct-file arm.

The current matched Luna/low cohort gives both agents the same seven preview outcomes. The local SDK
arm completes 29 internally recorded `fr` calls through two agent calls and 46,120 input tokens. The
direct-file arm uses seven calls and 89,118 input tokens. This is a 42,998-token (48.2%) reduction;
uncached input falls from 14,878 to 14,120 (5.1%). Both arms pass with no errors, bypasses, mutation
or correction. The result is one fixed-fixture observation, not a population or billed-quota claim.

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
5. **Live-agent context saving is demonstrated on one fixed matched pair.** The SDK arm reduces
   measured input and interaction count for the seven preview workflows. Repetition across projects,
   models and source-writing tasks remains future evidence; the CLI still exposes no billed quota.
6. **Static source has a tested analysis boundary.** B5 now records indirect calls and dead-code
   conclusions that source cannot settle. The report preserves these cases as uncertainty.

## Active delivery sequence

The original roadmap is complete. New work is scoped from the explicit evidence and semantic
boundaries above rather than extending the old PR numbering indefinitely.

| PR | Outcome | Status |
|---|---|---|
| PR 40 | Completion Audit and Agent Validation | [PR 313](https://github.com/e6qu/fun-refactor/pull/313) merged |
| PR 41 | Matched Agent Context and Local Guide Execution | Implementation and fresh acceptance complete; final gates pending |

### PR 41. Matched Agent Context and Local Guide Execution

Goal: let an agent express all authored guide inputs through the IR-shaped Python SDK, keep
intermediate reports outside its transcript, and measure the resulting workflow against a matched
direct-file agent.

Implemented:

- `GuideInputs` binds exact named scalar or file placeholders. `GuideFile` supplies at most 64 KiB
  through a private temporary file removed after the preview call.
- `complete_guide` runs every read/preview action locally. Each action refreshes the guide, checks its
  immutable basis, rejects write flags and verifies the returned schema and byte ceiling.
- The binding admission rule is mirrored in Rust, Python and Lean. Exhaustive agreement covers 1,024
  count, name, bound, execution and basis combinations; Lean proves every admitted binding has all
  required conditions.
- A source-bound paired harness freezes the binary and uses the same model, effort, tasks and exact
  outcome oracle for both arms. It records complete prompts, tool events, Codex usage and all 29
  hidden SDK calls. Direct files must read the relevant Rust, framework and Lean sources.
- Two immutable diagnostics retain invented API/packet shapes and an exit-classification harness
  bug. The corrected handoff gives exact public constructors and transports the common packet apart
  from Python source.
- The fresh acceptance pair passes all seven workflow families. SDK use reduces agent calls from
  seven to two and input tokens from 89,118 to 46,120 on this fixture. Source stays unchanged and
  neither arm records a command, tool, infrastructure or isolation failure.
- `fr audit workflows`, the portable skill, SDK documentation, completion evidence, defect ledger
  and continuity record link the public contract and its limits.

Acceptance:

1. Authored recipe, migration and proof fields use public Python IR types without copying guide
   responses through the agent transcript.
2. Missing, extra, duplicated, unbounded, execution-like or stale bindings refuse before preview.
3. Rust, Python and Lean agree on the complete finite binding policy; strict source anchors are fresh.
4. The matched arms receive identical tasks, model settings and final oracle. Every relevant prompt,
   request, response, internal SDK command and Codex usage value is retained.
5. Both fresh arms pass all seven outcomes without source mutation, direct-command bypass, failed
   commands, infrastructure errors or human correction.
6. Reported savings are limited to the retained pair. Population behavior, hidden reasoning and
   billed quota remain unclaimed.
7. Default, WASM, deep, Python, strict Lean, portable-skill and documentation gates pass on the
   final review head.

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
