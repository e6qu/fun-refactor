# fun-refactor roadmap

`fr` helps an agent understand a project with little context, change its structure,
and inspect evidence that each change meets its requirements.
The same tool should help other projects adopt Lean specifications incrementally.

This is the active roadmap. Git history preserves the original stages and their progress log.
[BUGS.md](BUGS.md) records defects. [CHANGELOG.md](CHANGELOG.md) records releases.
[CLI.md](CLI.md) documents commands that exist today. Names proposed below are design work.

## Current foundation

The core reads 19 languages. It supports 311 of 456 capability × language pairs.
The remaining pairs carry a reason in `fr capabilities`.
A supported cell describes an operation's scope; particular inputs can still require review or refuse.

| Measure | Current value |
|---|---|
| Query sets | 17 |
| Entry-point catalogs | 10 |
| Capabilities × languages | 24 × 19 |
| Supported pairs | 311 of 456, every other one carrying its reason |
| Defects fixed | 681 |
| Defects open | 1 |

Implemented foundations:

- Syntax trees, symbols, scopes, confidence tiers, byte edits and a content cache.
- Navigation, implementations, usages, call graphs, flow, impact and entry points.
- Rename, extract, inline, move, signature changes, imports, delete and structural rewrites.
- Cross-language references, configuration provenance and configuration-to-code traces.
- A shared translation IR with Rust, Go, Java, Python, TypeScript, Zig, Bash and Lean readers and writers.
- Next.js/FastAPI route conversion and OpenAPI service scaffolds, with explicit limits.
- Local recipes, expectations, workspace previews and canonical formatting.
- Native releases, a WASM API, a browser playground and patch downloads.
- Lean edit, position, history, pagination, confidence and workspace membership models, source anchors, signature maps and `spec check`, `sync` and `verify`.

Important gaps:

- `fr project` adds bounded maps, revision-bound details, call relationships and Cargo/npm manifest views. Complete dependency resolution and framework semantics remain pending.
- Native changes now have persistent history and checked undo/redo. History retention and large-journal scaling need further work.
- Browser undo restores the loaded workspace; it does not reverse individual transactions.
- Native history exports Git text patches. Repository status, staging and shared browser patch semantics remain pending.
- Strict spec signature maps currently accept Rust source declarations only.
- Framework readers cover selected patterns. Whole applications still need dependency and runtime work.
- Model proofs and shared executable cases do not establish general correspondence with the Rust implementation.

## Product contract

An agent should navigate from a project map to a module, symbol contract, relationships,
and selected implementation. It should request full bodies only when needed.
Each answer must state its coverage, uncertainty, source basis and omitted results.

A change should have one identity from preview through validation, patch export, apply,
undo and redo. Existing user changes must survive operations outside their scope.
Syntax validation, compilation, behavioral tests and formal proofs answer different questions.
Reports must distinguish them.

Use the CLI and its library as the common interface. Agent skills teach the workflow.
An additional transport can follow demonstrated integration needs.
Keep the core independent of a running language server.
Lean and project compilers are explicit verification dependencies.

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

## Milestones and order

### M0. Recover ordinary commit failures (complete)

The shared native commit path now provides:

- Stage replacements and recovery copies before replacing a target.
- Restore earlier targets if a later write fails; remove files this transaction created.
- Preserve file modes and refuse stale plans.
- Preserve unrelated concurrent changes and retain recovery material when restoration fails.
- Emit one JSON result after commit, with structured recovery evidence on failure.
- Route recipe formatting through the shared commit path.
- Exercise failure during preparation, application and recovery.

Exit: deterministic failure tests cover restored bytes, file existence, modes and recovery errors.
JSON success reports follow completed writes.
The guarantee covers errors the process handles. Crash recovery and persistent undo belong to M1.
Filesystem observers can see intermediate replacements; multiple renames are not one filesystem transaction.

Validation: `tools/check.sh` passes, including capability coverage and Lean kernels.
Failure regressions cover each application position, failed recovery, concurrent changes and a 256-file commit under a descriptor limit.
`tests/json_surface.rs` checks failed writes across refactoring, translation, scaffolding, recipes and formatting.

