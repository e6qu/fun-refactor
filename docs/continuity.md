# Development continuity

PR 0, the agent-ready verified refactoring foundation, merged as GitHub PR 259.
PR 1, Agent Context Protocol v2, merged as GitHub PR 261.
PR 2, Generalized Structural Authoring, merged as GitHub PR 262.
PR 3, Durable Git Workspace Lifecycle, merged as GitHub PR 263.
PR 4, the Lean Adoption Kit, merged as GitHub PR 264.
PR 5, Framework Semantic Model, merged as GitHub PR 265.
PR 6, Verified Feature Migration, merged as GitHub PR 266.
Release PR 260 then published the completed roadmap state from `main`.
PR 7, Context-Competitive Agent Workflow, merged as GitHub PR 267.
PR 8, Agent Workflow Simplification, merged as GitHub PR 269.
The current `project_context_v5` branch is roadmap PR 9, Bounded Project Query Batches.
Its first checkpoint adds `fr project batch --from MANIFEST`: up to sixteen existing read-only
project queries share one constructed snapshot and final workspace verification. The outer response
retains revision, handle prefix, coverage and context basis once. Each nested report omits those
common fields. The versioned manifest and ordered resolved request set carry separate digests.
A shared serialized-report budget admits or omits only complete nested reports. An omitted request
retains its ID, query kind, request digest and required byte count, and a later smaller report can
still use the remaining budget. Request counts, IDs, argument counts and bytes are bounded;
unknown command shapes and recursive batches refuse.

`FrKernels.Project.batchSectionFits` models the overflow-safe admission predicate. Three theorems
characterize acceptance, preservation of the total budget and rejection after exhaustion.
All 1,728 combinations over representative machine-sized values agree with Rust on a 64-bit host.
CLI regressions reconstruct standalone map, select and package reports exactly from the common
batch envelope and cover whole-report omission and adversarial manifests.

The second checkpoint lets a request argument use an RFC 6901 JSON Pointer into an earlier nested
report. References are backward-only and must resolve to a string, so a lookup can feed its exact
revision-bound handle into `show`, `calls` or another existing query without an agent round trip.
The resolved arguments remain subject to the per-request byte bound and join the project revision
and manifest identity in `resolution_basis`. Reports remain available for internal references when the output budget omits
them; their facts do not otherwise leak. Regressions cover omitted producers, missing pointers,
forward references and non-string results.

The third checkpoint adds the versioned manifest to the portable Explore skill. Its broad route
combines map, exact lookup, referenced source inspection, incoming calls, test candidates and gaps.
The skill checker creates the documented artifact, runs the batch through the built binary, checks
all request outcomes and enforces the 256-byte nested source limit. The portable bundle now has 43
executable shell examples. CLI and Lean documentation state the reconstruction and proof boundaries.

The historical PR 8 work follows.
It follows the fresh passing PR 7 trace: seven failed or refused requests, repeated symbol
inspection and ambiguous artifact references account for the first concrete reductions.
Its first checkpoint makes `project select` accept exact names and full revision-bound handles.
The selection policy distinguishes outside-scope, non-declaration, omitted-local and returned
declaration states and is modeled in Lean with exhaustive Rust correspondence.
The remaining checkpoints cover compact authoring discovery, harness boundaries, skill routes,
a bounded counterfactual measurement and a fresh Luna-low pair.
`fr author guide` supplies the second checkpoint without requiring a project scan. Its bounded
JSON names every operation and field, exact limits and the preview/save/apply/patch/undo/redo
templates. The portable author route points integrations to that schema and tells an agent that
already loaded the route not to spend additional subcommand-help calls.
The third checkpoint fixes two acceptance-boundary defects from the retained trace. Coordinated
manifest validation now passes `-h` and `--help` through to the CLI. Artifact writes return an
explicit canonical `fr_reference`; the prompt requires copying it verbatim into fragment and
manifest fields, and invalid relative paths name that contract in their refusal.
The fourth checkpoint keeps every portable route within its byte ceiling and all 42 shell
examples executable. After the first fresh diagnostic, the targeted five-file route is 1,825
tokens. Its added guidance distinguishes coordinated lookup and plan bases from project bases.
The fifth checkpoint publishes [the v4 prescribed workflow](agent-workflow-v4-evaluation.md).
It checks the immutable accepted trace and exercises the current handle selection on the pinned
workspace. The prescribed sequence retains every mutation and verification step while reducing
42 calls to 29 and measured context from 15,458 to 13,278 tokens. This 2,180-token reduction is
a one-trace counterfactual, not autonomous adoption or a population claim.

The first fresh PR 8 pair is retained as a diagnostic. Both arms made correct changes, passed
the project and receiver oracles, preserved indexes and reversed exactly. Both skipped the
original-state check before editing, so the ordered workflow rejected them. The `fr` arm used
23,973 context tokens and 49 calls, including 12 refused or failed requests. The files arm used
13,852 tokens and 21 calls. The harness now refuses source mutation before original checks pass;
skill and prompt guidance also cover the exact command errors in the trace.

The second fresh PR 8 pair passes all acceptance gates and exact replay. The `fr` arm uses
13,949 context tokens and 30 calls; ordinary files use 12,815 and 23. This leaves a 1,134-token
or 8.8% `fr` premium in one sample, down from the PR 7 pair's 33.3%. The `fr` arm uses fewer
inspection tokens; authoring, delivery payloads and tool latency remain larger. Its agent uses
the author guide, exact artifact references, one final manifest, the right plan basis, compact
history and a single saved batch without correction.

The first hosted PR 8 check exposed a checkout-path dependency in the counterfactual prompt.
It also exposed variable tokenization of opaque live hashes. The projection now reuses the
frozen tool path and fixed high-entropy identities after it validates the real live response.

