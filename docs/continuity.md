# Development continuity

This is the short contributor handoff. The [roadmap](../PLAN.md) owns unfinished outcomes; the
[changelog](../CHANGELOG.md), [defect ledger](../BUGS.md), retained evaluator manifests and Git
history preserve completed detail.

## Baseline

The current system provides:

- Bounded project, semantic, application and evidence models for 19 parser identities.
- Merkle-addressed progressive disclosure and a Python object-store protocol.
- Structured goals, deterministic guide routes and one uniform writable `GuideReview` lifecycle.
- Built-in refactors, recipes, semantic and surface edits, application migration and proof work.
- Checks, history, apply, undo, redo, recovery, patches and reviewed Git operations.
- Lean admission and transition kernels with strict source anchors and shared executable cases.
- A portable agent skill and zero-dependency typed Python runtime.

Use `fr audit` and `fr capabilities` for live counts and support. A supported route can still refuse
an input outside its syntax, identity, confidence or effect contract.

## Active outcome

PR #453 merged as `410482ce` after all final checks passed. Two SDK jobs needed a
retry because GitHub could not acquire runners; neither failed job had executed a
step. The earlier guide-size and excerpt-anchor failures are fixed. Local main is
refreshed, including release v0.55.0. The branch is `eval/configured-change-pilot`.
The previous merge's deep validation `37607221053` passed.

The approved deliverable is the four-attempt packaging change pilot described in
[PLAN.md](../PLAN.md). Resource admission is established with the small per-process
catalog and pinned OpenCode client. Authentication remains with OpenCode and the
existing `gh` login; do not inspect, copy or upload credential files.

The scoped collection in `reviews/2026-10-07-packaging-scoped` was frozen at
`8b42548e` before calls. It assigns eleven areas to three questions and two models.
Five reviews completed, one failed, and no cell hit a resource limit. Peaks were
442–576 MiB. GLM's policy answer used 2,046 reasoning tokens and two other output
tokens, exhausting the unchanged 2,048-token allowance. All six cells are retained,
and `stop.json` permanently closes the collection.

The generic coverage reporter shows seven areas with both model submissions.
All eleven have at least one submission. Findings, limitations, failures and missing
submissions remain visible; the report never infers task acceptance.
Kimi's object/API review repeated the incorrect postrelease claim outside its scope.
`assessment.json` rejects it against identical source and previously retained GitHub
grading, while preserving the original completed answer and the failed policy review.

Kimi made one fr call during policy review. Exact `Specifier.contains` returned
no rows; two ordinary searches and two ordinary reads followed. No useful fr source
was delivered. The public exploration guide now explains bare declaration names.
This is observed attempted use, without an efficiency claim.

Hosted workflow `37609277041` passed the protocol and all 324 policy combinations.
`policy-verification/` retains its grader, result and artifact provenance; the raw
archive matched GitHub's SHA-256 digest. `admission.json` assesses all eleven areas
and admits the task for one bounded pilot. The failed review remains failed.

The next runner uses terminal structured summaries, frozen configured profiles and
the existing edit machine. Offline tests cover exact submissions, forged evidence,
stale edits, resource stops and settings. `terminal-changes.yml` will exercise both
models and both arms through the pinned real client on Linux and macOS without paid
model calls. Retain those results and run one guarded workstation control before
freezing the four live attempts. Candidate execution stays on GitHub; all four live
attempts remain unstarted.

The first adapter controls passed all five Linux cases, but macOS admission failed:
the ordinary control peaked at 571.14 MiB and the fr control at 739.92 MiB. Both
completed below the 768 MiB cap, but the fr case exceeded the unchanged 640 MiB
admission threshold. Three macOS cases remain unstarted. Original artifacts and
offline replay live in `changes/2026-10-08-memory`. Hosted run `37744454024` adds
process attribution; its Linux controls passed and macOS stayed queued. New plans
freeze Bun's low-memory option; historical plans keep their original settings.
Linux run `37745425901` passed with this option, without establishing a reduction.
GitHub is retiring macOS 14, so active workflows now use macOS 15. Hosted run
`37746127031` tests that combination. Local pilot calls remain blocked pending
macOS admission; no resource limits increased.