### M1. Persistent plans, recovery and undo/redo (complete)

Schema 1 stores canonical workspace identity, affected-file basis digests, project source digests,
before/after snapshots, file existence, Unix modes and validation labels.
`--save-plan` records a preview. Native CLI writes record and apply a transaction.
`fr history` exposes list, show, apply, undo, redo and checked recovery by ID.
Recipes, recipe formatting, translations, scaffolds and OpenAPI output use the journal.

Durable checkpoints bracket source changes. A pending operation blocks further writes.
Recovery accepts mixed before/after states and restores the operation's starting snapshots.
Undo/redo require stack order and refuse conflicting content, existence or modes.
New application abandons the redo branch; saving a plan preserves it.
Schema 1 retains all records and source text until the user removes the journal.

Validation covers process exit after each apply/undo/redo file write, repeated recovery,
ordinary I/O failures, conflicting edits, branching and source revision drift.
The Lean history model covers snapshot acceptance, inverse laws, mixed-state recovery and stack transitions.
A corpus of 250 cases compares the anchored Rust snapshot predicate with Lean.
The full native/WASM gate, strict anchor checks and Lean build pass.
Proofs assume storage honors durable checkpoints and atomic rename; they do not prove filesystem implementation correspondence.

Limits: source digests cover recognized files with documented generated-directory exclusions.
Snapshots omit ownership, timestamps and extended attributes. Empty directories and orphaned staging files can remain.
Large histories need a storage/retention policy before broad deployment.

### M2. Compact project understanding (in progress)

M2a is complete. It provides a versioned `fr project` surface:

- Paged maps of directories, files and lexical symbol containment, with field selection and explicit omissions.
- Short IDs bound to a shared source revision, plus full handles and query-bound cursors.
- Bounded syntax headers, UTF-8 source slices, imports and indexed references with confidence and unresolved targets.
- Coverage counts and paged diagnostics for partial parsing, skipped paths and unsupported files.
- Lean page-length bounds, progress and partition laws with 1,728 Rust/Lean comparison cases on 64-bit hosts.

Existing JSON contracts remain intact. The new surface emits compact column-and-row maps.
`tools/project-context.py` checks nonlocal symbol identities and measures output bytes, calls and latency.
[The initial evaluation](docs/project-context-evaluation.md) records results and their limits.
M2a validation passed the full native/WASM gate, strict Lean verification and 11 focused CLI tests.

M2b1 is complete. It adds declared package boundaries and dependencies:

- Paged `project packages` and `project dependencies` for Cargo and npm manifests.
- Package roots, dependency sections, Cargo target conditions and literal workspace patterns.
- Explicit inheritance flags, partial-field diagnostics and unresolved declarations.
- Manifest snapshots participate in revisions, cursor checks and final verification.
- Discovery respects scan scope, ignores, size limits and symlink exclusions.

The declaration view preserves raw requirements and patterns separately from links.
Registry and lockfile resolution, complete workspace semantics and other ecosystems remain pending.
The shared page-length proofs apply; manifest parsing and interpretation are not formally verified.
Validation passes the full native/WASM gate, strict Lean verification and 18 focused project CLI tests.
The repository's 11 Cargo/npm manifests produce no manifest diagnostics.

M2b2 is complete. It adds local manifest links and workspace pattern candidates:

- Paged `project links` with manifest filters, bounded fields and revision-bound cursors.
- Cargo path dependencies and npm file dependencies link only to observed package manifests.
- Cargo aliases require matching target names; version and installation checks remain explicit gaps.
- Fixed-depth workspace patterns, Cargo exclusions and explicit unresolved ownership cases.
- Snapshot-only traversal refuses symlinks, unobserved directories and paths outside the selected root.
- Lean depth-preservation and literal-or-star matcher laws, with 67,081 Rust/Lean comparisons.

Pattern rows remain candidates. M2b3 adds a separate ownership and membership view.
The matcher proof does not establish filesystem containment, package-manager resolution or general Rust correspondence.
Validation passes the full native/WASM gate, strict Lean verification and 26 project CLI tests.
A simple workspace fixture agrees with `cargo metadata`; the repository view reports eight local links and nine member-pattern matches.