PR 7 targeted the fixed-projection gap through multi-symbol inspection, review-bound payload
omission, a smaller task-routed skill and a fresh paired acceptance cohort.
The retained fresh cohort passes independent project and receiver oracles, ordered checks, exact
reversal and index preservation. Its single-pair comparison remains narrower than a population claim.
PR 7 checkpoint 2 adds `project select NAME...`: one bounded traversal retrieves up to
32 exact declaration names under a shared revision, coverage envelope, page and source
budget. Per-request statuses distinguish matches, omitted locals and indexed absence;
cursors bind the full ordered request and context-basis reconstruction remains exact.
PR 7 checkpoint 3 adds `frpb1` reviewed-plan bases to authoring and feature migration.
They bind complete plan reports and exact source payloads, compact only equal top-level fields
and refuse clipped, stale or conflicting plans before persistence. Transaction context moves to
`frtb2`, which binds complete before and after snapshots while leaving journal compatibility intact.
PR 7 checkpoint 4 reduces the targeted authoring skill route from 9,937 to 6,074 raw bytes
and the complete portable bundle from 24,691 to 20,828 bytes. The checker now caps the
entrypoint, every reference and seven task routes; all 42 shell examples still execute. The
pinned tokenizer and retained read framing measure the targeted route at 1,693 tokens instead
of 2,699, a 37.3% reduction from this PR's starting revision.
PR 7 checkpoint 5 publishes a v3 fixed projection bound to the immutable passing cohort and
frozen v2 report. An exact path allowlist audits every changed request and response. It measures
a 10,960-token `fr` mean, 269 below v2, while marking multi-select and plan compaction as
inapplicable to those transcripts rather than changing their calls.
The first fresh PR 7 pair is retained as a failed diagnostic. The `fr` arm stopped after using
undocumented batch operation spellings; the files arm passed both behavior oracles but omitted
the original-state check. The author route now names every operation, and harness refusals expose
the received kinds and exact expected postconditions.
PR 7 checkpoint 6 retains a second fresh pair under the same task and Luna-low configuration.
Both arms pass all acceptance gates without corrections or restarts. The `fr` arm uses 15,458
measured context tokens and 42 calls; the files arm uses 11,600 tokens and 20 calls, leaving a
3,858-token or 33.3% `fr` premium in this sample. The `fr` agent adopted reviewed-plan compaction
and `frtb2` delivery. The evaluator now reconstructs an omitted `files_changed` count only from a
unique earlier batch preview with the same plan basis; missing and conflicting evidence refuse.
The cohort passes exact token audit and complete patch replay.
The final PR 7 native/WASM gate passes, including 408 library tests, 152 project scenarios,
311/311 capability coverage and 42 Lean jobs. Strict specification verification reports fresh
source anchors, zero obligations and a successful Lean package build.
PR 6's first checkpoint adds `fr migrate feature` for one revision-bound Next.js App Router or FastAPI route feature whose methods share one source file.
The planner refuses stale feature IDs, same-framework requests, escaping destinations and any disagreement between semantic and translated endpoint sets.
It retains the source route, adds the destination and classifies feature facts and gaps as automatic, agent-decision or unsupported work.
Validated Next.js placement and explicit FastAPI application wiring can register destinations automatically; explicit eligible cutover can remove the source in that transaction.
Preview, saved plans and writes use the existing source-history transaction, so patch checks, export, apply, undo and redo remain available.
Four CLI integration cases cover both migration directions, refusal boundaries and exact forward undo/redo replay.
Nine migration policies constrain supported directions, disposition classes, schema inclusion, registration, body promotion, body validation, cutover and dependency edits.
Thirty-nine framework theorems now cover seventeen helpers, with 364 shared Rust and Lean results.
The checkpoints have strict syntax, endpoint, schema, registration, dependency and project-check evidence; full framework behavior remains outside the bounded claim.
The second PR 6 checkpoint adds generic runtime oracles for both migration directions.
A telemetry handler runs through Node before migration and through generated Python afterward.
A parameterized metrics handler runs through Python before migration and through generated Node afterward.
Each oracle compares the exact status and JSON body.
The harness uses minimal registration stubs, so it establishes handler behavior without claiming framework middleware, dependency injection, startup or deployment behavior.
The third checkpoint canonicalizes translated model names, fields and supported types, reparses each generated destination and requires every source shape in that independent result.
FastAPI to Next.js compares only models reached by handler signatures; unrelated classes do not enter the migration contract.
Framework writers now retain declared model field spellings in both directions, fixing JSON key changes such as `device_id` becoming `deviceId`.
Wire aliases, validators, defaults, serialization settings and runtime validation remain unverified and visible as gaps.
Lean proves the schema inclusion policy, and shared execution checks empty, reordered, duplicate, partial and complete shape lists against Rust.
The fourth checkpoint applies an exported migration patch in a separate clean Git repository.
The receiver checks, applies, reverses and reapplies the patch, comparing the retained source and generated destination exactly.
That generic operations-route fixture exposed a route-binding bug: a Next.js `[operationId]` segment became `{operation_id}` while its source context still used `operationId`.
Framework migration now preserves dynamic parameter spelling in both the FastAPI route and handler signature, and generated documentation records the corrected contract.
The fifth checkpoint recognizes destination registration when a captured Next.js dependency owns an `app` or `src/app` output path.
The migration report binds that target application and classifies route placement as automatic; unknown targets and FastAPI router composition remain agent decisions.
Lean proves that automatic registration requires both pieces of evidence, and shared execution checks every Boolean combination against Rust.
The sixth checkpoint executes generic request payloads through source and generated handlers in both directions.
Mixed-spelling keys, strings, numbers and arrays retain their exact JSON values.
The fixture exposed that a typed Next.js `request.json()` binding became an annotated Python dictionary instead of a Pydantic model.
Generated FastAPI handlers now call `model_validate` for that bounded pattern before accessing declared fields.
It also exposed that Python `int` and TypeScript `number` need one language-neutral declaration family, so integer and float declarations now canonicalize as `number` for cross-language schema comparison.
The runners use minimal framework and Pydantic stubs; installed-framework validation, middleware, dependency injection and lifecycle behavior remain open.
The seventh checkpoint promotes one direct typed Next.js JSON binding to a native FastAPI body parameter when no path or query name collides.
Pinned FastAPI 0.141.1 mounts the generated router through `include_router` and handles requests through its ASGI application.
Pydantic 2.13.5 and Starlette 1.6.0 preserve the valid body and produce a field-specific 422 response for an invalid array item.
The promotion policy requires one candidate and no collision.
Three Lean theorems characterize that rule, and sixteen shared cases cover unique, missing, repeated and colliding candidates.
The eighth checkpoint installs a lockfile-pinned Next.js 16.3.4 application with React 19.3.0 and TypeScript 5.9.3.
App Router placement registers the generated generic events route.
A real framework request returns the exact FastAPI source payload.
The runtime fixture uses no example-specific translation rule.
The ninth checkpoint generates structural Next.js request validation from the intermediate type model.
Exactly one direct local body model is automatic when every reachable field uses a supported primitive, optional, list, string-keyed map, tuple or acyclic local-record shape within the depth bound.
The translator now emits local models reached transitively through record fields, so nested declarations and their validators remain complete without fixture-specific rules.
Ambiguous and unsupported shapes stay visible as fidelity notes.
The real FastAPI source and generated Next.js route both accept the generic valid event and reject its invalid array element with status 422 at the same body-field location.
The eligibility rule is anchored in Lean with three theorems and eight shared Rust/Lean cases.
Pydantic coercion, aliases, custom validators, constraints, strict and extra-field settings, connected project edits and cutover remain open.
The tenth checkpoint lets an explicit `--register-with PATH::APP_SYMBOL` target join a FastAPI migration transaction.
The command requires a captured, cleanly parsed Python file, one recognized FastAPI application binding, a valid dotted import path and no direct endpoint conflict.
It generates a collision-free router alias, adds the import and `include_router` call, reparses the combined edit and records the application file beside the generated route for exact undo and redo.
A pinned FastAPI fixture imports the edited application and serves the generic selected route through ASGI.
The eligibility policy has three Lean theorems and eight shared Rust/Lean cases.
Included or mounted routers, overlapping dynamic paths, middleware order and test configuration remain reviewed boundaries.
The eleventh checkpoint adds explicit `--cutover` source removal to the migration transaction.
It requires automatic destination registration and refuses a resolved reference from another source file to any indexed source symbol.
The history layer records source absence beside generated and connected-file edits, so patch export, undo and redo preserve existence, bytes and modes.
The pinned FastAPI fixture serves the generated route through the edited application after the source route disappears.
Three Lean theorems characterize cutover eligibility, and all eight Boolean cases agree with Rust.
Runtime imports, string paths, deployment routing, external callers and project-specific checks remain reviewed evidence outside the eligibility proof.
The twelfth checkpoint adds source-bound declared-check receipts to applied history transactions.
`fr checks --record-for <TX>` verifies the applied transaction before execution, hashes recognized source before and after every selected command, rereads the check configuration and revalidates under the history lock before recording.
Each `frce1:` receipt binds the reviewed configuration digest, stable source revision and selected check names; repeated recording is idempotent and `history show` retains the row through undo and redo.
Source or configuration drift fails the report, invalid transaction state refuses before project code runs and corrupted receipt data invalidates the journal.
Four Lean theorems characterize receipt acceptance, and all sixteen Boolean cases agree with Rust.
The boundary snapshots do not observe command-internal mutate-and-restore behavior, unsupported files, executable identities, dependencies, services, environment values or external state.
The thirteenth checkpoint adds explicit PEP 621 dependency edits for generated FastAPI imports.
The selected `pyproject.toml` must own the destination and contain one supported `[project].dependencies` array.
Caller-supplied requirement strings must cover each missing `fastapi` or `pydantic` import once, without extras or duplicate existing distributions.
The command preserves existing requirements, reparses TOML and records the manifest beside generated and registration files for exact patch, undo and redo behavior.
Next.js dependency evidence reuses the captured manifest required for automatic route registration.
Three Lean theorems characterize the four dependency-edit boundaries, and all sixteen Boolean cases agree with Rust.
Requirement semantics, package-manager resolution and installation remain reviewed work.
The fourteenth checkpoint binds project-owned test commands to the migration transaction.
Repeated `--check` options select names from `.fr/checks.json`, canonicalize them to declaration order and retain the exact configuration digest and names in history.
Evidence for that transaction must use the same configuration and selection; a mismatch refuses before any project command starts.
Three Lean theorems characterize the optional exact-selection rule, and all eight Boolean cases agree with Rust.
The mechanism is runner-agnostic and does not invent fixtures or behavioral assertions for a project.
`FrKernels.Checks` now has seven theorems and 24 shared Boolean results across receipt acceptance and required-selection policy.
The full native/WASM gate passes with 407 library tests, 150 project scenarios, 16 migration scenarios, six runtime scenarios, 311/311 capability coverage and 42 Lean build jobs.
Strict source-anchor and signature verification also passes with zero obligations.
The scheduled deep self-translation audit exposed four generic draft-validity defects while PR 6
was open. TypeScript templates now escape decoded control characters and delimiter sequences.
Java qualified types escape reserved path segments, Rust characters render as `char` or boxed
`Character`, and custom output stems produce legal wrapper class names. Focused translation
regressions and the complete Rust-source round-trip audit pass after these fixes.
The pull-request WASM and playground jobs then exposed a feature-boundary error in the same
framework writers. Their pure policy helpers were reachable only through the CLI-gated project
module, although browser translation uses them without CLI support. The kernel is now exported
from the common library boundary and re-exported through its established project path. The full
WASM check, including the no-default-feature configuration used by the browser build, passes.
Roadmap PR 5's first checkpoint added `fr project features` for bounded Next.js App Router and FastAPI hierarchies.
Applications contain exact-route-path feature candidates, which contain routes, handlers, contract fields, schema references and expanded same-file schema candidates.
Every fact carries a parent, source anchor, status, confidence, validation basis and explicit gaps.
Feature IDs retrieve one revision-bound subtree; cursors bind that selection.
Duplicate type declarations remain ambiguous and other route frameworks produce gap facts.
The second PR 5 checkpoint attaches a captured npm package to each supported Next.js application.
It adds bounded npm build scripts and dependency facts beneath that package.
Existing manifest-link evidence distinguishes linked local packages, unresolved local links and dependencies that still need package-manager resolution.
FastAPI retains an explicit Python-packaging gap.
Per-application limits cap build settings at 64 and dependencies at 256; omission facts carry either overflow.
The third checkpoint adds application middleware facts for Next.js `proxy` and legacy `middleware` convention files, plus direct FastAPI HTTP decorators and `add_middleware` calls.
FastAPI declaration order and reverse request order remain syntax evidence; runtime registration and behavior remain unchecked.
Middleware output is capped at 64 facts per application with an explicit omission gap.
FastAPI `Depends` and `Security` parameter markers now appear as route dependencies in contract output and as `execution-dependency` children in feature output.
Direct callable providers are name-only candidates; competing markers and computed providers stay unresolved.
Only `Security` is labeled as an authentication candidate, without claiming that authorization succeeds at runtime.
Unsupported middleware, authentication and lifecycle forms remain visible gaps. Runtime behavior, broad service reachability and frontend semantics remain open.
The fourth checkpoint recognizes FastAPI constructor and route dependency lists, preserving application, route and parameter scope.
It adds FastAPI lifespan and deprecated event hooks, with explicit conflict diagnostics.
Next.js instrumentation files contribute direct `register` and `onRequestError` exports; re-exports and competing files stay gaps.
Application configuration facts reuse captured environment declaration and accessor chains, omit values and distinguish unmatched reads.
Next.js `NEXT_PUBLIC_` variables carry a client build-time candidate marker; runtime substitution and client inclusion remain unchecked.
Handler inspection adds sanitized HTTP service candidates for fetch, axios, requests and HTTPX syntax.
It strips query strings, fragments and URL credentials, hides dynamic targets and reports them as gaps.
The fifth checkpoint adds Next.js page features even when an application has no API route.
Direct React function components retain server-default or `use client` placement, props, state hooks, effect schedules, events, style shapes and render edges.
The reader exposes no prop values, hook initializers, effect bodies, event bodies or class values.
Malformed page paths and stateful server-default components remain explicit gaps or conflicts.
Limits cap components at 128 and child details at 512 per query.
The sixth checkpoint extracts seven framework policy helpers used by production reporting and anchors them in `FrKernels.Project`.
Thirteen theorems cover cap partitioning, reverse middleware order, component hook placement, configuration visibility, service target tiers and redaction flags.
All 263 bounded Rust/Lean results agree, strict source and signature verification passes with zero obligations, and the axiom audit records only standard Lean axioms for the arithmetic and hook proofs; the finite classifiers use none.
Syntax recognition, framework runtime meaning and report assembly remain fixture-tested boundaries.
The seventh checkpoint attaches inherited Next.js layouts and recursively follows direct relative component imports within the captured package.
Cycles terminate through a bounded file set; missing, ambiguous, package-crossing and overflow cases remain explicit gaps.
Render edges resolve unique same-file, default-import and named-import declarations to source anchors.
Static reachability below a `use client` entry marks imported files as client-transitive candidates.
State and effect conflicts use that effective boundary.
The traversal reuses the workspace membership step whose closure and convergence have Lean proofs and shared Rust/Lean cases.
Import-edge construction remains covered by component fixtures.
Direct custom-hook calls retain names while their implementations and runtime needs remain unchecked.
Competing page and layout convention files produce ambiguity gaps.
The eighth checkpoint pins the framework syntax witnesses and compares feature reports with independent route contracts over unmodified Next.js and FastAPI project files.
It adds a real five-operation `APIRouter` source file and joins valid constructor prefixes into reported paths.
Dynamic and runtime-composed prefixes remain explicit gaps.
The verified prefix predicate raises the framework kernel to eight anchors, sixteen theorems and 271 shared Rust and Lean cases.
The framework model makes source-level candidate claims; any future runtime claim requires an executable framework fixture.
Roadmap PR 4's first checkpoint added `fr spec init [PATH]` with a pinned toolchain, a minimal checked Lake target and refusal to overwrite differing files or escape through paths and symlinks.
Initialization previews by default, supports saved plans and writes through source history. Apply, undo and redo cover files created below a new directory, while plan recording no longer creates the target directory as a lock side effect.
A real integration case initializes an external temporary workspace and passes `fr spec verify specs` against Lake.
The second PR 4 checkpoint adds `fr spec scaffold SOURCE::SYMBOL` for a bounded Rust type subset. It writes a full anchor, strict signature map, checked module import and one visible handwritten proof obligation in a single source transaction.
The integration case fills that obligation with a Boolean model and theorem. Strict correspondence and Lake then pass; changing the theorem to a false property makes the same verification command fail.
The third PR 4 checkpoint makes the same scaffold command a regeneration path. It validates unique ownership markers and source identity, replaces only the generated region, and verifies byte-identical preservation of the handwritten region.
The external-workspace case changes the Rust declaration, observes strict drift failure, refreshes the scaffold and rebuilds the preserved theorem successfully.
The fourth PR 4 checkpoint reports every live `sorry` as a file-and-line proof-debt record. Scaffold obligations carry stable names, strict checks reject unnamed debt, and `--max-debt` enforces an explicit ratchet ceiling.
The fifth checkpoint adds `fr spec ci`. It generates a valid read-only GitHub Actions workflow for the initialized package, current `fr` release, reviewed debt ceiling and warnings-as-errors Lake build.
CI preview refuses symlink traversal and divergent workflow replacement. Saved plans, writes and reversal retain the source-history contract.
The sixth checkpoint adds `fr spec evidence`. The bounded result combines strict verification with checked theorem names, declared assumptions, trusted components, debt and the unproved implementation/model relationship.
Its axiom analysis explicitly covers declared syntax rather than transitive theorem dependencies. The external fixture asserts these evidence boundaries after a passing regeneration and Lake build.
The proof-debt predicate now has an anchored Lean model and four ratchet theorems. All 4,225 pairs from zero through 64 match Rust; the axiom audit reports only `propext`.
This correspondence covers the ceiling predicate. Debt discovery, marker parsing, command enforcement and the full adoption workflow remain host-tested boundaries.
Roadmap PR 4 passed the complete repository gate before merge. Strict project verification reported 33 fresh anchors, no remaining proof debt and a successful 40-job Lake build; formatting, Clippy, prose budgets, 311/311 capability coverage and both WASM configurations also passed.
PR 3's first checkpoint added checked staging-journal compaction with explicit per-stack retention.
Compacted records retain stable IDs, statuses, path counts and digests while discarding bytes that can no longer be replayed.
Preview/write bases, the index lock and full journal rechecks preserve the index, working files and concurrent staging state.
Lean proves compaction selection requirements, unselected-payload preservation and idempotence; all eight boolean selection states agree with Rust.
The second PR 3 checkpoint adds `compact-removals` for 1 through 32 explicit completed removal archives.
One aggregate basis binds every normalized archive and its individual review; sequential writes report completed and stopped paths if the set cannot finish.
The third checkpoint adds `stage-history inspect`, which reports pending journal actions, index-lock metadata and bounded leftover preparation directories without reading or removing them.
Only a state with no pending, lock or preparation evidence is clean in the Lean model and all eight Rust/Lean cases.
The fourth checkpoint records selected assume-unchanged and skip-worktree values in preview bases and staging journals, then restores them in prepared indexes across apply, removal, undo, redo and recovery.
Intent-to-add remains unsupported; flag drift refuses replay, plain schema-one records retain their prior serialized digest form, and all sixteen index-entry policy states agree between Rust and Lean.
The fifth checkpoint carries the repository-local `extensions.worktreeConfig` mode through creation receipts, recovery bases and removal archives.
Recovery preserves a bounded regular `config.worktree` without rewriting it, removal archives its exact bytes, older receipts default safely, and mode drift or non-regular paths refuse.
The anchored configuration guard proves the reviewed-mode and regular-file requirements; all sixteen boolean states agree between Rust and Lean.
The sixth checkpoint publishes a bounded destination-keyed preparation before worktree mutation and removes it after ownership receipt publication.
A killed creator with an exact retained registration can recover from this provisional evidence without adopting an arbitrary worktree; new-branch and existing-branch crash tests preserve source Git state.
Lean proves the absent-receipt, matching-preparation and matching-registration requirements, and all eight policy states agree with Rust.
The seventh checkpoint adds Git symlink blobs to raw creation, recovery, removal and removal-archive validation without following their targets.
It bounds target length, rejects NUL bytes before mutation and rechecks entry identity after reads. Submodules remain outside the owned lifecycle.
The anchored entry-mode policy accepts one recognized blob kind, and all sixteen boolean states agree between Rust and Lean.
The eighth checkpoint extends schema-one source snapshots with an omitted-by-default entry kind, preserving regular-record serialization while representing UTF-8 symlink targets explicitly.
`fr file symlink` creates or replaces one link, `file delete` accepts links, and history apply, undo, redo, recovery, basis checks and Git patch export preserve the entry kind without following targets.
Git type changes render as the paired deletion and addition records Git requires; the anchored snapshot-mode projection fixes links at `120000`, and 8,258 Rust/Lean cases cover both entry kinds across the existing mode corpus.
The ninth checkpoint closes the source-history preservation deliverable with a direct Git-backed CLI scenario.
An affected path may already be staged before planning. Apply, undo and redo leave the index byte-identical.
They retain unrelated staged and unstaged content, a later tracked source edit and a later untracked file.
The History kernel now models selected namespace replay. It proves that replay installs selected snapshots and preserves every unselected path's current value.
The tenth checkpoint runs scan plus save, apply, undo and redo with `PATH` pointing to a nonexistent directory.
This establishes the CLI boundary that ordinary analysis and source history do not require a Git executable.
All PR 3 deliverables and acceptance categories now have implementation, proof or host-test evidence.
The final native and WASM gates passed on the merged PR 3 head.
PR 2's first checkpoint extends exact-byte Rust insertion to impl and trait bodies and generalizes the insertion placement model.
The second checkpoint authors TypeScript/TSX expression-bodied arrows and permits checked transitions between expression and block bodies.
The third checkpoint adds Java method, constructor and default-interface body authoring through the same checked splice and history path.
The fourth checkpoint lets one authoring batch combine declaration, caller and conservative import-organization changes.
The fifth checkpoint adds exact batch postconditions for changed files, edits, operations and paths.
The sixth checkpoint anchors the batch selection-conflict predicate and compares 6,084 bounded cases across Rust, Lean and an independent oracle.
The final workflow checkpoints make skill reads bounded, execute the documented batch example, preserve code fragments through stdin JSON, and enforce the complete preview/save/apply/check/undo/check/redo/check/receiver sequence.
`tests/agent-eval/results/2026-09-09-structural-authoring` retains the passing Luna-low pair. The fr arm uses 18,806 context tokens and 29 calls with no refusals; files use 15,192 tokens and 20 calls.
Both arms pass exact transition, index and receiver checks plus 1,060 independent allocation and byte-count cases. Token audit and offline replay pass.
Project and author reports now emit a revision-bound `frcb1:` basis. Supplying it omits only `coverage`, `handle_prefix` and `revision`; stale bases refuse before author plans can be saved or written.
Complete saved author diffs and detailed history records now emit a separate `frtb1:` transaction basis. Forward apply and redo reports can omit their repeated diffs while retaining change metadata; reverse use and mismatches refuse before writes.
Unit and CLI regressions reconstruct full reports exactly and cover missing, truncated, stale and conflicting bases.
Direct patch output writes a new artifact atomically and returns only identity, size and transaction metadata, avoiding another full patch in agent context.
The fixed M4ab projection preserves every recorded outcome and lowers mean fr context from 13,278.5 to 11,229 tokens. The normalized file mean is 6,810, so PR 1 still has a 4,419-token gap to address.
`tools/agent-eval-codex.py` now supplies the opt-in real-agent runner. It requires a complete prepared pair and explicit spend acknowledgement, then records ephemeral Codex JSONL under pinned model, effort and service-tier settings.
M4ac is complete: the matched check-output projection shows that verbose successful logs masked substantial fr workflow context in M4ab.
M4l's four passing workspace trials are retained in `3c8231c`; preparation is `811591e` and documented insertion is `6d0928b`.
M4m's smaller check reports are committed in `58b53bd`; M4n's targeted skill guidance is `1c44863`.
M4o's bounded source lookup is committed in `5214404`.
M4p's source and budget proofs are committed in `aa16584`.
M4q's cache measurement and failure controls are committed in `9b0c84e`.
M4r's release profiling is committed in `9a253e9`.
M4s's batched revision hashing and retained measurements are committed in `fb2e252`.
M4t's buffer proofs and shared state comparisons are committed in `c9f50f0`.
M4u's wrapped function authoring is committed in `e2933bc`.
M4v's inline module insertion is committed in `2cdbd1b`.
M4w's Go body authoring is committed in `3597115`.
M4x's insertion placement proofs are committed in `3fd1323`.
M4y's coordinated authoring batches are committed in `f0ba5a5`.
M4z's controlled batch comparison is committed in `5591c17`.
M4aa's coordinated task preparation is committed in `1c96eb1`.
M4ab's four coordinated agent trials are committed in `56995e3`.