Historical memory and review failures stay stopped. The catalog and low/max
transport evidence replay offline in `memory/2026-10-07-providers` and
`reasoning/2026-10-07-low`. The whole-task output failures and rejected partial
postrelease claims remain in `reviews/2026-10-07-packaging-low`. The earlier
distinct-object grader repair is retained in `reviews/2026-10-06-configured`.

Use one new frozen plan for an admitted packaging comparison: two configured
models, ordinary tools and optional fr, identical public feedback, independent
GitHub grading, no retries. Preserve failed attempts and unknown costs. Full builds
and gates remain on GitHub. The roadmap stays at 7 of 18 demonstrated technical
items, with all four milestones open.

## Completed disclosure and terminal-answer work

PR #440 merged as `8075e69e` after all 21 PR checks passed. Its hosted submission job took
68 seconds, including both control sets, 24 offline tests and fr authoring of the extracted function.
Independent artifact replay accepts both valid source reviews and preserves missing, duplicate and
interrupted submissions as failures. The adapter remains OpenAI-compatible only. No live model calls
or general efficiency claims follow from these controls. Historical stopped collections stay stopped.

PR #441 repairs a concrete CLI discovery failure.
A generated fixture with a long file scope and matching declarations makes the released CLI exceed
its response budget and return no rows. The change shrinks names, source and relationship pages
within the existing limits. It counts final context metadata and preserves explicit expansion positions.
The new hosted tests check complete traversal, exact source and relationship reconstruction, UTF-8
boundaries, and stale-cursor refusal. Full builds and evidence regeneration remain on GitHub.

The index refresh passed in 7m11s, including all ten pagination tests. Its eight samples reproduce
the pinned baseline and all 195 bindings match. Repository delivery passed in 1m54s; its imported
bundle passes all 365 behavior cases and all 227 bindings match. The exact-location refresh also
passed: nine recorded cases, eleven semantic links, zero false claims and 33 matching bindings.
The roadmap again shows 7 of 18 current technical items; all four milestones remain open.

