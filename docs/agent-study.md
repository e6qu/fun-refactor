# Compare ordinary tools with fr on independent tasks

`tools/agent-eval-study.py` freezes a study plan and audits retained attempt records.
Task names and repository identities are data. No production analyzer rule depends on them.
The same format accepts different models, single agents and delegated work.

The auditor checks retained evidence. `tools/agent-eval-host.py` converts provider responses and
operates a shared budget ledger. The existing Codex launcher now bounds process resources.
These pieces have synthetic tests, but no new live trial has run. A complete paid pilot still needs
a host that gates every model request, isolated task checkouts and independent hidden graders.
The Codex CLI launcher does not enforce a dollar cap and must not run the planned pilot.

## Freeze the comparison

Create a JSON manifest with these fields:

| Field | Meaning |
|---|---|
| `schema` | `fr-agent-study-1` |
| `seed`, `repetitions` | Nonnegative random seed and positive repetitions per matched pair |
| `spend_cap_usd` | Positive total study cap, enforced by the host before launching more work |
| `cache_state` | `cold`, `warm` or `uncontrolled`; record the host's cache policy honestly |
| `max_children` | Total child limit per delegated attempt, including descendants and failed children |
| `budgets` | Positive `wall_seconds`, `aggregate_agent_seconds`, `aggregate_tokens`, `rss_bytes`, `disk_bytes` and `attempt_cap_usd` |
| `fr` | Released `version`, `binary_sha256` and `skill_sha256`, shared across both arms |
| `tasks` | Task objects described below |
| `models` | Model profiles described below |

Each task has a unique lowercase `id`, a `kind` of `explain`, `fix`, `feature` or `proof`,
and a `repository` identity. Pin its full Git `revision`, exact `requirement` text and hidden
`grader_sha256`. Set boolean `held_out` and `delegated` flags. Keep grader code outside agent
workspaces and bind the host's independent grading job to its pinned digest.

Each model profile has a unique lowercase `id`, exact `provider`, `model`, `harness` version
and explicit `settings` object. Its `pricing` contains `source`, `as_of` and `usd_per_million`.
The rate keys are `uncached_input`, `cache_read`, `cache_write` and `output`.
Use rates for that run's service tier and date. Zero means a known zero rate, not missing pricing.
Separate profiles represent different settings, prices or disabled tool layers.

```sh
python3 tools/agent-eval-study.py plan manifest.json > frozen-plan.json
mkdir attempts
python3 tools/agent-eval-study.py report frozen-plan.json attempts > report.json
```

Planning creates one `files`/`fr` pair per task, model, mode and repetition. All tasks have a
single-agent mode; those marked `delegated` also have a delegated mode. The seeded order shuffles
pairs and arm order while keeping each pair adjacent for a serial runner. Planning never launches
an agent or incurs provider charges. Keep the frozen plan outside the attempts directory.