M2b3 is complete. It adds observed Cargo ownership and inherited local links:

- Paged `project workspaces` with root, declared and automatic membership evidence.
- Own-workspace, explicit-pointer and nearest-ancestor ownership within the selected snapshot.
- Transitive local path membership, including inherited paths, with exclusions and cycle termination.
- Inherited local dependencies resolve from the owning workspace root and preserve aliases.
- Ignored or unreadable ancestor boundaries block inheritance and participate in snapshot checks.

This is a bounded Cargo subset. Registry resolution, feature evaluation and package-manager validation remain unchecked.
M2b15 and M2b16 add general closure model proofs; full implementation correspondence remains pending.
Validation passes the full native/WASM gate, strict Lean verification and 34 project CLI tests.
The repository view identifies its root package and eight declared Cargo members.

M2b4 is complete. It adds bounded call and implementation relationships:

- Paged `project calls` for incoming, outgoing and internal call sites, including nested definitions and file-scope calls.
- Preserved graph confidence, dispatch origins and unresolved sites, with handles for subsequent detail retrieval.
- Paged `project implementations` for selected declarations, explicitly labeled as hierarchy candidates.
- Workspace analysis gaps and unsupported-language counts, with cursors bound to scope, direction, revision and resulting rows.

Output is bounded; these queries still build workspace analysis. Empty results do not prove the absence of relationships.
The existing Lean page-length laws apply. Call resolution, hierarchy inference and snapshot correspondence remain outside those proofs.
The 40 project CLI tests pass, including relationship preservation, scope, pagination, coverage and stale-handle checks.
Validation passes the full native/WASM gate and strict Lean verification, with all five source anchors fresh.

M2b5 is complete. It adds bounded route declarations:

- Paged `project routes` for selected directories and files, using captured source.
- Express, Flask, Axum, Gin and Spring declaration patterns with explicit framework uncertainty.
- Separate paged handler candidates preserve same-file name ambiguity and provide inspection handles.
- Bounded URLs and names, syntax and language gaps, shared revision checks and query-bound cursors.

This view reports pattern candidates. It does not establish framework identity or runtime reachability.
Mounted routers and cross-file handlers remain pending. Later milestones add partial contracts and bounded Next.js/FastAPI inspection.
The shared Lean page-length laws apply; route extraction and handler matching remain outside those proofs.
Validation passes the full native/WASM gate, all 46 project CLI tests and strict Lean verification with five fresh source anchors.

M2b6 is complete. It adds bounded configuration-to-code inspection:

- Paged `project configuration` declarations, candidate consumers and reads without observed declarations.
- Captured-source analysis rejects missing, mismatched and syntactically broken inputs with explicit gaps.
- Separate consumer rows preserve name-only confidence, competing declarations and bounded page sizes.
- Directory/file scopes follow selected declarations, candidate values files or code reads across the workspace.
- Bounded conditions and values paths retain the existing nearest-ancestor leaf-name lookup as a candidate.

Literal environment values and full source lines stay outside the compact response.
Accessor text can match comments or strings and miss dynamic or lowercase names; deployment identity and precedence remain unchecked.
The shared Lean page-length laws apply. Configuration inference and general source/model correspondence remain outside those proofs.
Validation passes the full native/WASM gate, 52 project CLI tests and three snapshot-analysis tests.
Strict Lean verification passes with all five source anchors fresh.
The check script now defaults Go's build cache to the checkout, alongside its Zig cache, for sandboxed compiler tests.

M2b7 is complete. It adds bounded test candidates and call-path witnesses:

- Paged `project tests` for directory, file and symbol scopes, using built-in test catalogs.
- Captured-source catalog checks report missing, mismatched and syntactically broken inputs.
- One shortest call-path witness per candidate, with depth limits, cycle termination and separate paged edges.
- Preserved rule IDs, edge confidence and dispatch origins, with explicit catalog, hierarchy and coverage gaps.
- An anchored Lean confidence model proves non-strengthening and tier bounds, with 5,461 shared Rust/Lean cases.