## Coordinated cohort results

The user approved four fresh evaluation agents after an explicit request to launch them.
Sessions live at `/tmp/fr-coordinated-eval-uihsblci/sessions`; preparation passed for all four.
The frozen binary is `target/agent-eval-bin/fr-m4ab`, matching the validated M4aa binary fingerprint.
`/tmp/fr-m4ab-frozen-inputs.json` records evaluator, skill, prompt and session fingerprints before agent work.
One fr agent and one ordinary-file agent ran per pair, with fresh context and inherited model settings.
The first pair, `coordinated_fr_r1` and `coordinated_files_r1`, finished before `coordinated_fr_r2` and `coordinated_files_r2` started.
No solutions or first-pair results reached later agents; frozen input fingerprints stayed unchanged throughout.
Scoring and repository checks began only after all four finished. No trials failed, restarted or were excluded; no human task corrections occurred.
`tests/agent-eval/results/2026-09-08-coordinated` retains all four passing scores, prompts, transcripts, patches and frozen skill files.
Both projects and receivers pass 1,060 independent inputs for each trial; exact reversal and unchanged-index checks pass.
The fr trials use 13,539 and 13,018 context tokens; file trials use 13,487 and 13,818, yielding a 2.7% mean reduction for fr.
Both file agents kept verbose check output while both fr agents used compact checks; this accounts for much of the aggregate result.
The fr trials still spend more context on inspection and changes/delivery, plus 2,851 skill tokens each, and more time in uncached project analysis.
M4ac now controls check-output policy with a fixed-transcript projection that preserves the original scores.
See [coordinated agent evaluation](agent-coordinated-evaluation.md) for the full protocol, categories and limitations.

