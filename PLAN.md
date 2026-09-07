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
| Defects fixed | 682 |
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
- Declared project checks with reviewed configuration digests, command outcomes and bounded output.
- Four passing agent trials on a pinned Rust release, paired context measurements and replayable patches with independent behavioral oracles.
- Lean edit, position, history, patch, pagination, confidence and workspace membership models, source anchors, signature maps and `spec check`, `sync` and `verify`.

Important gaps:

- `fr project` adds bounded maps, revision-bound details, call relationships and Cargo/npm manifest views. Complete dependency resolution and framework semantics remain pending.
- Native changes now have persistent history and checked undo/redo. History retention and large-journal scaling need further work.
- Browser undo restores the loaded workspace; it does not reverse individual transactions.
- Native history exports Git text patches and checks receiving files and indexes. Git status, repository change, diff, declaration and snapshot-local call pages exist. Raw staging and journaled index undo/redo are available on Unix. Reviewed commits are available with explicit index and HEAD bases. Worktree inspection reports registered workspaces with revision-bound pages. Reviewed creation adds raw checkouts on new or unused existing branches on Unix. Ownership receipts support checked completion of incomplete worktrees and reviewed removal of clean worktrees. Removal archives support inspection, checked resumption and reviewed compaction to audit summaries. Shared browser patch semantics remain pending.
- Strict spec signature maps currently accept Rust source declarations only.
- Framework readers cover selected patterns. Whole applications still need dependency and runtime work.
- The portable agent skill covers exploration, changes, declared checks, history, Git and Lean. Rust, TypeScript and TSX body replacement uses project handles and source history. Direct TypeScript/TSX function bindings support block bodies. Rust function declaration replacement can change signatures and implementations together. Rust function insertion appends through file handles. Broader agent evaluation, context reduction, wrapped initializers and nested declaration insertion remain pending.
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
- Extend the M4h paired agent evaluation to additional repositories and larger tasks with a pinned tokenizer and correctness checks.
- Reduce the observed inspection overhead, then measure context and task success again, including additional calls and uncertainty.
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

M3d: bounded repository status pages (complete).

- Add `fr git status` with JSON pages and staged, unstaged, untracked and conflicted filters.
- Report whole-repository counts, sorted paths, rename sources and raw Git status codes without source bodies.
- Bind continuation cursors to repository identity, query kind and observed status records.
- Keep conflicts separate from ordinary staged/unstaged counts and document omitted submodule state.
- Share the guarded Git runner with patch checks, refusing content filters on every tracked path.
- Inspect nested directories and linked worktrees independently of transaction history loading.

See [Git status scope and cursors](docs/git-status.md).
Status revisions identify Git observations, not complete working-file contents or a safe edit basis.
Page sizing reuses the anchored pagination helper. Git execution and parser correspondence remain unproved.
Three parser tests and five Git CLI scenarios cover pagination, unusual paths, conflicts, filters, submodule scope and preservation of index bytes.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 25 Lean build jobs.
Strict Lean verification retains seven fresh anchors and zero `sorry` obligations.

M3e: patch basis and executable-mode models (complete).

- Extract the mode projection, supported-mode-change predicate and patch-basis comparison into pure Rust helpers used by export and receiving checks.
- Anchor each helper to a Lean model with explicit signatures and 32-bit mode values.
- Prove the two projected modes, owner-execute equivalence, idempotence, supported-change characterization and reversal symmetry.
- Prove basis reflexivity, symmetry, transitivity, existence preservation and exact content/owner-execute requirements.
- Show full snapshot equality implies patch-basis acceptance and prove that the converse fails for other permission bits.
- Compare 45,419 mode results and 1,681 snapshot pairs between Rust and Lean.

The module contains 14 theorems. Its bitwise proofs use Lean 4.28's compiled bitvector checker.
The axiom audit includes `Lean.ofReduceBool` and `Lean.trustCompiler`; [the verification guide](docs/lean-specs.md) records the complete assumptions.
Model proofs and finite execution comparisons do not establish general Rust correspondence, filesystem correctness or Git behavior.
The standalone basis predicate accepts arbitrary strings; export still refuses binary snapshots before checking a receiving workspace.
Validation passes the full native/WASM gate, all Git and patch CLI regressions, 311/311 capability coverage and all 27 Lean build jobs.
Strict Lean verification passes with ten fresh source anchors and signature maps and zero `sorry` obligations.

