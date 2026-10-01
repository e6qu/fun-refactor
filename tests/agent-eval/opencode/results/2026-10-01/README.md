# OpenCode local rehearsal, October 1, 2026

Eight frozen task attempts used OpenCode 1.18.34 with the existing Kimi K3 and GLM 5.3 Flash
coding-plan profiles. Four passed the original graders, three stopped at the 45-second subprocess
deadline, and one completed with syntax the original local grader refused. No resource limit was
raised and no matrix cell was retried. Each invocation ran serially through the workstation guard.

| Model | Task | Arm | Original outcome | Wall seconds |
|---|---|---|---|---:|
| GLM 5.3 Flash | Interval boundaries | Files | Passed, 812 checks | 46.4 |
| GLM 5.3 Flash | Interval boundaries | fr available | Passed, 812 checks | 35.3 |
| Kimi K3 | Stable deduplication | Files | Response deadline | 58.0 |
| Kimi K3 | Stable deduplication | fr available | Grader syntax refusal | 52.4 |
| GLM 5.3 Flash | Stable deduplication | Files | Passed, 18 checks | 63.1 |
| GLM 5.3 Flash | Stable deduplication | fr available | Response deadline | 54.4 |
| Kimi K3 | Interval boundaries | Files | Response deadline | 53.2 |
| Kimi K3 | Interval boundaries | fr available | Passed, 812 checks | 32.6 |

The table follows the frozen execution order. Wall time includes preceding completed responses,
tool operations and final export/grading, so a response deadline can occur after 45 total seconds.
The largest sampled subprocess-group RSS was 654.2 MiB. This excludes the Python controller and
global OpenCode state; the separate workstation guard enforced its aggregate 1 GiB sampled limit.
The installed binary was `fr 0.35.0`; the plan retains its exact SHA-256 identity.

**None of the agents requested an fr operation.** Both arms used ordinary list/read/replace actions.
These tiny fixtures therefore provide no evidence of fr usage or savings. The available fr routes
were map/find/show only; the experiment does not cover the full skill, edits, delegation or proofs.
The tool routes also passed a separate local smoke check against the installed binary.

OpenCode reported zero cost for every complete response, but actual billed cost remains unknown.
The JSON report preserves input, output, reasoning and cache counters in their CLI schema. Missing
responses and incomplete attempts remain visible. No general efficiency or model ranking follows
from these small single-attempt comparisons or their time differences.

## Grader review

The original sequence grader's safe syntax subset excluded `try` and `hash`, causing Kimi's
completed submission to refuse before behavior checks. The reviewed grader admits those pure
operations and distinguishes syntax refusal from execution failure. It also adds equality cases
crossing hashability: a set and a frozenset with the same contents compare equal and must deduplicate.

The separate [grader review](grader-review.json) runs only on completed retained submissions and
makes no model requests. Kimi's implementation keeps hashable and unhashable values in separate
buckets and fails the new equality cases. GLM's completed implementation passes all 24 reviewed
sequence checks. Interval results remain passing. Original grades and transcripts are unchanged.
These are post-hoc findings; they are not a new prospective comparison or a retroactive rewrite
of the frozen outcomes. Timed-out attempts remain ungraded.

## Retained records

- `plan.json` freezes sources, instructions, grader hashes, models, binary and execution order.
- `attempts/` retains all eight records, submitted source bundles, CLI streams and available session exports.
- `report.json` is the offline audit, including requested fr/ordinary actions and formatting deviations.
- `frozen-runner/` retains the five bound implementation modules and the original private graders.
- `reviewed-graders/` and `grader-review.json` retain the later grader revision and its separate results.
- `preflights/ready/` retains the initial Kimi JSON echo and local session export.
- `preflights/forced-summary/` retains the failed one-step Kimi task preflight.
- `preflights/prose-before-json/` retains the failed strict-JSON GLM task preflight.

The two failed task preflights preceded the final frozen matrix. Their raw artifacts and original
plan identities remain available; their intermediate runner implementations are not retained as
replayable source snapshots. They must not be silently folded into successful matrix attempts.
All eleven OpenCode sessions remain represented. Provider billing and internal retry accounting
remain unknown across the entire rehearsal.

Audit with `tools/opencode-rehearsal.py report plan.json attempts` through the workstation guard.
The offline test suite rechecks the report, bound runner snapshots and reviewed grades without
OpenCode, network access or credentials. The larger independent study remains open.