## Matched check-output projection

`tools/checks-policy-context.py` validates the complete retained M4ab manifest before projecting either shared policy.
Prompts, requests, check listings, non-check payloads and call counts stay byte-identical; only executed structured check reports change.
The quiet-success projection retains declarations and yields means of 14,194.5 fr tokens and 7,726 file tokens, an 83.7% fr premium.
The compact projection also omits reviewed declarations and yields 13,278.5 and 6,810 tokens, a 95.0% fr premium.
File repetitions lose 5,925 and 5,928 successful-output tokens, then 916 declaration tokens each; fr already used the compact policy.
The projected mean gap is 6,468.5 tokens. This explains M4ab's apparent 2.7% advantage but does not alter its measured outcomes.
The result does not predict agent adaptation under a prescribed policy and does not isolate individual fr commands.

The live fixture checks both successful and failing commands, output limits and invalid UTF-8.
Its transformed reports equal actual CLI policy reports apart from elapsed time; failed diagnostics and raw byte totals remain intact.
Projection refuses changed evidence, missing or stale listings, declaration mismatches, contradictory success reports and truncated payloads.
`tests/agent-eval/checks-policy-context.json` retains transformed executions and checksums for the cohort, binary, tokenizer and measurement sources.
PR 1 now implements the first project and history reductions and retains their fixed projection separately from autonomous outcomes.
The obsolete Lean scanner helper that produced a native-build warning was removed while touching the transaction path.
The first Luna-low pair passed every behavioral oracle but failed strict workflow ordering. A Terra-low calibration restored ordering but repeated a successful saved plan, exposing a retry-idempotency gap in source history.
Identical pending plans now reuse one transaction, and the cohort prompt forbids replaying successful reads, checks, mutations and delivery steps.
The next Luna-low `fr` run preserved one transaction and the workflow shape, but twice lost the final hex digit of a reviewed check basis. Check execution now accepts a matching prefix of at least 128 bits while retaining the full digest in reports.
The final Terra-low diagnostic pair is retained at `tests/agent-eval/results/2026-09-08-context-v2`. The fr arm uses 15,979 context tokens and passes the project and receiver oracles, but an incomplete saved plan followed by a complete plan and a missing sentinel fail coordinated workflow acceptance. The file arm uses 11,025 tokens and completes the workflow, but its incomplete metacharacter set fails the independent oracle in both project and receiver.
The cohort therefore supplies negative diagnostic evidence rather than a context comparison. Its manifest records failed acceptance; token audit passes and behavioral replay refuses the failed score by design.
Evidence recording now retains Codex JSONL, stderr, final output and launch metadata, and accepts an explicit evaluated implementation commit so later recording work cannot misidentify an older binary as current `HEAD`.
The complete [v2 evaluation](agent-context-v2-evaluation.md) separates the fixed passing-cohort projection from fresh-agent outcomes. PR 1 does not claim context parity; the next measured optimization target is skill loading plus authoring and delivery receipts.

## Coordinated workspace task preparation

`tools/agent_eval/regex_escape_len.py` adds `regex-escape-len` on the existing pinned regex archive and dependency lock.
The task adds allocation-free escaped byte-length APIs in both crates and preallocation in the lower crate's existing `escape` function.
The independent oracle checks both public APIs, an explicit escaping reference, UTF-8 lengths, allocation counts and preserved append behavior.
Preparation recognizes only missing requested APIs as the original baseline; unrelated compiler diagnostics fail preparation.

`tools/agent-eval.py` adds `--project regex-coordinated`, generating paired `regex-escape-len` sessions.
Task-specific edit/export paths allow exactly the two crate roots; older tasks retain their single-file allowance.
Scoring requires both files to change. Replay checks both recorded results and whole tracked snapshots through undo and reapplication.
The fr arm must use one saved two-file batch and its matching history commands, alongside the existing ordered checks and receiver evidence.
Five new harness scenarios cover paired names, path allowances, diagnostics, coordinated delivery and second-file replay refusal.

`tools/regex-coordinated-check.py` rehearses two documented insertions and one body replacement through actual instrumented fr commands.
It checks the project at four stages, a separate patch receiver, unchanged indexes and preservation of an unrelated edit.
Negative controls target Unicode counting, missing escaping, allocation during length calculation, omitted preallocation and a disagreeing facade.
The retained `tests/agent-eval/regex/coordinated-rehearsal.json` passes all stages and 1,060 independent inputs in both project and receiver.
All five incorrect implementations compile but fail the oracle. The report retains commands, reports, snapshots, the patch and source/binary hashes.
The rehearsal remains prescribed infrastructure evidence. The retained 2026-09-09 structural-authoring pair supplies fresh autonomous acceptance for PR 2.

## Controlled coordinated authoring comparison

`tools/author-batch-context.py` compares individual authoring commands and a batch on a prescribed two-file Rust fixture.
Three retained pairs alternate route order, using equal-length temporary roots, disabled caches and complete source selections and review diffs.
Both routes add a helper, change a callee signature and implementation, then update its caller.
They compile with warnings denied and check runtime output at original, applied, undone and redone stages.
Exact snapshots and modes match across routes; index bytes stay unchanged and unrelated source survives reversal.
Exported patches apply to a separate receiver that matches the expected source and compiled output.

`tests/agent-eval/author-batch-context.json` retains every command output, artifact, patch, source snapshot and measurement/binary provenance.
Batch uses 12 calls and one transaction, versus 23 calls and three transactions.
Median visible payload tokens are 5,102 versus 7,408, about 31% lower; the manifest adds 458 prepared input bytes.
Request and artifact sizes remain separate from returned output. This is a synthetic prescribed workflow, without autonomous agent or latency claims.
Project/author calls fall from six to three; inspected routing implies twelve versus six scan passes, without runtime instrumentation.
Five evaluator regressions reject failed or clipped evidence, incomplete review and selection, and unexpected source bytes; they also check metric accounting.
The native acceptance suite now executes one complete pair. No production command or Lean model changes in this milestone.
The coordinated workspace preparation above follows this controlled comparison. PR 2 closes its autonomous acceptance with the retained 2026-09-09 structural-authoring pair.

## Coordinated authoring batches

`src/project/author.rs::author_batch` reads a bounded JSON manifest and plans each existing operation against the same captured project.
The manifest accepts 1 through 32 entries with `op` and `handle`; fragment operations also require `from`.
An optional `revision` supports short IDs and validates full-handle batches too.
An `organize-imports` entry uses a file handle without `from` and reuses the existing conservative import planner.
Unknown fields and operations refuse. All input paths resolve from the workspace root; input files retain the regular-file and 64 KiB guards.
Selected original regions must be disjoint even for no-ops; insertion points cannot share or touch another region's boundary.
Every step must succeed, and combined file results must reparse before the CLI records one source-history transaction.

`src/cli.rs::cmd_author` routes batches through the existing diff, persistence and source-verification path.
Schema `fr-author-batch-1` reports coverage once, ordered step signatures and original spans, sizes and hashes.
Optional postconditions report expected, actual and held values. Any mismatch refuses before persistence.
It omits after-spans because earlier edits can shift later positions. Insertion hashes include separator bytes.
Saved plans freeze all changes; unrelated source changes retain the existing history rules, and affected-file conflicts refuse the entire application.
This does not strengthen filesystem atomicity or prove typing and behavior.

