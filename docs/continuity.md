# Development continuity

The current `spec_extract` branch is roadmap PR 0, the agent-ready verified refactoring foundation.
It packages the completed source-history, project-inspection, Git, authoring, agent-skill, evaluation and Lean-model work that PRs 1 through 6 build on.
PR 1 is the next delivery unit after PR 0 lands.
M4ac is complete: the matched check-output projection shows that verbose successful logs masked substantial fr workflow context in M4ab.
The user authorized publishing PR 0 from this branch.
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
Next, project reductions for repeated project inspection and authoring/history metadata before changing production output.

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
The rehearsal remains prescribed infrastructure evidence; the coordinated cohort above supplies the later autonomous measurements.

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
The coordinated workspace preparation above follows this controlled comparison; autonomous evaluation remains open.

## Coordinated authoring batches

`src/project/author.rs::author_batch` reads a bounded JSON manifest and plans each existing operation against the same captured project.
The manifest accepts 1 through 32 entries with `op`, `handle` and `from`; optional `revision` supports short IDs and validates full-handle batches too.
Unknown fields and operations refuse. All input paths resolve from the workspace root; input files retain the regular-file and 64 KiB guards.
Selected original regions must be disjoint even for no-ops; insertion points cannot share or touch another region's boundary.
Every step must succeed, and combined file results must reparse before the CLI records one source-history transaction.

`src/cli.rs::cmd_author` routes batches through the existing diff, persistence and source-verification path.
Schema `fr-author-batch-1` reports coverage once, ordered step signatures and original spans, sizes and hashes.
It omits after-spans because earlier edits can shift later positions. Insertion hashes include separator bytes.
Saved plans freeze all changes; unrelated source changes retain the existing history rules, and affected-file conflicts refuse the entire application.
This does not strengthen filesystem atomicity or prove typing and behavior.

Seven new CLI scenarios include a compiled two-file caller/signature/helper change through save, apply, undo, patch checking and redo.
Mixed Rust/Go/TSX operations, shared revisions, no-op batches, size/count limits, overlap refusal and malformed inputs have regression coverage.
A two-edit batch also matches the Rust and Lean splice implementations, with Unicode, CRLF and different replacement lengths.
This is tested edit correspondence, with no new proof of the batch planner, manifest parser or filesystem transaction implementation.
The agent reference teaches combined review and one transaction ID; its existing shell examples remain unchanged.
The controlled comparison above measures report bytes and repeated calls without treating a prescribed workflow as autonomous agent evidence.

## Formal module insertion placement

`src/project.rs::module_insertion_offset` extracts the existing placement calculation from `insert_declaration` without changing its behavior.
The caller still selects the exact inline module and passes the prefix before its closing brace plus the opening-brace offset.
`kernels/FrKernels/Author.lean` models the result through character lists and UTF-8 byte widths.
Its fifteen theorems cover bounds, boundary validity, placement after an opening brace within the input, and an indentation-only suffix.
They characterize a trailing indented line, inline content, out-of-body line candidates and empty input.
A source anchor and explicit signature map identify the helper. Theorems use only `propext`, `Classical.choice` and `Quot.sound`.

`ProjectMain.lean` adds `module-offsets` for the generated corpus and `module-offset BODY_START TEXT` for a selected case.
The default kernel gate runs the corpus through the existing project executable.
Two new integration scenarios compare 28,185 cases on 64-bit hosts with both Rust and a reverse-scan oracle, and check eight CLI previews.
The corpus includes Unicode, CRLF, rejected indentation lookalikes, NUL in the pure helper, machine limits and large prefixes.
A 32-bit host compares 25,371 representable cases.
Each generated placement also passes through the Rust edit engine, with unchanged prefix and suffix checks.
Existing authoring cases retain their behavior and transaction evidence.
This proves model properties and tests correspondence; AST selection, parsing, name checks and full authoring refinement remain unproved.
See [placement kernels](lean-specs.md#module-insertion-placement-kernels) for assumptions and reproduction.

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

## Inline module insertion

`src/project/author.rs::insert_declaration` accepts a file or exact Rust inline module handle.
It locates the module by its name span and inserts at the selected declaration list's closing brace.
A closing brace on a whitespace-only line keeps that indentation; inline braces receive a leading separator.
Fragments remain verbatim, preserving multiline strings. Existing source bytes stay unchanged.
Duplicate-name and dangling-outer-metadata checks apply to the selected body's direct items.
External modules, impls, traits and functions refuse; file insertion retains its existing behavior and report fields.
Module reports add `container` with the original body span including braces and a module-scoped name-check description.

Five new CLI scenarios cover exact placement, raw identifiers, same-named modules, size/hash boundaries and stale selections.
A nested function calls a private sibling after saved application and redo; undo restores exact original bytes and the patch applies after undo.
The saved transaction retains its fragment despite later changes to the input file.
Handle validation covers the project revision; later history application checks affected files and permits unrelated manifest changes.
A reported nested-module edit is compared with the existing Rust and Lean splice implementations.
This is tested splice correspondence, with no new proof of AST selection, parsing or complete authoring behavior.
The portable authoring reference explains module-row handles, preserved fragment contents and unsupported scopes.

## Wrapped function authoring

`src/project/author.rs::function_initializer` follows the expression operand through parentheses, `as`, `satisfies`, postfix `!` and TypeScript angle-bracket assertions.
It accepts only arrow, ordinary function-expression and generator-expression terminals; replacement still requires a block body.
Comments do not count as operands. The angle-bracket form skips its type-argument child and follows the value expression.
Calls, conditionals, comma expressions and other initializer forms refuse, even inside an otherwise supported wrapper.
The existing handle/name check prevents selecting an enclosing or neighboring function.

The resulting edit preserves wrappers, types, comments and every byte outside the selected braces.
The existing report schema, source-basis guards, bounded diffs, save/apply path, undo/redo and patch handling remain in use.
Its signature stays a header excerpt ending before the body; inspect selected source for postfix assertion types.
The portable authoring reference names the accepted wrappers and uses `--locals` lookup for variable handles.
Its executable command examples stay unchanged; current skill text is separate from historical context measurements.

All thirty-four authoring scenarios pass, including shadowed wrapped bindings and stale handles/plans after a wrapper changes.
Four compiled history fixtures check lexical `this`, named recursion, generators and JSX before/after application, undo and redo.
They also freeze the saved fragment and check patch applicability after undo.
The Rust/Lean edit comparison now includes a reported wrapped TSX replacement with Unicode, CRLF, external comments and neighboring source.
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

The skill checker executes 36 examples. The Rust wrapper example compiles under `deny(missing_docs)` and passes a compiled caller assertion.
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

All sixteen autonomous trials pass. Recording also supports scored failures; behavioral replay refuses failed trials.
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
A later paired task should require coordinated changes across files; the current larger repository task is still a localized facade addition.
Keep portable skill references selective and executable against the distributed binary.
Further authoring operations, M5 automated Lean adoption and M6 framework migrations remain open in [PLAN.md](../PLAN.md).