The [product review](product-review.md#pilot-before-expanding-or-deleting-subsystems) calls for
four tasks, two models, two repetitions and two delegated tasks: 48 attempts. The generic planner
does not verify task independence or enforce that particular research design.
Review repository selection, held-out status, grader quality and task coverage before the run.
The synthetic 48-cell fixture in `tools/agent_eval/test_study.py` tests wiring only.

Changing the manifest requires a new plan. Retain the old plan and its results. A host must reserve
an attempt's maximum spend before launching it and stop when the remaining cap cannot cover it.
Unknown usage blocks further spend decisions. A retrospective report cannot enforce a live cap.

## Retain each attempt

Write one `attempts/<cell-id>.json` file per attempted or blocked cell. Store supporting files in
`attempts/artifacts/`. References have `path` and `sha256` fields, relative to `attempts/`.
The auditor checks each referenced file's digest, confines it to that directory and limits it to
16 MiB. This version accepts one bounded trace artifact per attempt.

Every record includes `schema: fr-agent-study-attempt-1`, its `cell` ID, `plan_sha256` and `status`.
The plan digest uses UTF-8 JSON with sorted keys, compact separators and ASCII escaping;
`agent_eval.study.digest` is the shared implementation. Duplicate JSON keys and nonfinite numbers refuse.

A `blocked` record needs a nonempty `reason`. It means execution never started and cannot contain
agents or a bill. Missing model access is blocked; a launched agent that fails belongs in `failed`.
Missing records remain `pending`. Extra top-level JSON files refuse instead of hiding unplanned retries.

Executed records, with `completed` or `failed` status, contain:

| Field | Evidence |
|---|---|
| `repository_revision`, `requirement_sha256`, `fr`, `cache_state` | Must match the frozen task, requirement digest, tool identity and cache policy |
| `wall_seconds` | Elapsed attempt time, distinct from summed agent durations |
| `agents` | Parent and complete child roster, described below |
| `trace` | Pinned host trace containing instructions, tool calls, handoffs, fallback and integration work |
| `grade` | Independent outcome and supporting evidence |
| `measurements` | All named measurement fields, with explicit nulls where unavailable |
| `actual_usd` | Actual billed cost or null; a number also requires pinned `billing_evidence` |

`grade` contains `outcome` (`passed`, `failed` or `inconclusive`), `grader_sha256`, pinned `evidence`,
and nonnegative `regressions`, `unsupported_claims` and `human_interventions` counts.
A passing outcome requires all three counts to be zero. A failed execution cannot pass.
This validates the grading contract; it does not rerun or establish the quality of the grader.

## Count parent and child usage

Each agent record contains `id`, `parent` (null only for the root), `children`, and terminal `status`
(`completed`, `failed` or `cancelled`). Include `provider`, `model`, `harness`, `settings`, `seconds`,
boolean `usage_complete` and an `invocations` list. Settings must match the model profile for every
agent in this version. Mixed-model delegation needs a future explicit contract; silent substitution refuses.

The roster must be one rooted tree, respect the child limit and include every declared child.
Agent sessions and provider invocation IDs cannot repeat across attempts. Failed children still count.
An invocation contains a unique `id`, pinned `raw_usage` artifact and a normalized `tokens` object:

| Counter | Accounting rule |
|---|---|
| `uncached_input` | Input outside cache reads and writes |
| `cache_read` | Input billed as cache reads |
| `cache_write` | Input billed as cache creation; remove it from uncached input if the provider includes it there |
| `output` | Complete output count, including reasoning when the provider includes reasoning |
| `reasoning` | Optional subset of output; never add it again to cost or aggregate tokens |

The first four counters are disjoint. Add an invocation `format` to recompute counters from its
pinned raw JSON: `openai-response-1`, `anthropic-message-1` or `codex-turn-1`.
The auditor refuses changed counters, mismatched model/provider identities and duplicate requests.
Records without `format` retain legacy host-supplied accounting and report `provider_usage_verified: false`.
Use `report --require-provider-usage --require-complete` when collecting the final study.
The first flag checks executed attempts; the second also requires all planned cells and measurements.

The host must retain every billable request, including retries, and reconcile children against its
complete trace. Verifying supplied responses cannot discover omitted calls or authenticate a provider bill.
The adapters accept final response objects, not streaming deltas; summing both would double count.

```sh
python3 tools/agent-eval-host.py normalize openai-response-1 EXACT_MODEL response.json
```

OpenAI's `input_tokens` includes cache reads and writes. The adapter subtracts both to derive ordinary
input; a missing write count leaves ordinary input and price unknown. Reasoning stays within output.
See [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching).
Anthropic's input count already excludes cache reads and writes, so the adapter adds those for total
input. One-hour cache writes and server tool charges refuse token-only pricing with this single-rate
profile. See [Anthropic prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching).
OpenAI hosted tools and separately counted media also require additional pricing support.

For Codex, retain the completed-turn event with host envelope fields `thread_id`, `turn_index` and
configured `model`. The adapter labels that model as host metadata and keeps cache writes unknown.
The documented [Codex JSONL summary](https://learn.chatgpt.com/docs/non-interactive-mode) does not
supply a cache-write count. This format cannot satisfy the strict provider-usage gate.

Missing counters stay null. Known zero usage must be explicit. Missing billable counters, an empty
invocation list or `usage_complete: false` makes the cost estimate unknown. An absent reasoning
subset remains visible without preventing pricing when complete output is known.
Agent durations can sum above wall time during delegation; neither substitutes for the other.

## Keep resource and context costs visible

Every executed record supplies these `measurements` keys, each a nonnegative number or null:

```text
tool_calls tool_result_bytes source_read_bytes repeated_read_bytes fallback_calls
retries pages instruction_bytes handoff_bytes integration_tokens peak_context_tokens
tool_cpu_seconds sampled_aggregate_rss_bytes disk_growth_bytes cache_growth_bytes
```

Only `tool_cpu_seconds` may be fractional. Growth means maximum additional bytes above the initial
baseline, including temporary files; reclaiming files later cannot erase a peak. Peak context counts
the largest active agent context, not cumulative input tokens. Record the sampling method in the trace.
Do not label the sum of separate agent memory peaks as sampled concurrent memory.

The auditor retains budget overruns and missing measurements. Comparable successful pairs require
both arms to pass, complete billable usage, measured budgets and no budget overrun.
Success counts and costs for all executed attempts appear before pairwise cost differences.
Model profiles and single/delegated modes remain separate. Cost per success includes failed attempts;
with zero successes it is null. Estimated and actual dollars have separate fields.
The spend ledger labels its use of actual billing where supplied and estimates otherwise.

Use `report --require-complete` for a finished collection. It prints the report and exits 1 for
pending or blocked cells, incomplete usage, missing measurements, inconclusive grades or budget overruns.
Ordinary reporting exits successfully for structurally valid partial evidence, including failed tasks.
Neither mode estimates a population savings rate or upgrades a roadmap acceptance item.

## Existing Codex sessions

The Codex runner accepts arbitrary task IDs. Session metadata may include `model`, `mode`,
`repository_revision`, `requirement_sha256`, `grader_sha256`, `budgets`, `model_settings`, `fr`,
`cache_state` and `plan_sha256`. Pair selection separates models and modes and rejects unmatched metadata.
Set `preserve_trial_order: true` in `experiment.json` to keep its declared pair and arm order.
Legacy experiments retain their declared arm order. Paths cannot escape the session directory.
Declared models and `model_settings` must also match the launch arguments before any trial starts.
For this launcher, `model_settings` has exactly `reasoning_effort` and `service_tier` keys.

The launcher uses a fresh POSIX process group. It kills that group after parent exit, timeout,
resource excess or monitor failure. Output retention stops at 16 MiB across stdout and stderr.
Defaults bound wall time to 900 seconds, sampled group RSS to 768 MiB, sampled CPU to 600 seconds
and session-directory growth to 128 MiB. The version probe has a ten-second deadline and 64 KiB output cap.
Run this workload on a GitHub worker; workstation invocations still require the local resource guard.

These are sampled process-group checks, not OS containment. Short-lived peaks, processes that leave the
group and writes outside the session need runner-level quotas and isolation. Logs record the limits,
observed peaks, stop reason and process exit code. Resource stops produce a nonzero launcher result,
even if the host process itself exited successfully. No claim of complete system-wide accounting follows.
Legacy evidence and historical cohorts keep their original schemas.

## Reserve money before requests

`agent_eval.study_budget.Budget` provides a SQLite ledger shared by parent and child host hooks.
It binds to the entire frozen plan, serializes mutations, and stores integer billionths of a dollar.
Keep it outside agent-writable directories. Each admitted attempt holds its maximum dollar budget.
Every request also reserves conservative input/output cost and tokens within that attempt.
Input uses the highest frozen input rate. Request bounds must cover all provider input, including
system text, tools and cached context; the host must enforce the output bound at the provider.

The command-line hook accepts a JSON action file:

```sh
python3 tools/agent-eval-host.py budget frozen-plan.json host-budget.sqlite action.json
```

| Action | Additional fields | Effect |
|---|---|---|
| `begin` | `cell` | Reserve the planned attempt maximum; refuse silent retries |
| `reserve` | `cell`, `request`, `agent`, `input_limit`, `output_limit` | Hold worst-case dollars and tokens across all agents |
| `dispatch` | `request` | Authorize one send after a successful reservation |
| `cancel` | `request` | Release an undispatched reservation |
| `unknown` | `request` | Retain an uncertain charge and block further dispatch |
| `settle` | `request`, `format`, `response` | Read final provider JSON and price it with frozen rates |
| `finish` | `cell` | Close only after every request settles or cancels |
| `snapshot` | None | Inspect durable attempts, requests and remaining holds |

Example reservation: `{"action":"reserve","cell":"CELL","request":"request-1","agent":"parent","input_limit":4000,"output_limit":1000}`.
Use the actual cell ID from the frozen plan. Each host send must follow reserve and dispatch.
Never give model workers a bypass path or the ledger credentials. Network retries need new reservations.
A crash after dispatch leaves its full hold intact; reconcile the request or mark it unknown before resuming.
Incomplete or malformed usage stays unknown. An overrun remains visible and blocks further dispatch.
The ledger never converts an ambiguous network failure to zero cost or rolls back a real overrun.
Settlement calculates a rate-based estimate; provider billing remains separate evidence in the report.

This is an integration contract, not a proxy around an uninstrumented CLI. It enforces the cap only
when a trusted host gates every paid operation and supplies valid request bounds and pricing.
Hosted tools, mixed cache-write prices and unrelated billable services need separate reservation support.

## What remains before the pilot

| Ready in this repository | Still required for live evidence |
|---|---|
| Frozen pairs and complete-attempt auditor | Independent tasks, pinned repositories and reviewed graders |
| Raw OpenAI/Anthropic response conversion | Host collection of every request, retry and child invocation |
| Concurrent reserve/dispatch/settle hooks | A provider host wired through those hooks with no bypass |
| Codex process-group cleanup and sampled limits | Runner containment, isolated checkouts and private grader execution |

No acceptance item changes from these synthetic tests. After integrating the host, run a small paid
preflight before the 48-attempt pilot. Use its complete costs and independent grades to choose which
fr routes deserve improvement or removal.