Seven new CLI scenarios include a compiled two-file caller/signature/helper change through save, apply, undo, patch checking and redo.
The PR 2 import case combines a declaration, caller and import cleanup in one transaction and compiles with warnings denied.
Mixed Rust/Go/TSX operations, shared revisions, no-op batches, size/count limits, overlap refusal and malformed inputs have regression coverage.
A two-edit batch also matches the Rust and Lean splice implementations, with Unicode, CRLF and different replacement lengths.
A body-and-import batch supplies another Rust/Lean splice comparison over Unicode source.
The shared `author_selection_conflict` helper now gives the batch planner an anchored Rust boundary for duplicate, nested and insertion-boundary rejection.
Six Lean theorems cover symmetry, adjacent nonempty ranges, overlap, distinct insertion points and insertion at either boundary.
The executable comparison checks 6,084 valid 64-bit range pairs against Lean, Rust and an independent interval oracle, with explicit multibyte UTF-8 boundary cases.
This is tested edit and selection-predicate correspondence, with no proof of the manifest parser or filesystem transaction implementation.
The agent reference teaches a concrete batch manifest and the preview, save and history-apply sequence. The skill checker executes this two-operation example through exact undo and redo.
The controlled comparison above measures report bytes and repeated calls without treating a prescribed workflow as autonomous agent evidence.

## Formal declaration insertion placement

`src/project.rs::declaration_insertion_offset` supplies the placement calculation for inline module, impl and trait insertion.
The caller passes the prefix before the selected container's closing brace plus the opening-brace offset.
`kernels/FrKernels/Author.lean` models the result through character lists and UTF-8 byte widths.
Its fifteen theorems cover bounds, boundary validity, placement after an opening brace within the input, and an indentation-only suffix.
They characterize a trailing indented line, inline content, out-of-body line candidates and empty input.
A source anchor and explicit signature map identify the helper. Theorems use only `propext`, `Classical.choice` and `Quot.sound`.

