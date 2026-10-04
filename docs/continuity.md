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
Post-merge deep validation is still pending at this handoff.

The current change adds [narrow source-packet reviews](../tests/agent-eval/opencode/reviews/2026-10-04-packets/README.md).
One Kimi review completed; two following calls timed out and the remaining three were stopped.
Do not resume this collection or raise its limits. GLM submitted before timing out, but its session
audit did not finish, so it remains failed. The completed finding targets recursive interpolation
accepted by the old dotenv grader. GitHub must check the same wrong repair against old and new
graders, plus the reference repair. Full task review and agent-efficiency comparisons remain open.

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