M3f: recorded file deletion and owner-execute authoring (complete).

- Add `fr file delete PATH...` and `fr file executable PATH... --set on|off` with compact JSON previews.
- Record explicit regular text-file operations through `--save-plan` or `--write`, sharing the existing durable transaction engine.
- Validate complete snapshots again under locks, skip unchanged modes and preserve every other permission bit.
- Support empty-file deletion, checked undo/redo, interrupted-write recovery and recorded forward/reverse patch export.
- Refuse unsafe, duplicate, missing, non-regular or binary targets before applying a batch.
- Anchor the owner-execute setter to six additional Lean laws and compare 8,258 results with Rust.

See [file transaction scope and guarantees](docs/file-transactions.md).
Five CLI scenarios cover previews, saved plans, writes, stale bases, refusals, no-ops and preservation of unrelated Git state.
Recovery tests cover handled failures and process exit after every write during apply, undo and redo for both operation kinds.
The `file-snapshots` label does not claim dependency, compilation or behavioral validation after deletion.
The new model retains the bitvector checker's compiler-trust assumptions. File planning and general Rust correspondence remain unproved.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 27 Lean build jobs.
Strict Lean verification passes with eleven fresh source anchors and signature maps and zero `sorry` obligations.

M3g: bounded file diff details (complete).

- Add `fr git diff PATH` for index-to-worktree, staged and commit-to-worktree comparisons.
- Page through hunk headers, line coordinates and capped UTF-8 excerpts with explicit truncation metadata.
- Bind continuation cursors to the complete observed diff, literal path, comparison and canonical repository root.
- Report binary, mode-only and empty-file changes through metadata; refuse unsupported paths and conflicts.
- Guard selected paths against content filters and disable external diff, text conversion and index refresh.
- Disable demand fetching of missing objects in the shared Git runner.

See [Git diff detail scope and cursors](docs/git-diff.md).
Three parser tests and seven CLI scenarios cover comparison bases, coordinates, excerpts, cursors, refusals, linked worktrees and Git execution guards.
Page sizing reuses the anchored pagination helper; parser and Git execution correspondence remain unproved.
Git conversion semantics and concurrent changes limit observation identity. Collection work and memory remain unbounded by the page size.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 27 Lean build jobs.
Strict verification passes with eleven fresh source anchors and signature maps and zero `sorry` obligations.

M3h: changed declarations on Git comparison sides (complete).

- Add `fr git diff PATH --symbols` for working, staged and commit-based comparisons.
- Page through declarations overlapping changed lines, with containing declarations, side identities and coverage counts, without source bodies.
- Read historical and staged blobs directly; require each parsed snapshot to match the observed blob identity without conversion.
- Report partial parses, unmapped lines, unsupported languages and metadata-only changes explicitly.
- Bind symbol cursors to the diff, tool version, declaration results and coverage; reject line-view cursors.
- Anchor the inclusive line-range predicate to six Lean laws and compare 1,728 boundary cases with Rust.