Catalog entries include fixtures and helpers; paths establish inferred relevance, without proving runner discovery or runtime coverage.
The graph uses fresh hierarchy analysis and final snapshot checks, as the existing call view does.
Output and witness depth are bounded; workspace analysis cost, catalog accuracy and graph correspondence remain outside the model proofs.
Validation passes the full native/WASM gate, 58 project CLI tests and two captured-source catalog tests.
Strict Lean verification passes with all six source anchors fresh.

M2b8 is complete. It adds partial route request/response contracts:

- Paged `project contracts` shares route IDs and handler handles with the declaration view.
- Separate path parameter, Axum extractor, Spring binding and declared return-type rows preserve evidence and ambiguity.
- Captured-source signature inspection omits handler bodies and parameter defaults, with bounded names and type spellings.
- Explicit gaps cover unsupported parameters, absent return annotations, complex path markers and unresolved handler signatures.

Every contract remains partial. Declared types do not establish serialization, runtime validation, status codes or media types.
The shared Lean page-length laws apply; signature extraction and wire correspondence remain outside those proofs.
M2b11 adds separate declared-field inspection; recursive expansion and type/import resolution remain pending.
M2b9 and M2b10 add partial Next.js and FastAPI contract evidence.
Validation passes the full native/WASM gate and all 66 project CLI tests, including eight new contract regressions.
Strict Lean verification passes with all six source anchors fresh.

M2b9 is complete. It adds Next.js App Router declaration and contract inspection:

- Captured-source readers recognize named HTTP function exports under root `app` and `src/app` layouts.
- URLs preserve `/api`, parameter spelling and static segments, with route groups outside the URL.
- Handler matches use declaration positions, with separate candidates and diagnostics for duplicate exports.
- Existing contract pages expose path names and declared return types, while input bindings remain unknown.
- Unsupported paths and export forms produce paged diagnostics with explicit interpretation limits.

This covers `route.ts` and `route.js`, static segments, route groups and simple dynamic segments.
M2b12 adds terminal catch-all paths and local function export aliases.
M2b13 adds direct variable handlers and nested npm package evidence.
Pages Router, wrapped handlers, cross-file re-exports and custom extensions remain outside the reader.
Framework identity, layout precedence, runtime route validity, `basePath`, rewrites and implicit methods remain unchecked.
The shared Lean page-length laws apply; URL extraction and handler correspondence remain outside those proofs.
Validation passes the full native/WASM gate and all 73 project CLI tests, including seven new Next.js regressions.
Strict Lean verification passes with all six source anchors fresh.

M2b10 is complete. It adds FastAPI declaration and contract evidence:

- Captured imports and direct constructor assignments identify candidate receivers for top-level verb decorators.
- Handler matches use declaration positions; FastAPI evidence replaces overlapping Flask guesses, including unsupported-path diagnostics.
- Paged explicit parameter markers preserve aliases and type spellings while omitting defaults and validation metadata.
- Separate decorator response models and Python return annotations preserve stacked-decorator metadata and explicit disabled models.
- Dynamic paths, unsupported bindings, computed models and expanded decorator options retain explicit gaps or null fields.

M2b11 adds separate declared-field inspection. Implicit parameter classification, dependency injection, imports at runtime and routing composition remain unchecked.
The shared Lean page-length laws apply; FastAPI extraction and wire correspondence remain outside those proofs.
Validation passes the full native/WASM gate and all 81 project CLI tests, including eight new FastAPI regressions.
Strict Lean verification passes with all six source anchors fresh.

M2b11 is complete. It adds bounded declared schema inspection:

- `project schemas` pages direct Python class annotations and TypeScript interface/object-alias properties from captured source.
- Separate fields, type references and candidate endpoints preserve duplicate definitions and support following declaration handles.
- Same-file matching uses full type spellings before clipping, with unresolved imports, qualified names, builtins and lexical scope.
- TypeScript optional/readonly markers describe syntax; wire requiredness and runtime schema identity remain unknown.
- Explicit gaps cover validation, inheritance, generics, decorators, duplicate fields and unsupported members or types.
- Types and names have UTF-8 limits; metadata, defaults and bodies stay outside the output, and references do not recursively expand cycles.