Hosted evidence: [index and pagination](https://github.com/e6qu/fun-refactor/actions/runs/37424349654),
[repository delivery](https://github.com/e6qu/fun-refactor/actions/runs/37424358148), and
[exact locations](https://github.com/e6qu/fun-refactor/actions/runs/37425184269).
PR #441 CI exposed a Clippy comparison warning and a 17-byte skill-guide overrun. Both are fixed
without changing limits. The guide is 4,025 bytes. The focused hosted gate now checks Clippy and
skill budgets before pagination and index replay; it passed in 9m18s on `4c09e07a`.
Replacement evidence under `2026-10-06-explore-ci-*` passes independent audit with the same outcomes
and current bindings. See [lint and index validation](https://github.com/e6qu/fun-refactor/actions/runs/37435891865)
and [repository and location validation](https://github.com/e6qu/fun-refactor/actions/runs/37435904795).
All 21 final PR checks passed. PR #440 release, Pages and deep validation all passed.

## Completed recipe audit repair

The user merged release PR #434 at `eae73262` (0.52.0). PR #435's 20 PR checks passed,
but its post-merge full-workspace recipe replay timed out. The current branch removes
unnecessary indexing for unchanged recipe steps and repairs single-file preview paths
found while dogfooding. Full builds, Rust tests and evidence refreshes run on GitHub.
Repository-delivery, recovery and index evidence has been refreshed and verified against current
source bindings. The complete [deep run](https://github.com/e6qu/fun-refactor/actions/runs/37301872179)
passed all 17 jobs. Its recipe test took 303.91 seconds and the whole job took 6m52s.
The unmodified rerun took 777.19 seconds and 14m47s respectively. These are two runner samples
with identical coverage and deadlines. The repair is merged; submission controls are the active work.

GitHub [37301881349](https://github.com/e6qu/fun-refactor/actions/runs/37301881349) completed
both scripted repository tasks with 365 behavior cases on `c272d5fb`. The imported bundle
matches 227 source bindings. Recovery run
[37304200800](https://github.com/e6qu/fun-refactor/actions/runs/37304200800) passed on
`ededba2a`, after updating its source-bound runner reference. Its 18 bindings match,
with 430 handled-failure and 430 process-exit boundaries and 224 model cases.
The roadmap retains seven demonstrated items; no broader claim follows from this refresh.
Index run [37303849906](https://github.com/e6qu/fun-refactor/actions/runs/37303849906) passed
in 8 minutes 23 seconds. Its eight samples reproduce the pinned baseline's complete symbol
and reference answers; all 195 source bindings match the current tree.

Four other active reports also bound `src/cli.rs`. GitHub refreshed
[guided edit accounting](https://github.com/e6qu/fun-refactor/actions/runs/37310459036),
[intent edit accounting](https://github.com/e6qu/fun-refactor/actions/runs/37310483336),
[seven completion workflows](https://github.com/e6qu/fun-refactor/actions/runs/37310763209), and
[retained proof delivery](https://github.com/e6qu/fun-refactor/actions/runs/37310785657)
against `1acf1294`. All passed, and their 54 recorded source bindings match the current tree.
Active tests now select the new reports; historical reports remain unchanged. These scripted
checks do not establish a live-agent efficiency advantage. The [CI guide](ci.md) lists all seven
refresh groups and explains the recovery runner's own source binding.

The first PR gate also exposed a race in the fake-provider budget test. It now holds
the winning reservation until the competing admission is refused. All 14 gateway tests
and 20 repeated contention cases passed locally under the guard. The single-file patch
regression has its own target so the historical author runner stays byte-identical.

The goal is a general CLI that helps agents understand unfamiliar code, make changes and fixes,
and write and check useful proofs with less total effort and context. Start with the
[product review](product-review.md) and the plain-language [plan](../PLAN.md).

PR #425 retained source-based candidate reviews and added failed-attempt accounting for native
source reads. Its 20 current CI checks passed before squash merge at `b1525faf`.
The [source-reading report](native-read-outcomes.md) has two passes and four timeouts, with no
successful ordinary/fr pair. The [code-change report](native-change-outcomes.md) has ten behavior
passes and two timeouts; none used fr. Complete context and billing remain unknown.
Two [source-based review attempts](../tests/agent-eval/opencode/reviews/2026-10-04-native/README.md)
timed out after 17 tool calls without findings. Do not restart their four stopped cells or raise limits.

PR #427 merged at `3c1d2008` after all 20 CI checks passed. It removes repeated output during
`project explore` pagination. Source follow-ups omit relationships; relationship follow-ups omit
source. Native schema 5 exposes these views; older schemas retain their original behavior.
Its post-merge deep validation passed.
Release creation initially failed after main advanced past the release commit. A temporary branch
at `38413ce6` restored an existing ref for that commit. Retrying run `37190700691` created the
`fun-refactor-v0.49.2` tag at that exact revision; cleanup then removed the recovery branch.
All release artifact builds passed as well.

PR #428 merged at `feadf1cb` after all 20 checks passed. Its post-merge deep audit also passed.
It added [narrow source-packet reviews](../tests/agent-eval/opencode/reviews/2026-10-04-packets/README.md).
One Kimi review completed; two following calls timed out and the remaining three were stopped.
Do not resume this collection or raise its limits. GLM submitted before timing out, but its session
audit did not finish, so it remains failed. The completed finding targets recursive interpolation
accepted by the old dotenv grader. GitHub job `111408265330` verified the same wrong repair passes
the old grader and fails only the new case. The reference passes. The collection retains both grades
in `counterexample.json`; full task review and agent-efficiency comparisons remain open.

PR #430 merged at `db850bc3` after all 20 final checks passed. It compresses eleven historical
flow-cache reports, saving 366.6 MiB in an
expanded checkout. The [restoration guide](../tests/agent-eval/EVIDENCE-ARCHIVES.md) records exact
hashes and original Git blob IDs. The latest acceptance report and its source-bound evaluator stay
unchanged. The conversion preserves all samples and failures; no roadmap acceptance item closes.
Release PR #429 merged at `ced10cb8`; its post-merge release workflow passed. Its earlier PR runs
contained no jobs and supplied no test evidence. PR #430's post-merge release workflow passed;
deep validation and Pages also passed.

The current [reference reviews](../tests/agent-eval/opencode/reviews/2026-10-05-references/README.md)
were frozen at `6d3b5c79` with proposed reference source explicitly disclosed. All six calls finished:
three completed Kimi reviews and three GLM timeouts. No retries or larger limits were used. Two
reviews found no scoped issue. Kimi's packaging finding was wrong about exclusive post-release
comparison; exact source and a retained GitHub control artifact refute it. Keep the original finding
and separate rejection. The graders and references are unchanged; full task acceptance remains open.

Kimi's only fr request was refused because behavior mode lacked a handle. New native schema 6,
selected with `native-rehearsal.py freeze --recovery-hints`, returns a names query after that refusal.
It preserves selectors and performs no automatic extra call. Schemas 1–5 and historical reports
remain unchanged. Scripted recovery and replay pass; no live recovery benefit has been measured.
PR #431 also lets native/static jobs queue independently of the standalone Zig cache check.
Each job retains its own pinned, checksum-verified toolchain installation and existing deadline.
The [CI guide](ci.md) records the observed queue delay and cold-cache tradeoff; no test is removed.

PR #431 merged at `0fd64365` after all 20 checks passed. Its post-merge deep validation, release and
Pages workflows passed. The user merged release PR #432 at `3a66e140`; this branch incorporates it.
Its original PR workflows had no jobs, so they provide no test evidence. Post-merge checks are tracked
separately from that empty run.

The [boundary reviews](../tests/agent-eval/opencode/reviews/2026-10-05-boundaries/README.md) were frozen
at `2ef0d354` with native schema 6. Both dotenv calls timed out without submissions, stopping the four
remaining cells. Never resume this collection. There were 16 calls and 44,357 tool-result bytes;
complete usage and billing remain unknown. The lone fr call used a nonexistent path; it did not
exercise handle recovery. No independent task-review gap closes.

`native_costs.read_identity_reuse` replays read-only traces and adds a separate exact-file overlap
audit. `source_reviews.source_reuse` binds it to a frozen collection and retains unstarted cells.
It detects 12,310 bytes repeated across paths beyond existing counters in the two failed attempts.
Old reports and counters remain unchanged. File identities do not imply equivalent module behavior.
PR #433 merged at `61f0ec9a` after all 20 checks passed. PR #432's post-merge workflows also passed.

The [single-assertion reviews](../tests/agent-eval/opencode/reviews/2026-10-05-assertions/README.md)
were frozen at `c938151b` before calls. Three reviews completed: both models reviewed two-object
identity and Kimi reviewed one rejected directory. All found no scoped contradiction. GLM's next
response omitted the submission tool; Kimi's following attempt submitted four times. Those two
failures stopped the final GLM cell. Never resume this collection or promote its readable failed
responses to completion. All attempts finished within budget; none called fr.

`source_packets.compare` records selected before/after file hashes, lengths and executable flags.
`check_comparison` recomputes them from frozen bytes, rejecting forged or stale metadata. This adds
context to new packets without changing historical reports or asserting equivalent path behavior.
The [requirement review table](candidate-review-status.md) names the remaining gaps. Next address
reliable native submission with scripted checks before allocating further frozen model calls.

GitHub run [37163442730](https://github.com/e6qu/fun-refactor/actions/runs/37163442730) refreshed
the location and repository evidence against `20688a31`. It checked 11 semantic links with zero
false claims and 365 repository behavior cases. The imported bundles match 259 current source
bindings; the roadmap again has seven demonstrated items, with all four milestones open.
Run [37166142952](https://github.com/e6qu/fun-refactor/actions/runs/37166142952) passed all seven
pagination tests and refreshed index evidence against `7517d5d6`. Its eight symbol/reference
comparisons match the pinned baseline, and the imported result matches 194 current source bindings.

Next finish independent task and grader review, then compare public edits
and test feedback on unfamiliar tasks. Keep GitHub container grading separate from local OpenCode
rehearsals. Retain every failure and complete parent/child costs before claiming an efficiency gain.
The [study host](agent-study.md) supports constrained commands, provider reservations, private
grading and bounded child work. Full worker/context costs and source-connected proof grading remain
open. Its fake-provider tests do not count as live trials; the older Codex CLI cannot enforce the
pilot dollar cap. The [product review](product-review.md) records the evidence and removal criteria.

The representative guided delivery milestone is complete for its pinned acceptance corpus.
Accepted live trials cover upstream read/trace, multi-file Rust rename and body edits, frontend
changes, application migration and Lean proof authoring. Two matched cohorts repeat one scalar
source-writing task. The [evaluation guide](evaluations.md) records each result and its limits.

The active direction is agent analysis and task planning. The [architecture review](agent-analysis-review.md)
assesses the baseline at `c22ba827`. The [roadmap](../PLAN.md) defines pending outcomes and acceptance:

1. Exact occurrence locations, analysis explanations and task investigation.
2. Control flow, interprocedural value summaries and rule-based sources/sinks.
3. Dependency-aware reuse and resumable task plans.
4. Task-complete changes, translation and explicit proof obligations.

Exact declaration, relationship and flow occurrences now share typed source locations.
Python scalar summaries cover recursion, ordered expressions, explicit call binding,
literal defaults, regular packages, re-exports, module aliases and single-root namespaces.
Dependency-bound plans support interruption, invalidation, fresh target correspondence
and reviewed checked delivery. The roadmap retains the remaining milestone gates.

Use concrete tasks to expose missing reusable language behavior. Validate that behavior through
independent examples, explicit unsupported cases and unfamiliar projects. Preserve bounded reads,
verified saved data and edit review. Agent hypotheses remain distinct from tested outcomes.

Application IR expansion follows concrete task requirements. The four HTTP adapters read and write
their documented validation subsets. Selected FastAPI middleware/providers/service calls and
React/Next state/events have explicit contracts. Configured middleware, external effects and
broader validation behavior still need models and independent oracles.

## Recent completion

PR #392 added single-root namespace dependencies and partitioned the complete PR test
gate across native and SDK runners. Every successful shard retains its inventory and
capability log; the final gate checks complete assignment and combined coverage.
PR #393 completed ordered scalar assignments and exact target origins, with all 276 native
targets and 916 SDK cases assigned exactly once and 311/311 capabilities covered at that baseline.

PR #394 added the [roadmap status](roadmap-status.md) and fixed regression scenarios for boltons
and more-itertools. Those scripts exercise prescribed changes, interruption recovery, checks and
patch replay; they do not measure independent diagnosis or token savings. At that baseline, CI
covered 277 native targets, 916 SDK cases and 311 capabilities. Seven of 18 technical acceptance
items are demonstrated for their stated cases; eleven items and all four milestones remain open.
Keep the generated report current with `tools/roadmap-status.py --check`.

## Validation

The complete gate definitions are below. Run these workloads on GitHub for this workstation:

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

Rust test and Lean worker defaults are one. On the shared workstation, run local `fr`
commands and lightweight checks through `/Users/zardoz/.codex/tools/fr-local-guard.py`.
Keep full builds, complete gates and evidence regeneration on GitHub. The guard preserves
64 GiB free disk, limits target data to 2 GiB and sampled workload RSS to 1 GiB, and
stops work after 180 seconds. Move refused workloads to CI without raising these limits.
See [CI latency and coverage](ci.md) and the [development guide](development.md).

For repository changes, dogfood `fr`, recipes and reviewed history whenever an admitted operation
exists. Treat an incorrect preview, refusal or impractical flow as product evidence and fix its root
cause in the same outcome. Record direct editing only when no suitable `fr` operation exists.

## Durable boundaries

- Generated formalization covers admitted typed pure declarations and structural snapshots.
- Parsing, extraction, lowering, hashing, Git, filesystems and runtime behavior remain trusted or
  separately tested unless a proof record states a narrower correspondence claim.
- Application migration covers the documented reader/writer subsets. Static facts alone cannot
  establish arbitrary middleware, external service behavior or runtime effects.
- Accepted live trials establish their pinned outcomes. General bug diagnosis, feature discovery,
  security analysis and task resumption still need dedicated acceptance cases.
