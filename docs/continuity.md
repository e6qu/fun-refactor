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

The [study planner and auditor](agent-study.md) now freeze task matrices and audit declared usage,
including child agents and failed attempts. The host CLI now provides a budgeted provider gateway
and private black-box grading in constrained Docker containers. A serial agent loop connects provider
tool calls, pinned Git exports, isolated commands and grading for fix/feature tasks. Child agents
receive a focused question and workspace copy; their edits never merge automatically.
Both arms also have a bounded source-read tool. Its content hashes detect stale continuations;
retained byte ranges measure repeated disclosure within and across agents. The auditor recomputes
trace counters. Any command keeps total source reads unknown while preserving measured page counts.
A frozen resource profile can now place all command and grader containers in a fresh Linux cgroup.
Kernel CPU and memory evidence remains separate from the unmeasured host/daemon, disk and cache costs.
Resource stops fail the attempt and retain evidence; complete worker accounting is still open.
Study CI uses fake providers and executable submissions. A separate guarded
[OpenCode rehearsal](opencode-rehearsal.md) now supports local Kimi/GLM protocol and small task checks.
It retains CLI accounting separately. The [explanation adapter](opencode-source-evidence.md) adds
three pinned Python projects with factual and retrieved-source checks; this is a bounded rehearsal,
not the independent pilot. The [native tool adapter](opencode-native-tools.md) tests the same tasks
through OpenCode tool calls, avoiding the one-JSON-action response requirement.
Select independent tasks, extend proof grading, and collect complete context/tool
and system resource measurements before the Sonnet/Luna pilot.
The existing Codex CLI path still cannot enforce the pilot dollar cap.
Use those results to simplify overlapping routes and choose useful analysis
improvements. The older two-task comparison consumed more calls, context and time with `fr`;
general efficiency remains unproven. Test general language rules on unfamiliar projects; do not
special-case the two repositories used in the recent scripted delivery tests.

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