M2b14 adds Rust structs and optional contract type-name candidates. Type resolution and wire-model inference remain unchecked.
Runtime schema builders such as Zod remain outside this declaration subset.
The shared Lean page-length laws apply; schema extraction and type resolution remain outside those proofs.
Validation passes the full native/WASM gate and all 90 project CLI tests, including nine new schema regressions.
Strict Lean verification passes with all six source anchors fresh.
The FastAPI fixture exposes 19 declarations and 44 direct fields; five-row pages retain continuation cursors.

M2b12 is complete. It extends Next.js routing and binding evidence:

- Terminal catch-all and optional catch-all paths retain distinct URL templates and paged parameter cardinality.
- Malformed names, repeated parameters and nonterminal catch-alls produce explicit diagnostics.
- Local named HTTP exports follow top-level function declarations, with separate export and handler positions.
- Duplicate definitions remain separate candidates; type-only exports supply no HTTP candidates.
- Unresolved bindings and cross-file exports retain gaps without loading handler bodies; M2b13 extends direct variable handlers.

Runtime route validity, reassignment, import resolution and implicit methods remain unchecked.
The shared Lean page-length laws apply; path extraction and export correspondence remain outside those proofs.
Validation passes the full native/WASM gate and all 98 project CLI tests, including eight new Next.js regressions.
Strict Lean verification passes with all six source anchors fresh.
The Petstore fixture now yields 13 route candidates without Next.js reader gaps; five-row contract pages retain continuation cursors.

M2b13 is complete. It adds Next.js variable handlers and nested package evidence:

- Direct arrow/function-expression bindings support exported variables and local aliases, with precise binding handles.
- Contract rows read initializer annotations, omit defaults and bodies, and report unknown inputs including bare arrow parameters.
- The shared contract reader also exposes initializer annotations for same-file Express handler candidates.
- Captured npm dependencies identify nested app candidates, with explicit root/manifest evidence and package-relative URLs.
- The nearest observed package boundary prevents borrowing an outer package's dependency evidence.
- Unsupported initializers and missing or invalid nested package evidence retain explicit gaps.

Runtime layout precedence, configuration, reassignment, dependency installation and import resolution remain unchecked.
The shared Lean page-length laws apply; binding extraction and package-to-framework correspondence remain outside those proofs.
Validation passes the full native/WASM gate and all 106 project CLI tests, including eight new Next.js regressions.
The existing Express regression now checks initializer annotations and preserves unresolved inline/external handlers.
Strict Lean verification passes with all six source anchors fresh.

M2b14 is complete. It adds Rust schema fields and optional contract type references:

- Named and unit Rust structs join schema pages, with explicit gaps for unsupported declarations, attributes, conditions and type expressions.
- `contracts --types` pages references from supported declared returns and Axum request types, with field IDs and reference counts.
- Separate same-file declaration candidates preserve duplicate definitions and unresolved names, with followable `schemas`/`show` handles.
- Full names drive matching before UTF-8 clipping; cycles do not recursively expand, and unsupported expressions produce no partial references.
- Type-reference pages bind the mode, scope and revision and retain captured-source analysis with final drift checks.

FastAPI marker/model references, Spring request types and broader language coverage remain pending.
Type/import resolution, const/type namespaces, serialization and runtime validation remain unchecked.
The shared Lean page-length laws apply; schema extraction and reference correspondence remain outside those proofs.
Validation passes the full native/WASM gate and all 114 project CLI tests, including eight new schema/reference regressions.
Strict Lean verification passes with all six source anchors fresh.

M2b15 is complete. It isolates and formalizes observed workspace membership expansion:

- Build eligible dependency edges once, then use an anchored synchronous expansion helper while preserving deterministic membership witnesses.
- Prove seed preservation, exact one-step additions, monotonicity and reachability soundness for every round.
- Prove containment in any closed superset and exact reachable membership when expansion stabilizes.
- Compare 20,750 shared Rust/Lean rounds across all directed graphs and seed sets through three nodes, plus four 64-bit duplicate/limit cases.
- Independently check final reachability with a queue traversal and retain CLI coverage for competing paths, disconnected cycles, excluded packages and distinct owners.