`ProjectMain.lean` exposes `declaration-offsets` for the generated corpus and `declaration-offset BODY_START TEXT` for a selected case.
The default kernel gate runs the corpus through the existing project executable.
Integration scenarios compare 28,185 cases on 64-bit hosts with both Rust and a reverse-scan oracle, and check ten CLI previews across modules, impls and traits.
The corpus includes Unicode, CRLF, rejected indentation lookalikes, NUL in the pure helper, machine limits and large prefixes.
A 32-bit host compares 25,371 representable cases.
Each generated placement also passes through the Rust edit engine, with unchanged prefix and suffix checks.
Existing authoring cases retain their behavior and transaction evidence.
The Java method and Rust impl history cases now also assert that mode `0640` survives apply, undo and redo on Unix.
This proves model properties and tests correspondence; AST selection, parsing, name checks and full authoring refinement remain unproved.
See [placement kernels](lean-specs.md#declaration-insertion-placement-kernels) for assumptions and reproduction.

## Go body authoring

`src/project/author.rs::BodySyntax` includes Go function and method declarations with brace-delimited blocks.
The existing exact-name-span selector distinguishes same-named methods on different receivers.
Function literals in variables, interface specifications and bodyless declarations refuse.
The fragment parser accepts exactly one block in a temporary function, then reparses the destination file.
Receiver headers, generic parameters, named results and directives outside the block remain unchanged.
Compilation, imports, package rules and behavior require separate project checks.

Five new authoring scenarios cover declaration forms, source preservation, size boundaries, revision guards and unsupported selections.
Five compiled history fixtures cover generic functions, pointer receivers, named results with deferred updates, `init` and multiline raw strings.
They check behavior before edits, after application, after undo and after redo, with frozen fragments and applicable patches.
A reported Go method edit is compared with Rust and Lean splice implementations, preserving Unicode, CRLF and surrounding comments.
This extends tested splice correspondence; it adds no new theorem or general proof of AST selection, typing or authoring behavior.
The agent reference states which Go handles to select and which forms refuse.

## Java body authoring

`src/project/author.rs::BodySyntax` validates a replacement block as a method inside a temporary Java class.
It accepts indexed method and constructor declarations with bodies, including default interface methods.
Abstract methods, bodyless interface declarations, initializer blocks and lambdas remain outside this target path.
Annotations, modifiers, generic headers, parameters, throws clauses and all source outside the braces remain byte-identical.

A saved method transaction compiles and runs with `javac -Xlint:all -Werror` before application, after application, after undo and after redo.
Constructor and default-interface fixtures also compile with warnings denied after replacement.
The method workflow checks its patch after undo and freezes the saved block against later fragment changes.
A reported Java method edit matches the Rust and Lean splice implementations with Unicode, CRLF and a neighboring method.
These checks establish parser acceptance, compiled examples and edit correspondence. They do not prove Java typing, behavior or parser correctness in general.

## Rust declaration insertion containers

`src/project/author.rs::insert_declaration` accepts a file, exact Rust inline module or trait handle, or an existing direct method handle that identifies its exact enclosing impl or trait.
It locates the selected declaration list and inserts at its closing brace.
A closing brace on a whitespace-only line keeps that indentation; inline braces receive a leading separator.
Fragments remain verbatim, preserving multiline strings. Existing source bytes stay unchanged.
Duplicate-name and dangling-outer-metadata checks apply to the selected body's direct items.
External modules, free and nested functions and empty impls refuse. Traits alone accept bodyless functions.
Container reports identify an inline module, impl or trait and include the original body span and scoped name-check description.

The original module scenarios cover exact placement, raw identifiers, same-named modules, size/hash boundaries and stale selections.
A saved impl insertion compiles with warnings denied after application and redo, restores exact source on undo and exports a checked patch.
Trait scenarios compile a default method and cover bodyless requirements through both trait and member handles.
A nested function calls a private sibling after saved application and redo; undo restores exact original bytes and the patch applies after undo.
The saved transaction retains its fragment despite later changes to the input file.
Handle validation covers the project revision; later history application checks affected files and permits unrelated manifest changes.
A reported module, impl and trait edit is compared with the Rust and Lean placement implementations.
This is tested splice correspondence, with no new proof of AST selection, parsing or complete authoring behavior.
The portable authoring reference explains module-row handles, preserved fragment contents and unsupported scopes.

## Wrapped function and expression-body authoring

`src/project/author.rs::function_initializer` follows the expression operand through parentheses, `as`, `satisfies`, postfix `!` and TypeScript angle-bracket assertions.
It accepts arrow, ordinary function-expression and generator-expression terminals. Arrows accept expression or block bodies and can transition between the forms.
Other function terminals continue to require blocks.
Comments do not count as operands. The angle-bracket form skips its type-argument child and follows the value expression.
Calls, conditionals, comma expressions and other initializer forms refuse, even inside an otherwise supported wrapper.
The existing handle/name check prevents selecting an enclosing or neighboring function.

The resulting edit preserves wrappers, types, comments and every byte outside the selected braces.
The existing report schema, source-basis guards, bounded diffs, save/apply path, undo/redo and patch handling remain in use.
Its signature stays a header excerpt ending before the body; inspect selected source for postfix assertion types.
The portable authoring reference names the accepted wrappers and uses `--locals` lookup for variable handles.
Its executable command examples stay unchanged; current skill text is separate from historical context measurements.

The authoring suite includes shadowed wrapped bindings and stale handles or plans after a wrapper changes.
Four compiled history fixtures check lexical `this`, named recursion, generators and JSX before/after application, undo and redo.
They also freeze the saved fragment and check patch applicability after undo.
An additional expression-arrow history fixture compiles before application, after application and redo, and after exact undo.
The Rust/Lean edit comparisons include reported wrapped block and expression TSX replacements with Unicode, CRLF, external comments and neighboring source.
This extends tested splice correspondence; it adds no proof of AST traversal, parsing, typing or complete authoring behavior.
See [body authoring](body-authoring.md) for supported targets and review limits.

## Revision buffer verification

`kernels/FrKernels/Digest.lean` adds twenty-one theorems over emitted and pending lists.
They cover flush idempotence, preserved byte order, failed-write restoration, pending bounds, sequence composition and threshold/flush-schedule independence.
An abstract digest theorem requires the update function's chunk-composition law as an explicit premise.
The model handles arbitrary finite sequences and natural thresholds; pending bounds require a positive threshold and a bounded initial state.
All twenty-one theorem dependencies use only `propext` and `Quot.sound`, with some requiring no axioms.

`src/project/digest.rs` holds the private implementation extracted from M4s, unchanged apart from module visibility qualifiers.
`tests/lean_digest.rs` compiles this same module and compares it with `kernels/DigestMain.lean` through the `fr-digest-kernel` executable.
It checks 1,570 states across 404 sequences against both Lean and explicit JSON-byte expectations.
Four hundred sequences cover every zero-through-three-operation combination from seven operations; four longer cases exercise size boundaries and large failed writes.
Each state checks exact pending bytes, the emitted-byte digest, final digest, successful-byte order and the post-operation pending bound.
The test uses Rust's fixed 65,536-byte threshold; arbitrary-threshold laws belong to the model.

This model has no separate source anchor and does not prove general Rust refinement, serialization, SHA-256 internals, allocation or revision-input selection.
Existing source anchors retain their own scope. There are no new custom axioms or proof obligations.
The default Lake targets and `tools/check-kernels.sh` include the new executable; the native Rust gate runs its comparison.
Measurement tools now include the extracted module in future source provenance. Existing artifacts remain immutable and refer to their original source layouts.
See [revision buffer kernels](lean-specs.md#revision-buffer-kernels) for assumptions and reproduction.

## Batched revision hashing

Project construction uses a reusable `RevisionDigest` buffer while preserving every serialized input, its ordering and SHA-256.
It flushes after complete items reach 65,536 bytes and at finalization; an individual item can exceed this threshold.
Capacity is retained until construction ends. A serialization error truncates its partial item while preserving previously buffered bytes.
Three unit regressions cover explicit JSON encodings, large-to-small reuse, flush boundaries and recovery from partial serialization failure.
M4s added no formal proof of revision hashing; M4t adds the separate buffering model above.

`Project::new_profiled` adds nine construction stages to the development example when invoked with `--construction`.
The ordinary constructor disables checkpoints at compile time, and normal CLI reports keep their existing fields.
Nested timings include checkpoint overhead; buffered hash work can be charged to a later stage that triggers its flush.
The normal CLI measurements therefore provide the end-to-end comparison.

`tools/project-construction.py` alternates separate baseline/candidate CLI and profiler executables on populated caches.
Four repetitions per lookup require complete report equality and all 249 fact-cache hits in each profiler sample.
Full package, dependency, workspace, gap and default nonlocal map pagination adds 22 matching pages.
Both source-invalidation probes require stale-handle refusal, cached/uncached agreement and restoration of source and index bytes.
Twenty-one evaluator regressions now include missing, negative and excessive construction-stage intervals.

`tests/agent-eval/project-construction.json` retains sixteen ordinary CLI samples and sixteen separate profiles from `/tmp/fr-m4s-comparison-final.json`.
Cached CLI medians fall from 179.481 to 168.476 ms for `escape`, and from 182.837 to 172.008 ms for `escape_into`.
Profiled construction falls by about 13 ms; reference serialization and hashing remain the largest stage at about 98 ms.
All samples, 22 additional pages and both probes pass. The independent audit verifies digests, medians, cache inventories and retained payloads.
These are single-host query measurements, with no autonomous context-saving or general production-latency claim.

The baseline CLI is `target/agent-eval-bin/fr-m4s-before`, matching M4r's retained release digest.
The baseline profiler is `target/agent-eval-bin/project-profile-m4s-before`; it adds checkpoints before the digest optimization.
`tests/agent-eval/project-construction-before.patch` reproduces this instrumentation against `9a253e9` in a disposable checkout.
The patch applies cleanly to both original files and preserves all five original per-item digest updates.
See [batched revision hashing](project-context-evaluation.md#batched-revision-hashing) for reproduction, allocation behavior and evidence limits.

## Release profiling

`tools/project-profile.rs` is the `project-profile` Cargo example, gated on the CLI feature.
It times the public library pipeline with default scan options and a project directory, then emits a separate profiling envelope.
M4r left normal CLI output and production library code unchanged; M4s adds the optional constructor profiling above.
Stages cover root resolution, scanning, cache opening, indexing, project construction, querying, final verification, serialization and cleanup.
The internal interval excludes process startup, argument parsing and envelope output; Python also records complete subprocess time.

`tools/project-profile.py` checks every report against the release CLI on the same pinned regex fixture.
Eighteen samples pass across disabled, empty and populated caches; all populated samples report 249 fact hits for 249 indexed files.
The profile is `tests/agent-eval/project-profile.json`, copied from `/tmp/fr-m4r-project-profile.json`.
Populated subprocess medians are 179.2 and 178.9 ms. Project construction takes about 146 ms, indexing about 11 ms and verification about 9 ms.
Separate phase medians need not sum to the median whole-command time.

`tests/agent-eval/project-cache-release.json` repeats the original cache procedure with the same optimized CLI.
All eighteen ordinary CLI samples and both source-invalidation probes pass; populated medians are about 180 ms and disabled medians about 1.4 seconds.
It comes from `/tmp/fr-m4r-project-cache-release.json`; earlier debug and autonomous evidence remains immutable.
Both artifacts retain binary and input digests, runtime details and raw samples; their statistics and payload checks pass an independent audit.
M4r's twenty evaluator regressions include missing/negative phases and invalid interval totals.
See [release profiling](project-context-evaluation.md#release-stage-profiling) for reproduction and measurement limits.

## Cache measurement

`tools/project-cache.py` compares identical bounded source lookups with disabled, empty and populated fact caches.
Each sample owns a temporary cache outside the pinned regex workspace; mode order rotates across three repetitions.
Complete JSON must match across modes, including revisions, handles, coverage, source and omissions.
The report records whole-subprocess times and cache inventory digests, without phase profiling or per-query cache-hit counters.
Priming queries and cache inventory reads stay outside the timing summaries; priming times remain separately visible.
This uses the validated debug binary and makes no production-latency or agent-context claim.
The retained report is `tests/agent-eval/project-cache.json`, from the confirmation run in `/tmp/fr-m4q-project-cache-final.json`.
All eighteen timed queries pass: disabled medians are 10.088 and 10.203 seconds; populated medians are 3.280 and 3.282 seconds.
The respective reductions are 67.5% and 67.8%. Empty-cache medians remain near the disabled values.

Two temporary comment probes compare cached and uncached changed reports, reject stale handles and restore the original reports.
Tracked source bytes and modes and the Git index must finish unchanged; all cache paths belong to the disposable fixture.
Existing agent evidence and the harness's `--no-cache` policy remain unchanged.
Three new evaluator regressions reject report changes and incomplete source, and require source restoration after an injected query failure.
M4q's default acceptance harness ran nineteen regressions; M4r adds the phase-timing control above.
See [cache measurements](project-context-evaluation.md#query-time-and-the-fact-cache) for the retained report and reproduction.

## Source verification

`src/project.rs::source_slice_length` serves both find and show through their existing source renderer.
The extraction preserves the CLI's slicing behavior and invalid-offset diagnostic.
`kernels/FrKernels/Source.lean` anchors that helper with an explicit signature map.
Nineteen theorems cover valid boundaries, bounds, maximality, continuation conditions, byte partitioning and page allocation.
The page model preserves empty rows and shares one raw source-text budget, including unused bytes from UTF-8 clipping.
It models the caller's allocation loop without a separate source anchor.

Three new Lean integration scenarios pass: 19,220 slice cases, 5,180 page allocations and actual lookup pages across eleven budgets.
The slice corpus includes machine limits, invalid offsets and Unicode width boundaries; an independent forward scalar oracle checks Rust.
The page corpus covers all sequences through three rows from six strings, including empty and multi-byte-leading rows.
The project executable accepts `source-slices` and `source-pages` for these corpora and `source-page BUDGET TEXT...` for CLI comparisons.
The gate builds the Source module and runs both corpora. CLI flags and portable skill examples remain unchanged.

These are model proofs with tested Rust and CLI correspondence; no proof covers the entire implementation.
The axiom audit lists only `propext`, `Quot.sound` and `Classical.choice`; no new custom or compiler-trust axiom appears.
Parser spans, UTF-8 library internals and report assembly remain outside the proofs.
See [bounded source kernels](lean-specs.md#bounded-source-kernels) for the precise domain and evidence.

## Current implementation

`project find NAME --source --bytes N` appends source slices under one raw UTF-8 byte budget shared across the returned page.
The range is 4 through 65,536 bytes, defaulting to 2,048. Slice metadata and handles remain when later rows receive no source bytes.
Resume a non-null `source.next_offset` with `project show HANDLE --source --offset NEXT --bytes N`.
Row cursors bind source mode and budget; ordinary lookups retain their previous reports and cursor identities.
Both find and show call the same source-slice helper, which reuses the existing modeled page-length arithmetic.
Use show separately when positions, child counts or relationships are needed.

The controlled report is `tests/agent-eval/find-source-context.json`; reproduce it with `tools/find-source-context.py --fr PATH --tokens`.
Two prescribed regex queries preserve lookup metadata and complete selected source while using about 40% fewer visible payload tokens.
The comparison checks unchanged tracked source and index bytes and records actual arguments, payloads and binary/source digests.
It does not run agents, discover the targets, build the workspace or measure total-task context or latency.
See [source lookup measurements](project-context-evaluation.md#bounded-source-during-name-lookup) for the exact counts and reproduction.

## Current handoff

Targeted edits start with Author. Pagination, relationship queries and broader discovery load Explore when needed.
History links to `references/recovery.md` only for a pending operation or interrupted lock; existing recovery constraints are preserved there.
Only a lookup explicitly scoped to an existing file supplies that file's insertion handle in `root`.
Unscoped/directory roots cannot substitute, and source changes invalidate the handle. A file map remains the fallback.

The skill checker executes 38 examples. Its Rust batch example compiles under `deny(missing_docs)` and passes a compiled caller assertion.
It also checks wrong/stale-root refusals, exact undo/redo, unrelated-edit preservation and unchanged index bytes.
The example uses the lookup's source and root directly, without separate source or file-map queries.

`tests/agent-eval/skill-context.json` retains controlled reading costs and the handle-reuse trace audit.
M4n's targeted route costs 2,375 tokens, versus 2,825 recorded skill tokens; this is a conditional 15.9% reduction.
M4n's six-reference route cost 2,960 tokens. Both M4ab fr agents later followed the targeted five-file route on the newer skill.
The first regex fr trial has one redundant 387-token map; the second lacks the prerequisite scoped lookup and still needs its map.
`tools/skill-context.py --tokens` reproduces the report using the pinned tokenizer; omit the flag for byte counts without tiktoken.
See [targeted reading measurements](agent-skill.md#targeted-reading-measurement) for scope and the extra cost when recovery is needed.

## Check-report reduction

After reviewing `fr checks`, execution can omit repeated declarations while retaining names, basis, outcomes, diagnostics and unselected names.
The report marks `declarations_omitted: true`; join results to the reviewed listing with the same basis for command metadata.
Default reports and listings retain their fields. Configuration guards and execution behavior stay unchanged.
The portable skill's existing check example combines this option with quiet-success output; all 33 examples pass.
Twelve check CLI scenarios pass, including failed/spawned commands, bounded invalid UTF-8 and reconstruction against the listing.

`tools/checks-context.py` compares real reports on the pinned regex workspace and projects omission onto frozen trial payloads.
Each retained fr trial's four execution reports fall from 2,720 to 1,804 tokens, a 33.7% reduction.
The live comparison preserves tracked source and index bytes; successful output lengths and execution timings can vary.
`tests/agent-eval/checks-context.json` retains actual stdout, fixed-projection counts, binary digest and input provenance.
No new agents ran. These measurements exclude the listing and other task context and do not replace autonomous trial scores.
See [check report measurements](project-checks.md#controlled-report-measurement) for reproduction and boundaries.

## Earlier single-file workspace result

The regex task adds a documented public buffer-writing API by discovering and using an existing workspace helper.
It exercises a real seven-package repository with 227 Rust files and 5,553,380 Rust source bytes.
Only `src/lib.rs` changes, so this is not evidence of coordinated edits across packages.
Four fresh agents ran as two sequential pairs, without conversation history, shared solutions or human task corrections.
No trials failed, restarted or were excluded. Both arms used the same instrumented checks and cooperative tool boundary.

| Repetition | fr context tokens | File context tokens | Result |
|---|---:|---:|---|
| 1 | 11,967 | 6,113 | Both pass |
| 2 | 11,603 | 12,660 | Both pass |

Across both repetitions fr uses 25.6% more measured context.
Both fr agents use exact-name lookup, quiet-success checks and smaller history completion reports without tool failures.
The second file agent keeps verbose check output, accounting for much of the difference between file repetitions.
Skills and transaction reports remain fixed costs; repeated uncached project indexing dominates fr tool time.
These results establish working agent transactions, not general context savings or production latency.

Every trial passes both declared checks in original, changed, undone and redone states.
The checks cover 154 regex/regex-syntax library tests and no-default-feature compilation; integration and documentation tests are excluded.
The independent append/allocation oracle passes 3,174 cases in each project and each clean patch receiver.
Undo/redo restore exact tracked bytes and modes and preserve an unrelated later edit. Project and receiver indexes stay unchanged.
See the [workspace report](agent-workspace-evaluation.md) for raw results, source provenance, runtime details and limitations.

## Durable evidence

Never rewrite existing evidence when changing the tool, skill or evaluator.
Each cohort retains prompts, transcripts, scores, patches and its original skill snapshot, with manifest checksums.

- `tests/agent-eval/results/2026-09-07`: four initial strsim trials; interrupted infrastructure pilots are retained separately.
- `tests/agent-eval/results/2026-09-07-context`: four follow-up strsim trials after M4i; no pilots.
- `tests/agent-eval/results/2026-09-08-regex`: four repeated regex workspace trials; no pilots.
- `tests/agent-eval/results/2026-09-08-coordinated`: four two-crate regex trials using the M4aa protocol; no pilots.
- `tests/agent-eval/results/2026-09-09-structural-authoring`: one fresh two-crate pair using the completed PR 2 workflow; no pilots.

All eighteen retained acceptance trials pass. Recording also supports scored failures; behavioral replay refuses failed trials.
Replay checks recorded patches and transition evidence without rerunning agents. Token auditing recounts retained payloads.
The initial and follow-up strsim findings remain in the [context report](agent-context-followup.md).
Controlled reports are separate under `tests/agent-eval/`: `history-context.json`, `regex/rehearsal.json`, `checks-context.json`, `skill-context.json`, `find-source-context.json` and `project-cache.json`.
Release reports add `project-cache-release.json` and `project-profile.json` without rewriting those earlier artifacts.
M4s adds `project-construction.json` and its baseline instrumentation patch while preserving those reports.
M4z adds `author-batch-context.json`; M4aa adds `regex/coordinated-rehearsal.json`. Both remain prescribed comparisons rather than autonomous trials.
M4ac adds `checks-policy-context.json`, a fixed projection rather than a new agent trial.
The regex rehearsal uses a prescribed solution and rejects three compiled negative controls; it is not autonomous evidence.

Temporary regex sessions remain under `/private/tmp/fr-regex-agent-eval-2026-09-08`, one directory per retained trial name.
The frozen binary is `target/agent-eval-bin/fr-workspace-eval`.
Its SHA-256 is `6eccc6da5db83d295fa447089f6a0337e69a3adf5633df59c4fb60193026e856`.
It, the skills, prompts, evaluator and oracle stayed unchanged throughout both pairs.
Temporary projects are disposable after retention; use repository evidence for replay and audits.

## Validation and commands

M4ac's full native/WASM gate passes in `/tmp/fr-m4ac-full-check.log`, including 311/311 capability coverage and the live projection acceptance test.
All 38 evaluator regressions pass, including evidence-tampering, stale-basis, truncated-payload and failed-diagnostic controls.
The tokenized report is `/tmp/fr-m4ac-checks-policy-final.json`, retained as `tests/agent-eval/checks-policy-context.json`.
It uses frozen binary `target/agent-eval-bin/fr-m4ab`; the report binds that binary, the M4ab manifest, tokenizer and measurement sources by checksum.
Prose budgets remain unchanged, and `git diff --check` passes.

M4ab's full native/WASM gate passes in `/tmp/fr-m4ab-full-check.log`, including 311/311 capability coverage and the existing acceptance regressions.
All four retained coordinated trials replay successfully in `/tmp/fr-m4ab-replay.json`; exact token auditing passes in `/tmp/fr-m4ab-token-audit.json`.
Preparation and recording logs are `/tmp/fr-m4ab-prepare.json` and `/tmp/fr-m4ab-record.json`; individual scoring outputs use `/tmp/fr-m4ab-score-*.json`.
The evidence manifest verifies all 32 retained files. Frozen input fingerprints match before pair two and after all trials.
Strict verification passes in `/tmp/fr-m4ab-spec-verify.json`: 24 fresh anchors and signature maps, zero obligations and 38 Lean build jobs.
Prose budgets remain unchanged. Final documentation checks are in `/tmp/fr-m4ab-docs-final.log`.

M4aa's full native/WASM gate passes in `/tmp/fr-m4aa-full-check.log`, including 311/311 capability coverage and the existing authoring/Lean comparisons.
All 31 evaluator regressions pass in `/tmp/fr-m4aa-harness-tests.log`.
The successful real-workspace rehearsal is `/tmp/fr-m4aa-rehearsal.json`, retained as `tests/agent-eval/regex/coordinated-rehearsal.json`.
It uses frozen CLI `target/agent-eval-bin/fr-m4aa`, with matching binary and evaluator-source fingerprints.
The initial rehearsal refused a function-pointer missing-value diagnostic; the final classifier accepts that exact missing API and still rejects unrelated errors.
All four historical regex trials replay successfully in `/tmp/fr-m4aa-regex-replay.json`.
Strict verification passes in `/tmp/fr-m4aa-spec-verify.json`: 24 fresh anchors and signature maps, zero obligations and 38 Lean build jobs.
Prose budgets remain unchanged. Final documentation checks are in `/tmp/fr-m4aa-docs-final.log`.

M4z's full native/WASM gate passes in `/tmp/fr-m4z-full-check.log`, including 311/311 capability coverage and the new live workflow pair.
All 26 evaluator regressions pass in `/tmp/fr-m4z-harness-tests.log`, including the final added-source refusal guard.
The focused native workflow test passes in `/tmp/fr-m4z-workflow-test.log`.
The final three-pair token measurement is `/tmp/fr-m4z-batch-final.json`, retained as `tests/agent-eval/author-batch-context.json`.
It uses the frozen validated CLI copy `target/agent-eval-bin/fr-m4z`; later feature builds can replace `target/debug/fr`.
Strict verification passes in `/tmp/fr-m4z-spec-verify.json`: twenty-four fresh anchors and signature maps, zero obligations and 38 Lean build jobs.
Prose budgets remain unchanged. Final documentation checks are in `/tmp/fr-m4z-docs-final.log`.

M4y's full native/WASM gate passes in `/tmp/fr-m4y-full-check.log`, including all fifty-one authoring scenarios and 311/311 capability coverage.
The seven new batch scenarios and reported batch splice comparison also pass in `/tmp/fr-m4y-batch-final.log`.
Six reported authoring edit comparisons now pass against the existing Lean splice kernel.
Strict verification passes in `/tmp/fr-m4y-spec-verify.json`: twenty-four fresh anchors and signature maps, zero obligations and 38 Lean build jobs.
Prose budgets remain unchanged; the skill validator passes. Final documentation checks are recorded in `/tmp/fr-m4y-docs-final.log`.

M4x's fifteen theorems build with warnings as errors in `/tmp/fr-m4x-lean-build.log` (38 jobs).
The axiom audit is `/tmp/fr-m4x-axioms.log`, generated from `/tmp/fr-m4x-axioms.lean`.
Both placement scenarios pass in `/tmp/fr-m4x-placement-tests.log`: 28,185 shared cases and eight actual CLI previews on this 64-bit host.
All forty-four authoring scenarios pass in `/tmp/fr-m4x-author-final.log`.
Strict verification passes in `/tmp/fr-m4x-spec-verify.json`: twenty-four fresh anchors and signature maps, zero obligations and 38 Lean build jobs.
The initial strict check, anchor preview and reviewed synchronization are in `/tmp/fr-m4x-spec-before.json`, `/tmp/fr-m4x-anchor-preview.json` and `/tmp/fr-m4x-anchor-sync.json`.
Prose budgets remain unchanged. The full native/WASM gate passes in `/tmp/fr-m4x-full-check.log`, including 311/311 capability coverage.
Final test-target linting passes in `/tmp/fr-m4x-final-clippy.log`; all six documentation suites pass in `/tmp/fr-m4x-docs-final.log`.
Formatting and diff checks pass.

M4w's forty-four authoring scenarios and 35 enabled Lean integration scenarios pass in `/tmp/fr-m4w-focused.log`.
Two existing deep self-audits remain outside the default gate.
All five reported-edit Rust/Lean comparisons pass, including the new Go receiver-method case.
Strict verification passes in `/tmp/fr-m4w-spec-verify.json`: twenty-three fresh anchors and signature maps, zero obligations and 36 Lean build jobs.
The direct Go lookup-to-saved-plan smoke check passes in `/tmp/fr-m4w-agent-smoke.log`.
The skill frontmatter validator passes; prose budgets remain unchanged.
The full native/WASM gate passes in `/tmp/fr-m4w-full-check.log`, including 131 project CLI scenarios, the packaged skill workflow and 311/311 capability coverage.
All six final documentation suites pass in `/tmp/fr-m4w-docs-final.log`; formatting and diff checks pass.

M4v's thirty-nine authoring scenarios pass in `/tmp/fr-m4v-author-final.log`.
All four reported-edit Rust/Lean comparisons pass in `/tmp/fr-m4v-reported.log`, including nested module insertion.
Strict verification passes in `/tmp/fr-m4v-spec-verify.json`: twenty-three fresh anchors and signature maps, zero obligations and 36 Lean build jobs.
The skill frontmatter validator passes; prose budgets remain unchanged.
The full native/WASM gate passes in `/tmp/fr-m4v-full-check.log`, including 131 project CLI scenarios, the packaged skill workflow and 311/311 capability coverage.
A direct file-scoped lookup-to-saved-module-plan smoke check passes.
All six final documentation suites pass in `/tmp/fr-m4v-docs-final.log`; formatting and diff checks pass.

M4u's thirty-four authoring scenarios pass in `/tmp/fr-m4u-author-final.log`.
All three reported-edit Rust/Lean comparisons pass in `/tmp/fr-m4u-reported.log`, including the new wrapped-body case.
The skill frontmatter validator passes; prose budgets remain unchanged.
The full native/WASM gate passes in `/tmp/fr-m4u-full-check.log`, including 131 project CLI scenarios, the packaged skill workflow and 311/311 capability coverage.
Strict verification passes in `/tmp/fr-m4u-spec-verify.json`: twenty-three fresh anchors and signature maps, zero obligations and 36 Lean build jobs.
All six final documentation suites pass in `/tmp/fr-m4u-docs-final.log`; formatting and diff checks pass.

M4t's model and executable build with warnings treated as errors in `/tmp/fr-m4t-lean-build.log` (36 jobs).
All 1,570 correspondence states pass in `/tmp/fr-m4t-correspondence.log`; the three existing digest regressions pass in `/tmp/fr-m4t-focused.log`.
The complete theorem axiom audit is `/tmp/fr-m4t-axioms.log`, generated by `/tmp/fr-m4t-axioms.lean`.
The full native/WASM gate passes in `/tmp/fr-m4t-full-check.log`, including 131 project CLI scenarios and 311/311 capability coverage.
Strict verification passes in `/tmp/fr-m4t-spec-verify.json`: twenty-three fresh anchors and signature maps, zero obligations and 36 Lean build jobs.
All twenty-one evaluator regressions pass in `/tmp/fr-m4t-harness.log`; Python compilation, formatting and prose budgets also pass.
All six final documentation suites pass in `/tmp/fr-m4t-docs-final.log`; diff checks pass.

M4s passes the full native/WASM gate in `/tmp/fr-m4s-full-check.log`, including 131 project CLI scenarios and 311/311 capability coverage.
Strict verification passes in `/tmp/fr-m4s-spec-verify.json`: twenty-three fresh source anchors and signature maps, zero obligations and 31 Lean build jobs.
The three new digest regressions pass in `/tmp/fr-m4s-focused.log`; all twenty-one evaluator regressions pass in `/tmp/fr-m4s-harness.log`.
The final optimized CLI and development profiler build with locked offline dependencies in `/tmp/fr-m4s-build-final.log`.
The retained measurement passes the independent artifact audit in `/tmp/fr-m4s-audit.log`.
All six documentation suites pass in `/tmp/fr-m4s-docs-final.log`; formatting, Python compilation, prose budgets and diff checks also pass.

M4r's optimized CLI and profiling example build with locked offline dependencies in `/tmp/fr-m4r-build.log`.
Example clippy passes in `/tmp/fr-m4r-clippy.log`; all twenty evaluator regressions pass in `/tmp/fr-m4r-harness.log`.
Rust formatting and Python compilation pass. The two retained measurement reports pass complete payload, input-digest and statistics audits.
The default acceptance entry point passes in `/tmp/fr-m4r-acceptance.log`; all six documentation checks pass in `/tmp/fr-m4r-docs.log`.
M4r added a profiling example and evaluator tooling; production library code and Lean definitions retained M4p's validated behavior at that point.

M4q's eighteen measured queries and both invalidation probes pass in `/tmp/fr-m4q-project-cache-final.json`.
The retained artifact audit checks every payload digest and byte count, recomputes summaries and verifies the binary and measurement-source hashes.
All nineteen evaluator regressions pass in `/tmp/fr-m4q-harness-tests.log`; Python compilation also passes.
Twelve cache regressions and six documentation checks pass in `/tmp/fr-m4q-validation.log`.
The default acceptance entry point passes in `/tmp/fr-m4q-final-check.log`; final documentation checks pass in `/tmp/fr-m4q-docs-final.log`.
M4q left production Rust code and Lean definitions unchanged from M4p, which retained the full native/WASM gate and strict Lean verification.

M4p's focused source tests pass in `/tmp/fr-m4p-focused.log`.
The full native/WASM gate passes in `/tmp/fr-m4p-full-check.log`, including all 131 project CLI scenarios and 311/311 capability coverage.
Strict verification passes in `/tmp/fr-m4p-spec-verify.json`: twenty-three fresh anchors and signature maps, zero obligations and 31 Lean build jobs.
The axiom audit uses `/tmp/fr-m4p-axioms.lean` and writes `/tmp/fr-m4p-axioms.log`.
Final documentation checks pass in `/tmp/fr-m4p-docs-final.log`; formatting, prose budgets and diff checks also pass.

M4o's focused project and skill tests pass in `/tmp/fr-m4o-focused.log`, covering 131 project scenarios and 36 executable examples.
The full native/WASM gate passes in `/tmp/fr-m4o-full-check.log`, retaining 311/311 capability coverage and 29 Lean build jobs.
Strict verification passes in `/tmp/fr-m4o-spec-verify.json`: twenty-two fresh anchors and signature maps, zero obligations and a successful Lean build.
The final-binary controlled lookup comparison is `/tmp/fr-m4o-find-context-final.json`; documentation checks pass in `/tmp/fr-m4o-docs-final.log`.
M4o added no new source-budget proof; M4p extends that historical boundary with the model and comparisons above.

M4n's skill workflow and documentation tests pass in `/tmp/fr-m4n-validation.log`, including 37 executed examples and both root-refusal checks.
Final documentation checks pass in `/tmp/fr-m4n-docs-final.log`.
The reading measurement passes in `/tmp/fr-m4n-skill-context.json`; the skill validator, Python compilation and prose checks also pass.
M4n changed no production code; its recorded validation remains historical evidence.

M4m passes the full native/WASM gate in `/tmp/fr-m4m-full-check.log`, including all twelve check CLI scenarios and 33 skill examples.
Capability coverage remains 311/311. Strict verification passes in `/tmp/fr-m4m-spec-verify.json`.
It retains twenty-two fresh source anchors and signature maps, zero obligations and 29 Lean build jobs.
The controlled report passes in `/tmp/fr-m4m-checks-context-final.json`, with the final binary digest checked before and after execution.
Documentation checks pass in `/tmp/fr-m4m-docs-check.log`; formatting, prose budgets and the skill validator also pass.
These are presentation and execution regressions, not a new formal proof of process behavior.

M4l exact token auditing passes in `/tmp/fr-m4l-token-audit.json`.
The opt-in four-patch workspace replay passes in `/tmp/fr-m4l-workspace-replay.log`.
Default acceptance and documentation checks pass in `/tmp/fr-m4l-evidence-check.log`.
They retain sixteen harness regressions, eight earlier patch replays and the controlled history comparison.
Prose budgets remain 272 long sentences and 9,335 Rust comment lines, with other tracked categories at zero.

Use cached dependencies with `CARGO_HOME="$PWD/target/cargo-home"` and `CARGO_NET_OFFLINE=true`.
`tools/check.sh` runs the native/WASM gate; `fr spec verify kernels --json` checks strict source correspondence and Lean builds.
The separate deep audit remains optional unless a change warrants it.
Scoring uses `target/agent-eval-venv/bin/python`, tiktoken 0.12.0 and a checksum-pinned o200k_base vocabulary.
Reference tokens count the prompt and visible instrumented payloads once, excluding system context, hidden reasoning and billed usage.

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-08-regex
python3 tools/agent-eval.py replay tests/agent-eval/results/2026-09-08-regex
CARGO_HOME="$PWD/target/cargo-home" CARGO_NET_OFFLINE=true cargo test --test agent_acceptance recorded_workspace_patches_pass_checks_oracles_and_exact_reversal -- --ignored
```

Workspace replay needs the pinned regex dependencies. Follow the bootstrap in the workspace report on a cold machine.
The integration regression is opt-in so default CI does not acquire this additional dependency requirement.

## Next steps

The agreed real-agent baseline is local `codex exec` with `gpt-5.6-luna`, `low` reasoning and the default service tier.
Routine CI keeps deterministic replay and does not consume agent quota; real-agent smoke pairs and cohorts are explicit authenticated runs.
Each fresh trial must ignore user configuration, retain Codex JSONL events and record the CLI catalog entry and complete model settings.
A small Terra or Sol calibration is reserved for milestones where Luna failures could otherwise conflate model capability with workflow usability.
Check execution metadata is now optional; use retained traces to reduce remaining project/transaction metadata and skill-loading costs.
Preserve coverage, source bases, guards and reviewable edits.
Validate whether fresh agents adopt the targeted route before claiming autonomous context savings; include a task that actually requires broader exploration.
Measure proposed reductions on fixed transcripts or controlled workflows before requesting another autonomous cohort.
Reference serialization and hashing remain the largest measured construction cost after batching; evaluate further changes against this profile.
Keep revision inputs, coverage and final source verification intact; require byte-identical reports and distinguish model proofs from implementation correspondence.
State the cache policy for the next autonomous cohort; existing trials explicitly disable it and their records remain immutable.
The latest paired task requires one coordinated three-operation change across two crate roots.
Both arms pass, while `fr` uses 33.3% more measured context and 22 more calls in this single pair.
Use its refused path guesses, help calls and repeated inspection to simplify authoring discovery
before spending quota on another autonomous cohort.
Keep portable skill references selective and executable against the distributed binary.
PR 7 merged as GitHub PR 267. PR 8 is ready for review on `agent_workflow_v4`; after merge,
choose the next M2 context-efficiency or M4 authoring outcome in [PLAN.md](../PLAN.md).