See [changed declarations and scope](docs/git-diff.md#changed-declarations).
Seven new CLI scenarios cover hierarchy, pagination, selected snapshots, gaps, conversions, source races, Unicode bounds and SHA-256 repositories.
The view uses extension-based language detection and strict span containment. It does not match declarations across sides or resolve callers.
The Lean laws establish range behavior; Git capture, hashing, extraction, hierarchy and general Rust correspondence remain unproved.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with twelve fresh source anchors and signature maps and zero `sorry` obligations.

M3i: repository-wide change discovery (complete).

- Add `fr git changes` for working, staged and commit-based comparisons, with sorted path pages and complete metadata counts.
- Join raw modes and object identities with numeric line counts, without collecting patch bodies.
- Preserve binary, empty-file, mode-only and symlink/type-change metadata, with explicit regular-file detail candidates.
- Bind cursors to comparison metadata and resolved commit identity; document content changes that metadata cannot detect.
- Guard the union of index and selected commit paths against content filters, and refuse unmerged index state.
- Hand discovered literal paths and pinned commit identities to the existing diff and changed-declaration views.

See [repository change scope and cursors](docs/git-changes.md).
Three parser tests and seven CLI scenarios cover joins, scopes, pagination, unusual paths, filters, conflicts, worktrees and SHA-256 detail handoff.
The command excludes untracked files and submodules. Collection work and memory remain unbounded by the page limit.
Page sizing reuses the anchored helper. Git execution, metadata correspondence and aggregation remain unproved.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with twelve fresh source anchors and signature maps and zero `sorry` obligations.

M3j: snapshot-local calls touching changed declarations (complete).

- Add `fr git diff PATH --calls` with incoming, outgoing and both-direction pages for working, staged and commit-based comparisons.
- Analyze each verified file snapshot independently, retaining confidence, dispatch origins, unresolved targets and coverage gaps, without source bodies.
- Include nested declarations and call sites under changed containers, and distinguish direct overlap from containment.
- Isolate source-dependent resolution in a worker using captured source, preserving the caller's active workspace.
- Bind call cursors to the diff, normalized direction, tool version, rows and coverage.
- Anchor the boolean selection predicate to six axiom-free Lean laws and compare all 16 inputs with Rust.

See [snapshot call scope and fields](docs/git-diff.md#calls-touching-changed-declarations).
Seven CLI scenarios cover directions, pagination, containers, selected snapshots, receiver inference, dispatch, gaps, Unicode limits and conversion refusal.
A library regression checks preservation of the caller's active workspace.
The view does not load neighboring files, match edges across sides or establish runtime impact.
The proofs cover supplied flags; source capture, extraction, graph construction, enum mapping and general Rust correspondence remain unproved.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with thirteen fresh source anchors and signature maps and zero `sorry` obligations.

M3k: explicit file context for staged calls (complete).

- Add repeatable `--include FILE` to `fr git diff PATH --calls --staged`, with at most 32 explicit context arguments.
- Capture HEAD/index blobs for included files, including unchanged files and paths absent from one side.
- Resolve caller, target and dispatch candidates across those files while selecting changed declarations in the focus file only.
- Report endpoint and site paths, context identities, partial parses and explicit cross-file coverage boundaries.
- Bind cursors to context blobs and modes, including body changes that leave call rows identical.
- Guard all selected paths against filters and unsupported inventories, and recheck index entries before returning a page.

See [staged context scope and consistency](docs/git-diff.md#explicit-file-context).
Nine new CLI scenarios cover resolution, snapshot sides, cursors, additions/deletions, unborn branches, literal paths, SHA-256, dispatch, refusals and index races.
The workspace-isolation regression now exercises both single-file and selected-file analysis.
Working source bytes and omitted dependencies remain outside these graphs. Observations do not freeze concurrent Git state.
The existing Lean selection and page laws apply; inventory interpretation, file containment and snapshot consistency remain outside their proofs.
Validation passes the full native/WASM gate, all 16 call CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with thirteen fresh source anchors and signature maps and zero `sorry` obligations.

M3l: working and commit-based call context (complete).

- Extend `--calls --include FILE` to default and `--since REV` comparisons, retaining staged blob-only analysis.
- Capture the selected index or commit before side and raw tracked working files after, without expanding omitted dependencies.
- Feed captured focus text to analysis and recheck selected content, existence, projected modes and index entries before returning pages.
- Bind cursors to raw working identities, including context body changes with identical call rows and modes ignored by Git configuration.
- Preserve raw context CRLF while requiring focus bytes to match the observed diff; report source bases and missing sides explicitly.
- Exclude working replacements absent from the index and retain refusals for symlinks, unsupported files and invalid source encodings.

See [explicit context and observation limits](docs/git-diff.md#explicit-file-context).
Eight CLI scenarios cover comparison bases, cursors, conversions, missing files, executable modes, partial sources, source races, linked worktrees and SHA-256 identities.
The existing selection, pagination and Git mode models apply. Inventory interpretation and snapshot consistency remain regression-tested, without general Rust correspondence proofs.
The checks do not freeze concurrent state or detect changes fully restored between observations.
Validation passes the full native/WASM gate, all 24 call CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with thirteen fresh source anchors and signature maps and zero `sorry` obligations.

M3m: explicit-path staging previews (complete).

- Add `fr git stage PATH...` for up to 32 literal paths, reporting raw proposed entries and action counts without source bodies.
- Bind a reusable basis token to selected index identities, working bytes and projected owner-executable modes.
- Recheck selected sources and index entries while allowing unrelated staged changes and conflicts.
- Share snapshot readers with call inspection, retaining filter, path, encoding and symlink guards.
- Support untracked additions, tracked updates/removals, unborn repositories, linked worktrees and SHA-256 identities.
- Establish the default preview contract without index writes, object writes or history records.

See [staging preview semantics and limits](docs/git-staging.md).
Eight CLI scenarios cover actions, no-write behavior, basis drift, raw conversion differences, ignored files, modes, conflicts and source/index races.
The existing mode model applies. Classification, snapshot coherence and general Rust correspondence remain outside its proofs.
Validation passes the full native/WASM gate, eight staging CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with thirteen fresh source anchors and signature maps and zero `sorry` obligations.

M3n: reviewed staging application (complete).

- Add `fr git stage PATH... --basis TOKEN --write` on Unix, preserving raw bytes and owner-executable modes.
- Own the worktree index lock before checking the basis; copy the current index and update only reviewed entries.
- Verify prepared entries and unrelated staged inventories, including conflict stages and assume-unchanged/skip-worktree flags.
- Recheck sources, live index bytes and lock ownership before syncing and atomically installing the prepared index.
- Preserve unrelated staging added after preview, support unborn and linked worktrees, and bypass Git hooks and inherited index redirection.
- Refuse split indexes, sparse checkouts and nonregular indexes; report directory-sync failures as applied with a durability warning.

See [staging application and recovery limits](docs/git-staging.md#applying-a-reviewed-proposal).
Ten new CLI scenarios cover application, preservation, raw modes, SHA-256, locks, hooks, preparation failures and source/index races.
The existing mode model applies. Index installation and preservation have regression evidence, without new correspondence or crash-consistency proofs.
Staging does not create a source-history transaction; source undo and redo do not reverse index application.
Validation passes the full native/WASM gate, all eighteen staging CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with thirteen fresh source anchors and signature maps and zero `sorry` obligations.

M3o: durable staging history and checked replay (complete).

- Journal changed staging writes beside each worktree index, retaining before/after raw blobs for replay after object pruning.
- Add bounded record inspection and basis-checked undo/redo previews, using stack order and preserving working files and unrelated staged entries.
- Sync pending records before installation and finalize after index directory sync; block further staging writes while recovery is pending.
- Recover the starting selected state from complete before/after observations, refusing mixtures and external changes.
- Report completed index writes truthfully when journal finalization fails, with transaction identity and recovery guidance.
- Refuse changed selected special flags until replay can restore them, while preserving unrelated flags and conflicts.
- Anchor transition acceptance in Lean and prove abstract undo/redo and unselected-entry preservation laws.

See [staging history and recovery](docs/git-stage-history.md).
Nine CLI scenarios cover stack order, stale bases, object pruning, binary restoration, linked journals, corruption and recovery before/after installation.
An additional executable comparison covers every transition-predicate input. Filesystem durability and full Rust correspondence remain outside these model proofs.
Validation passes the full native/WASM gate, all 27 staging CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with fourteen fresh source anchors and signature maps and zero `sorry` obligations.

M3p: reviewed index commits (complete).

- Add `fr git commit -m MESSAGE` with a complete index basis, reviewed branch/parent, identities, tree and bounded changed-path reporting.
- Prepare previews in temporary index/object storage without repository writes, retaining raw indexed binary and symlink blobs.
- Publish only with `--basis TOKEN --write`, preserving working files, index bytes and completed staging history.
- Let Git prepare and lock HEAD and its branch before the final basis checks and publication request.
- Distinguish confirmed publication from uncertain outcomes, retaining the candidate commit identity and inspection guidance.
- Refuse unsupported Git states; disable hooks, signing, replacement objects and inherited Git environment overrides.
- Anchor branch/parent acceptance in Lean and prove abstract index and unrelated-reference preservation laws.

See [reviewed commits and publication limits](docs/git-commit.md).
Twelve CLI scenarios cover no-write previews, initial and linked commits, raw blobs, stale bases, ref locks, index races and publication failures.
Shared Rust/Lean cases cover branch and parent changes. Git locking, object handling and complete Rust correspondence remain outside the model proofs.
Validation passes the full native/WASM gate, twelve commit CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with fifteen fresh source anchors and signature maps and zero `sorry` obligations.

M3q: registered worktree inspection (complete).

- Add `fr git worktree list` with bounded registration pages and shared repository identity.
- Identify primary and invoking worktrees, attached and detached HEADs, unborn branches, locks and pruning annotations.
- Preserve paths with newlines and cap optional reasons at UTF-8 boundaries.
- Bind cursors to all registration bytes, including omitted rows and truncated reason suffixes.
- Preserve source files, indexes and registrations without content inspection or filter execution.
- Reuse the anchored pagination kernel and document concurrent observation limits.

See [worktree inspection](docs/git-worktrees.md).
Eight CLI scenarios cover linked and bare-primary repositories, stale cursors, raw metadata and dirty-index preservation.
Three parser tests cover framing, contradictory records, UTF-8 handling and bounded reasons.
Validation passes the full native/WASM gate, all eight worktree CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with fifteen fresh source anchors and signature maps and zero `sorry` obligations.

M3r: reviewed raw worktree creation (complete).

- Add `fr git worktree create PATH --branch NAME` with a start commit and bounded file inventory.
- Bind previews to the destination parent, branch, commit, tree and complete worktree registrations.
- Require a fresh destination and new branch, then create a separate raw checkout and index on Unix.
- Preserve source files and index bytes; disable hooks, replacements and content filter execution.
- Retain registration locks and report partial or uncertain creation with explicit inspection guidance.
- Anchor checkout payload limits in Lean and prove abstract fresh-destination preservation laws.

See [reviewed worktree creation](docs/git-worktree-creation.md) for raw checkout semantics and limits.
Nine CLI scenarios include linked and SHA-256 repositories, stale bases, unsupported trees and six injected failure modes.
Shared Rust/Lean cases cover payload boundaries; Git and filesystem operations remain outside full correspondence proofs.
Validation passes the full native/WASM gate, all nine creation CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with sixteen fresh source anchors and signature maps and zero `sorry` obligations.

M3s: ownership receipts and checked worktree recovery (complete).

- Record pending ownership receipts after Git registration and finalize them after checkout checks.
- Add `fr git worktree recover PATH` with bounded missing paths, captured ownership and a reviewed checkout basis.
- Fill missing committed files while preserving matching files and index bytes; refuse changed, extra or unsupported content.
- Refuse completed receipts so recovery cannot reverse later intentional deletions.
- Prepare absent indexes privately, install without replacement and hold Git's index lock through completion.
- Coordinate receipt writers with owned locks and preserve replacement locks during cleanup.
- Anchor recovery acceptance in Lean and prove abstract preservation of existing files.

See [recorded worktree recovery](docs/git-worktree-recovery.md) for ownership scope and partial-failure limits.
Seven recovery CLI scenarios cover registered failures, preserved inodes, stale bases, unsupported states, ownership replacement and retryable failures.
Two lock tests cover exclusion, release and replacement preservation. Shared Rust/Lean cases cover all predicate inputs.
Validation passes the full native/WASM gate, all seven recovery CLI scenarios, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with seventeen fresh source anchors and signature maps and zero `sorry` obligations.

M3t: reviewed owned-worktree removal (complete).

- Add `fr git worktree remove PATH` with bounded previews and a basis covering ownership, contents and private metadata.
- Accept clean raw checkouts at the owned branch's current commit, including later commits; retain that branch.
- Hold receipt, index, HEAD and branch leases while archiving metadata and deleting only reviewed files.
- Refuse extra content, unknown private metadata, active operations and unsupported configuration.
- Preserve late content through checked unlinks and empty-directory removal, with explicit partial-failure records.
- Anchor the deletion guard in Lean and prove abstract preservation of unselected paths.

See [reviewed worktree removal](docs/git-worktree-removal.md) for supported states and manual recovery limits.
Eight CLI scenarios cover later commits, SHA-256, linked invocation, stale bases, refusal states and late-content failures.
Unit tests check branch lease exclusion and selective cleanup. Shared Rust/Lean cases cover all deletion-guard inputs.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with eighteen fresh source anchors and signature maps and zero `sorry` obligations.

M3u: removal inspection and checked resumption (complete).

- Add `fr git worktree resume-removal RECORD` with bounded remaining paths, missing paths and blockers.
- Validate archived inventories against retained commits and restrict deletion to owned checkout and private metadata paths.
- Resume partial checkout or metadata deletion and confirm removals whose directories are already absent.
- Refuse replacement paths, changed content, unsafe archives, stale bases and existing locks.
- Coordinate initial removal and resumption with an archive lease and retain the recorded branch through deletion.
- Anchor resume acceptance in Lean and prove idempotence of abstract selected removal.

See [removal resumption](docs/git-worktree-removal-resumption.md) for supported states, inspection reports and failure limits.
Eight CLI scenarios cover partial checkout and metadata deletion, absent roots, blockers, unsafe archives, late changes and linked SHA-256 invocation.
Shared Rust/Lean cases cover all sixteen resume-guard inputs.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with nineteen fresh source anchors and signature maps and zero `sorry` obligations.

M3v: reviewed checkout of existing local branches (complete).

- Add `create --existing-branch NAME` beside new-branch creation, with explicit branch actions in previews.
- Bind the selected mode, existing tip and worktree registrations into the creation basis.
- Hold a prepared branch verification lease through registration, raw checkout and receipt completion.
- Preserve refs, branch reflogs, upstream configuration and the invoking worktree's dirty state.
- Refuse missing, symbolic, ambiguous and occupied branches; never guess a remote branch or force checkout.
- Record the branch mode in receipts while accepting older receipts and removal archives.
- Anchor branch selection in Lean and prove abstract preservation of refs during attachment.

See [existing-branch checkout](docs/git-worktree-existing-branches.md) for scope, reference preservation and failure limits.
Eight CLI scenarios cover retained refs and configuration, dirty sources, stale bases, registration races, locks, recovery, packed SHA-256 refs and older records.
Shared Rust/Lean cases cover all eight branch-selection inputs.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with twenty fresh source anchors and signature maps and zero `sorry` obligations.

M3w: reviewed compaction of completed removal archives (complete).

- Add `fr git worktree compact-removal RECORD` with a review basis and a bounded audit preview.
- Require a completed removal, absent target roots and an owned archive lease before discarding recovery data.
- Save and synchronize a small audit summary before unlinking the exact reviewed full record.
- Retain the completion marker and unrelated archive files; preserve refs and the invoking worktree.
- Inspect compacted records through their original paths and resume interrupted compaction with a fresh review.
- Anchor the compaction guard in Lean and prove abstract audit preservation and idempotence.

See [archive compaction](docs/git-worktree-archive-compaction.md) for retained data, irreversible discard and failure limits.
Eight CLI scenarios cover audit inspection, blockers, stale bases, unsafe archives, interrupted publication, replacement locks and linked SHA-256 invocation.
Shared Rust/Lean cases cover all sixteen compaction-guard inputs.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with twenty-one fresh source anchors and signature maps and zero `sorry` obligations.

Next M3 work:

Extend raw file modes and per-worktree configuration support, and add bulk archive retention.
Extend recovery across failures before receipt publication and improve stale-lock and crash-state inspection.
Extend selected flag replay and add staging journal retention and compaction.
Keep Git optional for ordinary analysis and transaction history.
Undo an `fr` transaction without resetting unrelated Git changes.

Exit: patches apply to the expected basis and reproduce the recorded result.
Tests cover dirty repositories, staged changes, untracked files and conflicts.

### M4. Agent skills and bounded code authoring

M4a: portable agent handoff and executable examples (complete).

- Add `skills/fr` with a small entrypoint and separate exploration, change, history, Git and Lean references.
- Include the portable skill in native release archives without installing it globally.
- Execute every fenced command example against a selected binary in temporary source and Lean projects.
- Check stale handles and plans, recipe expectations, behavior, undo/redo, patch application and failing Lean claims.
- Keep failed JSON spec checks parseable as one report while retaining unsuccessful exit status (B840).
- Bound introductory context and measure fixture output bytes without claiming agent success or token savings.

See [agent skill validation](docs/agent-skill.md) for the 31 examples, measurements and evidence limits.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification retains twenty-one fresh source anchors and signature maps and zero `sorry` obligations.
The skill validator and local archive check pass. Real-agent evaluation and release-platform execution remain separate work.

M4b: bounded Rust function-body replacement (complete).

- Add `fr author replace-body HANDLE --from FILE` with current project handles and explicit short-ID revisions.
- Replace one complete Rust block, retaining signatures, attributes and all source bytes outside it.
- Require clean original and resulting parses, and cap both blocks and input files at 64 KiB.
- Return bounded JSON diffs with omission counts, source fingerprints and explicit syntax-only validation.
- Save exact replacement plans through existing source history, including apply, undo/redo, recovery and patches.
- Anchor the body-size guard in Lean and compare 64 boundary cases with Rust.

See [body authoring](docs/body-authoring.md) for supported targets and verification limits.
Eight CLI scenarios cover behavior, preserved source, history, stale identities, refusals, byte limits and permissions.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification passes with twenty-two fresh source anchors and signature maps and zero `sorry` obligations.

M4c: TypeScript and TSX function-body replacement (complete).

- Extend handle-based body replacement to named declarations, generators, methods, accessors and constructors.
- Parse fragments and resulting files with the selected language grammar, including JSX for TSX targets.
- Preserve surrounding bytes and reuse the existing size guard, saved plans, undo/redo and patch export.
- Refuse unrelated local selections without editing enclosing functions.
- Select brace-token spans to preserve external comments and semicolons across parser node boundaries.
- Check typed and JSX behavior with TypeScript compilation and Node execution.
- Refresh the portable skill and document parser and target-selection limits.

Fourteen authoring scenarios pass, including overloads, accessors, grammar refusals and compiled TSX behavior.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification retains twenty-two fresh source anchors and signature maps and zero `sorry` obligations.
The existing size and edit models apply unchanged; AST selection and parsing retain test-based evidence.

M4d: direct TypeScript and TSX function-binding bodies (complete).

- Select direct arrow, function-expression and generator initializers through variable or class-field handles.
- Require block bodies and preserve bindings, lexical receivers, signatures and neighboring source.
- Refuse wrapped initializers, expression bodies and unrelated local selections.
- Bound signature output to the selected binding, including declarations sharing one statement.
- Check shadowed selections, typed behavior, JSX, saved plans, undo/redo and patch export.
- Refresh agent instructions while retaining the existing Lean size and edit models.

Seventeen authoring scenarios pass, including compiled checks for lexical receivers, recursion, generators and JSX.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification retains twenty-two fresh source anchors and signature maps and zero `sorry` obligations.
AST selection retains test-based evidence; the command reports syntax validation without claiming type or behavioral verification.

M4e: bounded Rust function declaration replacement (complete).

- Replace one complete function through its project handle, retaining its name and outer attributes.
- Allow combined signature and implementation changes without changing callers or imports.
- Require exactly one function fragment, clean destination syntax and complete declarations within 64 KiB.
- Reuse bounded previews, source revisions, saved plans, undo/redo and patches.
- Prove suffix preservation in the Lean edit model alongside the existing prefix theorem.
- Cover compiled signature changes, name and attribute refusals, nested contexts, size limits and stale handles.

Twenty-four authoring scenarios pass, including a compiler rejection that remains separate from syntax acceptance.
A declaration edit reported by the CLI produces matching results in Rust and the Lean edit model.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification retains twenty-two fresh source anchors and signature maps and zero `sorry` obligations.
Prefix and suffix preservation have model proofs; AST selection, parsing and filesystem operations retain test-based evidence.

M4f: bounded Rust function insertion (complete).

- Append one complete function through a Rust file handle while retaining every existing source byte.
- Report EOF placement, fragment size, newline separators and full inserted spans separately.
- Refuse duplicate direct names and pending outer documentation or attributes; disclose incomplete name resolution.
- Reuse syntax checks, bounded previews, source revisions, saved plans, undo/redo and patches.
- Prove recovery of original model text after removing an insertion and compare a reported insertion with Lean.
- Cover empty files, CRLF, comments, name normalization, fragment limits, refusals and compiled behavior.

Twenty-nine authoring scenarios pass, including saved insertion identity, compiled execution, undo/redo and patch export.
Reported insertion and replacement edits produce matching results in Rust and Lean.
Validation passes the full native/WASM gate, 311/311 capability coverage and all 29 Lean build jobs.
Strict verification retains twenty-two fresh source anchors and signature maps and zero `sorry` obligations.
The insertion-recovery theorem covers the edit model; full name resolution, parsing and filesystem behavior remain outside that proof.

M4g: declared project checks (complete).

`fr checks` previews `.fr/checks.json` and executes selected names against a reviewed configuration digest.
Reports separate declared coverage, process outcomes, omitted output and checks that did not run.
Focused tests cover selection, stale configuration, failures, timeouts, output bounds and path refusals.
The portable skill now exercises 33 examples. Its validator and the full native/WASM gate pass.
Validation retains 311/311 capability coverage and all 29 Lean build jobs.
Process execution and declared coverage have test evidence; they do not carry formal correctness claims.

M4h: measured real-agent acceptance (complete).

Two tasks on the pinned strsim 0.11.1 release pair fresh agents using fr and ordinary file tools.
The harness records visible context, calls, timing, refusals and exact source transitions.
Independent oracles check Unicode similarity and normalized OSA behavior, including clean patch receivers.
Trials include declared checks, patch export, undo/redo and preservation of an unrelated edit.
All four valid trials pass upstream tests and independent oracles without tool refusals or human task corrections.
The fr trials consume 18,628 and 17,370 retrieved-context tokens; ordinary-file trials consume 9,469 and 10,393.
These small-project results establish working agent transactions and identify context overhead for the next optimization.
Retained prompts, transcripts, patches and token audits support reproducible behavioral replay.
Ten harness regressions and replay of all four patches pass. The full native/WASM gate and exact token audit pass.
Validation retains 311/311 capability coverage, 29 Lean build jobs and twenty-two fresh source anchors with zero `sorry` obligations.
See [real-agent acceptance](docs/agent-acceptance.md) for provenance, pilot failures, measurement scope and limits.

M4i: measured context reduction (complete).

Targeted declaration lookup, quiet successful checks and a more selective skill handoff reduce retrieved output.
`project find` returns paged handles and optional headers with exact or literal substring matching and subtree selection.
`checks --quiet-success` preserves failure diagnostics, outcomes and omission counts while dropping successful stream text.
Skill references separate authoring from recipes and patch delivery from Git administration.
The full native/WASM gate, 127 project CLI scenarios, ten check scenarios and all 33 skill examples pass.
Strict verification retains twenty-two fresh source anchors and signature maps, zero `sorry` obligations and 29 Lean build jobs.
Four fresh paired trials exercise implementation commit `0498c6b`, all passing without human corrections or tool failures.
The fr trials use 12,287 and 13,109 retrieved-context tokens, 34.0% and 24.5% below the first fr trials.
Fresh ordinary-file trials use 6,087 and 8,397 tokens; fr still costs more context on these small tasks.
Both evidence bundles are retained, with eight patches covered by behavioral replay and exact follow-up token auditing.
See the [context-reduction follow-up](docs/agent-context-followup.md) for comparisons, protocol changes and measurement limits.

M4j: smaller history completion reports (complete).

Retained trials repeat diff text in apply, undo and redo completion reports after agents already reviewed the changes.
Opt-in `--no-diff` on history write transitions retains outcomes, file existence and modes while omitting repeated diff text.
Previews, stored snapshots, patch exports and all transition checks retain their existing behavior.
The portable skill uses this option after review and clarifies exact-name versus partial-name lookup.
Focused history and skill tests pass, including 33 executable examples and interrupted recovery.
Controlled replay of the two retained edits reduces completion-output tokens by 69.8% and 59.1% with identical source and patch results.
Including undo/redo previews, history-output reductions are 41.9% and 35.5%. These measurements do not include total task context or fresh agents.
The full native/WASM gate passes, including eight history CLI scenarios, eighteen history unit scenarios and all 33 skill examples.
Validation retains 311/311 capability coverage, 29 Lean build jobs and twenty-two fresh anchors and signature maps with zero `sorry` obligations.
The [follow-up report](docs/agent-context-followup.md#subsequent-history-completion-reports) retains measurements and distinguishes them from paired-agent results.

Next M4 work:

Extend paired real-agent evaluation to larger projects and tasks spanning package boundaries.
Repeat trials with the current lookup, skill and output options before choosing further inspection or transaction-report changes.
Keep examples compatible with the distributed binary and load specialized references only when needed.

Extend body authoring to selected wrapped initializers or additional languages, and support nested insertion or further declaration kinds.
Reuse revision checks, edit planning, syntax validation and history.
Compose high-level intentions as inspectable recipe steps with explicit postconditions.
Extend declared checks only where real task evidence requires additional selection or execution support.

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