M2b15 leaves general convergence to M2b16. Graph construction, witness selection and full Rust implementation correspondence remain unproved.
Cargo rule coverage is unchanged; the next rule extension remains separate from these model claims.
Validation passes the full native/WASM gate, all 116 project CLI tests and 13 Lean integration tests; two exhaustive self-audits remain in the separate deep gate.
Strict Lean verification passes with all seven source anchors fresh and zero `sorry` obligations.

M2b16 is complete. It proves general convergence of the workspace membership model:

- Prove that equal membership yields equal sorted, duplicate-free representations.
- Show that every changing round removes a missing candidate, with stabilization within the number of supplied edges.
- Derive unconditional exact reachability, least-closure correctness and persistence of the fixed point.
- Compare the executable closure with Rust across 4,165 small graph/seed configurations, a 64-bit limit case and three longer chains.
- Exercise the round bound on chains requiring exactly 4, 16 and 64 changing rounds.

The proofs cover arbitrary finite seed and edge lists. Cargo interpretation, graph construction and complete Rust correspondence remain separate obligations.
The convergence proofs use standard propositional extensionality, quotient soundness and classical choice. They introduce no custom axioms.
Validation passes the full native/WASM gate, all 116 project CLI tests and 13 Lean integration tests; two exhaustive self-audits remain in the separate deep gate.
Strict verification passes with seven fresh source anchors, zero `sorry` obligations and all 25 Lean build jobs successful.

M2b17 is complete. It corrects Cargo subtree exclusions and explicit-member precedence:

- Apply literal exclusions to directory subtrees in both membership and pattern-candidate pages.
- Let explicit member paths override exclusions for their descendants, including automatic path members and inherited dependencies.
- Report glob-shaped exclusions as unsupported instead of applying wildcard semantics that Cargo does not use here.
- Compare eleven nested-workspace configurations with `cargo metadata`, including Unicode paths, manifest paths, neighboring names and transitive dependencies.
- Preserve captured-manifest analysis and reject cursors after exclusion or member declarations change.

Parent-relative members follow in M2b18. Broader member globs and complete Cargo validation remain pending.
The existing Lean matcher and closure models retain their scope; exclusion interpretation has regression evidence, not an implementation proof.
Validation passes the full native/WASM gate, all 120 project CLI tests and strict Lean verification with seven fresh anchors and zero `sorry` obligations.

M2b18 is complete. It adds parent-relative Cargo member patterns within the captured snapshot:

- Resolve leading parent components without escaping the selected project root or reading additional files.
- Match literal and fixed-depth sibling member patterns, with explicit-pointer ownership and transitive inherited path membership.
- Retain candidate-page ownership gaps and refuse missing, excluded or distinct workspace owners.
- Preserve declared-path exclusion precedence, including Cargo's refusal to treat parent aliases as equivalent literal overrides.
- Compare sibling configurations with `cargo metadata` and cover pagination, captured sources and narrower inspection scopes.

Parent components after literals or wildcards, parent-relative exclusions and npm parent patterns remain unsupported.
The existing Lean matcher and closure proofs apply to supplied components and edges; path interpretation and ownership correspondence remain unproved.
Validation passes the full native/WASM gate, all 124 project CLI tests and strict Lean verification with seven fresh anchors and zero `sorry` obligations.

Next M2b work:

- Extend workspace rules beyond the observed Cargo subset and strengthen implementation correspondence.
- Extend request/model reference evidence and remaining schema subsets without guessing imports or runtime validation.
- Evaluate bounded inspection tasks on unfamiliar repositories with an agent, a pinned tokenizer and correctness checks.
- Measure model tokens and task success against file reading, including additional calls and uncertainty.
- Improve repeated-query cost and bound analysis work where measurements justify it.

Exit: agents answer project questions and identify change sites without loading unrelated implementations.
Context reduction must preserve needed contracts, uncertainty and task success.
The current byte comparisons do not close that acceptance requirement.

### M3. Git patches and repository integration

M3a: recorded text patch export (complete).

- Add `fr history patch ID`, reverse export and optional JSON metadata.
- Generate from recorded snapshots without Git or working-tree content reads.
- Cover text additions/deletions, empty files, executable modes, unusual paths and missing final newlines.
- Represent recorded moves as deletion/addition pairs without adding new mutation commands.
- Refuse binary snapshots and permission changes that Git modes cannot represent.
- Check Git application in both directions and preserve unrelated staged, unstaged and untracked changes.

See [patch scope and usage](docs/git-patches.md).
Tests check patch rendering against Git; implementation correspondence remains unproved.
The existing recorder does not yet expose every change kind that history snapshots can represent.
Validation passes the full native/WASM gate and Git round trips across 18 change cases.
Strict Lean verification retains seven fresh anchors and zero `sorry` obligations.

M3b: receiving patch basis checks (complete).

- Add `fr history patch ID --check`, with optional `--against DIR` and reverse direction.
- Report affected-file content, existence and Git mode matches without source text or a patch.
- Keep full recorded-permission equality separate from patch basis equality.
- Return a failing exit code for mismatches and preserve source, history and Git state.
- Reject unsafe receiving paths, unreadable files and unsupported exports before printing a report.
- Compare results with Git application and cover differences outside patch hunks.

Full snapshot equality reuses the anchored history predicate.
Filesystem observation, Git mode projection and report correspondence remain unproved.
The check does not run Git or establish index, attribute, write-permission or project-digest compatibility.
Validation passes the full native/WASM gate, receiving checks across 18 Git round-trip cases and strict Lean verification.

M3c: Git application checks (complete).

- Add `fr history patch ID --git-check`, with optional receiving directory, reverse direction and index checks.
- Resolve repository roots and prefix nested paths so Git cannot silently skip the patch through subdirectory filtering.
- Support linked worktrees and preserve working files, history, indexes and worktree pointers.
- Report Git's verdict and bounded diagnostics without including the patch.
- Use repository configuration, clear inherited Git overrides and disable system/global configuration, monitor hooks and optional locks.
- Refuse affected content filters and ambiguous reserved filter-driver names before application checks.

Tests cover Git execution and attribute handling; Lean correspondence remains unproved.
Git checks do not freeze concurrent state or establish snapshot equality, write permissions or project validation.
Validation passes the full native/WASM gate, all five Git CLI scenarios and Git checks across 18 patch cases.
Strict Lean verification retains seven fresh anchors and zero `sorry` obligations.

Next M3 work:

Add bounded repository status pages for staged, unstaged and untracked files.
Extend Lean coverage to patch basis and executable modes, with shared Rust cases.
Extend recording for deletion and executable-mode operations before advertising those as authoring commands.
Expose changed files, staged and unstaged changes, and structural impact since a revision.

Add explicit staging, commits and isolated worktree workflows after patch correctness.
Keep Git optional for ordinary analysis and transaction history.
Undo an `fr` transaction without resetting unrelated Git changes.

Exit: patches apply to the expected basis and reproduce the recorded result.
Tests cover dirty repositories, staged changes, untracked files and conflicts.

### M4. Agent skills and bounded code authoring

Ship a small introductory skill with references for exploration, changes, recovery, Git and Lean.
Validate example commands against the released binary.
Use capability discovery and the recipe vocabulary when choosing an operation.
Load specialized instructions only when the task needs them.

Add bounded insertion and replacement of declarations or bodies where existing refactorings cannot express a change.
Reuse revision checks, edit planning, syntax validation and history.
Compose high-level intentions as inspectable recipe steps with explicit postconditions.
Select project checks from declared configuration and report what each check covers.

Exit: an agent locates, changes, validates, exports and reverses a real task using the skills.
Measure correctness and context use together. A refusal should direct the next useful inspection.

### M5. Lean adoption in other projects

Provide package initialization, declaration selection, anchored model scaffolds and proof-obligation reports.
Complete one Rust adoption workflow first, using the existing signature checker.
Choose subsequent languages from actual project demand.

Add generated-region ownership, conflict-aware regeneration, explicit signature synchronization and CI support.
Introduce named proof debt and ratchets without equating a zero count with complete verification.
An agent authors claims and searches for proofs; Lean checks them.
Ensure selected specs participate in the checked build targets.
Report assumptions, axioms, covered properties and the relationship between source and model.

Exit: an external repository adopts one useful property, detects source drift and repairs it through `fr`.
A broken property fails its check. Handwritten regions survive regeneration.
The report separates proved model properties, tested correspondence and proved implementation correspondence.

### M6. Hierarchical project and framework transformations

Add a project and framework model alongside the code IR:

- Applications, packages, features, build settings and dependency boundaries.
- Backend routes, schemas, handlers, middleware, authentication boundaries and service dependencies.
- Frontend components, properties, events, state, effects, styles and rendering boundaries.
- Source anchors, confidence, unsupported constructs and validation evidence for each fact.

Support moving a feature, extracting a route group, reorganizing a package and updating connected callers.
Each operation selects a subtree and follows the relationships its contract requires.
Build framework readers and writers for explicit patterns and versions.
Keep framework-specific semantics visible where the common model cannot express them.

Start with the existing Next.js/FastAPI backend pair.
Choose one frontend pair and a bounded component subset before implementing that migration.
A feasibility report must separate automatic steps, agent decisions and unsupported behavior.
Migration should work feature by feature and allow mixed frameworks during transition.

Exit: migrate a real feature with contract checks, build and execution evidence, a patch and undo/redo.
Check middleware order, authentication, validation and lifecycle behavior where the migration claims to preserve them.
Every unsupported construct remains visible in the plan.

## Formal verification across milestones

Prioritize properties whose failure silently changes code or misleads an agent.
Extend the existing edit and position kernels, then cover transaction laws and selected analysis kernels.
Define executable semantics for a small IR subset before proving its transformations.
Expand arithmetic, precedence, scope and rewrite properties as the supported subset grows.

Every proof record must name:

- The property, domain and assumptions.
- The Lean declaration and checking toolchain.
- Source anchors and signature correspondence where available.
- The evidence connecting the model to the implementation.
- Remaining obligations and trusted components.

Shared corpora test correspondence on their cases.
General implementation claims require a proof of correspondence or a justified verified generation path.
Translation to Lean alone establishes neither.
Parser, compiler, runtime and filesystem assumptions must remain explicit.

## Acceptance and validation

Use focused regressions during implementation and `tools/check.sh` for the PR gate.
The separate playground CI job builds WASM, typechecks the UI and exercises exported behavior.
`tools/check.sh deep` runs repository-scale audits after merge and on demand.
Preserve compile checks, refusal evidence, capability coverage and Lean kernel checks.

The product acceptance scenario is an unfamiliar project:

1. Inspect a compact map and find the relevant feature.
2. Retrieve only its contracts, relationships and necessary source.
3. Plan and validate a structural change.
4. Export a patch, apply it, then undo and redo it.
5. Introduce one Lean property and detect subsequent drift.
6. Migrate one supported framework feature with explicit remaining work.

Record context use, success, refusals, manual corrections, latency and verification coverage.
Use pinned real repositories alongside adversarial fixtures.
Do not mark a milestone complete from a capability predicate or a clean syntax tree alone.

## Decisions and open choices

LSP delegation remains excluded from the default engine.
The earlier measurement found a limited rename benefit at the cost of server lifecycle and project setup.
Reconsider it only against agent tasks that demonstrate a need.

Daemon/watch mode remains deferred until cache and latency measurements justify it.
The framework pair for the first frontend migration remains open.
Git history stores the old measurements; the active roadmap states current commitments.

## Further reading

- [CLI.md](CLI.md): implemented commands and write guarantees.
- [RECIPES.md](RECIPES.md): selection, operations and expectations.
- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md): current references and translation boundaries.
- [API_CONTRACTS.md](API_CONTRACTS.md): route conversion and HTTP contracts.
- [IR.md](IR.md): the existing code representation.
- [docs/lean-specs.md](docs/lean-specs.md): implemented checks and the adoption workflow.
