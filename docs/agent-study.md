# Compare ordinary tools with fr on independent tasks

`tools/agent-eval-study.py` freezes a study plan and audits retained attempt records.
Task names and repository identities are data. No production analyzer rule depends on them.
The same format accepts different models, single agents and delegated work.

The auditor checks retained evidence. `tools/agent-eval-host.py` now sends provider requests through
the budget ledger and grades submitted code against private cases in a container. The existing
Codex launcher bounds process resources. Tests use fake providers and small executable submissions;
no new paid trial has run. The serial runner below now connects the gateway to tools and grading
for fix/feature tasks. The pilot still needs independent tasks, reviewed oracles, explanation/proof
grading adapters and complete tool/context and system resource measurements.
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
The report validates this grading contract. The `grade` command below executes a pinned black-box
grader; task reviewers still need to establish the quality and independence of its cases.

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

## Send a request through the ledger

Run `send` on a trusted GitHub worker after admitting the cell with `budget` action `begin`.
Keep provider credentials and the ledger outside model workers. The only credential variables the
transport reads are `OPENAI_API_KEY` and `ANTHROPIC_API_KEY`. It uses fixed provider origins, follows
no redirects, retries no generation calls and limits retained response bodies to 16 MiB.
It applies socket timeouts and a body-read deadline; the worker must also enforce its overall lifetime.
No network call or credential lookup occurs without the CLI spend acknowledgement.

Set the model's `harness` to `fr-study-api-1` and freeze these `settings`:

```json
{
  "request": {},
  "max_output_tokens": 1024,
  "input_token_ceiling": 1000000,
  "input_bound_source": "Documented input limit for the exact model and settings"
}
```

The numbers illustrate the format. Use the model's documented bounds and freeze suitable prices
before live use. `request` contains supported provider options such as reasoning effort, service tier
and client function tools. Callers supply only conversation input and instructions; they cannot replace
model settings, turn on hosted tools, enable streaming or refer to mutable server-side conversations.
This adapter accepts text and client tool messages. Media, hosted tools and Anthropic signed thinking
replay need separate adapters. It refuses them before generation.

OpenAI input files contain `input` and optional `instructions`. Anthropic input files contain `messages`
and optional `system`. Include all prior client tool calls and their results explicitly.

```sh
python3 tools/agent-eval-host.py send frozen-plan.json host-budget.sqlite CELL_ID \
  CELL_PARENT request-001 input.json attempts --confirm-agent-spend
```

For a child call, use its unique agent ID and `--parent CELL_PARENT`. Admission persists one rooted
agent tree, enforces the single/delegated child limit and rejects session reuse across attempts.
The gateway checks the open attempt and unknown-charge state before contacting the provider.
It then retains the exact request object, counts input, reserves the bounded generation cost,
dispatches once, retains the response and settles the reservation. Competing parent and child calls
share the same atomic dollar and token holds. Each retry needs a new request identity.

[OpenAI's counting endpoint](https://developers.openai.com/api/docs/guides/token-counting) documents
an exact count for explicit request input. That count supplies the input reservation.
[Anthropic's endpoint](https://platform.claude.com/docs/en/build-with-claude/token-counting) returns an
estimate, so that path reserves the entire frozen `input_token_ceiling`. The ceiling must bound actual
provider input, such as the documented model context limit; adding a guessed percentage is insufficient.
This can require a larger per-attempt budget. Refusal means the frozen cap cannot cover the request.

The ledger covers generation tokens at the frozen rates. Confirm pricing tiers, counting endpoints,
host compute and other service charges separately before a paid pilot. Account-wide billing can include
charges outside this gateway. The code does not intercept arbitrary Codex or Claude CLI traffic.

Each request creates `attempts/REQUEST_ID/` with payload, count, response when available, and receipt.
The receipt's `invocation` object plugs into the auditor's agent record; its artifact paths are relative
to `attempts/`. Receipts also bind the frozen plan, agent relationship, settings and request elapsed time.
Failed requests retain a receipt. Ambiguous sends and incomplete usage remain unknown;
provider errors never trigger a hidden retry. A crash after dispatch retains the full reservation.
Reconcile such requests before restarting. Complete agent rosters, durations and tool measurements still
come from the calling host; request receipts alone cannot prove that the host reported every operation.

## Grade an exported submission

The `grade` command reads a grader file whose byte digest matches the frozen task's `grader_sha256`.
It snapshots a bounded submission, then starts a fresh Docker container for each case. Expected outputs
remain in the trusted host process. The container receives only the submission, command and that case's
stdin. It has no network, no capabilities, a read-only root and submission, bounded tmpfs, half a CPU,
a memory limit with no extra swap, a process-count limit and an unprivileged user.
See [Docker's runtime controls](https://docs.docker.com/engine/containers/run/).

Stop all agent processes before grading. Supply a clean exported submission; this command does not
check out repositories or establish its base revision. Keep the private grader outside that directory.
The snapshot rejects symlinks and special files and binds paths, contents, executable bits and empty
directories. Prepare dependencies in the pinned image; grading never pulls an image or installs packages.
Images declaring writable volumes refuse before execution.
The runtime supports a local default Docker context. Use a disposable runner with a trusted daemon.

A small grader file has this shape:

```json
{
  "schema": "fr-stdio-grader-1",
  "image": "sha256:REPLACE_WITH_THE_LOCAL_IMAGE_ID",
  "command": ["python3", "/workspace/answer.py"],
  "limits": {
    "wall_seconds": 3,
    "memory_bytes": 67108864,
    "scratch_bytes": 1048576,
    "output_bytes": 4096,
    "candidate_bytes": 1048576
  },
  "cases": [{"id": "hidden-case", "stdin": "6\n", "stdout": "42\n", "exit_code": 0}]
}
```

Use an actual immutable image ID or repository digest, then hash the complete grader file before
freezing the plan. Choose independent cases appropriate to the task; this arithmetic example only
explains the format. The grader compares exact UTF-8 output and exit status. It cannot judge explanation
quality or arbitrary behavioral correctness without an appropriate executable protocol.

```sh
python3 tools/agent-eval-host.py grade frozen-plan.json CELL_ID exported-submission private-grader.json
```

The result binds the grader, image and candidate snapshot and retains each case's output bytes, hashes,
exit state and resource-stop reason. A wrong answer, timeout, memory failure or execution error cannot
pass. Cleanup explicitly removes the container, including after a client timeout. Cleanup failure stops
the worker instead of returning a usable grade. Retain the result as the attempt's grading evidence.
Docker isolation depends on the worker kernel and daemon; it is not a proof against container escapes.

The CI study check runs real containers for correct and incorrect submissions, private-grader visibility,
read-only mounts, network denial and timeout cleanup. Local checks skip those cases and use small fake
providers. No model service or local container starts as part of the workstation tests.

## Run one agent attempt

The `run` command executes one frozen fix/feature cell. It stages the exact commit from a local
Git object database, runs a conversation through the gateway, dispatches tools and grades the final
submission. Use it on an isolated GitHub worker. It does not select tasks or launch the 48-cell pilot.

First obtain settings for a reviewed, secret-free tool image with Python 3 and the task's dependencies
already installed. The image must have no writable `VOLUME` declarations. Pin its local image ID or
repository digest; the runner never pulls an image. Supply a Linux `fr` binary for Linux containers.

```sh
python3 tools/agent-eval-host.py loop-profile openai sha256:IMAGE_DIGEST skills/fr > loop-profile.json
```

Copy `runner` from this output into the study manifest and `tools` into each corresponding model's
`settings.request.tools`. Generate the other provider's tool definitions with `loop-profile anthropic`.
Keep `harness: fr-study-api-1`, exact model identities, validated token bounds and dated prices.
The runner digest binds its implementation and host dependencies; editing those files requires a new plan.
The skill entry file must match `fr.skill_sha256`; `runner.skill_tree_sha256` also binds its reference files.
Freeze the plan only after choosing these settings. `loop-profile` performs no provider requests.

```sh
python3 tools/agent-eval-host.py run frozen-plan.json host-budget.sqlite CELL \
  /worker/repository /worker/private-grader.json /worker/attempts \
  --binary /worker/fr --skill /worker/fr-skill --confirm-agent-spend
```

The command checks source, tool and grader identities before admitting an attempt. It ignores
uncommitted and untracked files. Git submodules, symlinks, special files, invalid UTF-8 paths and
archives that omit tracked files refuse. Git archive attributes still apply to exported content;
the trace retains the exact initial workspace digest. Use ordinary pinned source trees for this runner.
An existing cell record or ledger admission refuses a second run. A preflight failure can leave an
artifact directory without an admitted attempt; inspect it before preparing a fresh run.

Both arms expose `command(argv, stdin)`, `read_source(path, offset, bytes, sha256)` and
`delegate(question)`. Only the `fr` arm receives the
binary and skill mounts. The operator must ensure the shared image does not already contain `fr`.
Commands can use ordinary tools, execute tests and edit files. Each command starts in a fresh,
network-free container with a read-only root, two 16 MiB writable tmpfs mounts, 256 MiB memory,
half a CPU core and 32 processes. The host never mounts credentials, the budget ledger, grader
or Docker socket. Only the regular files exported from `/workspace/project` persist between calls.
Environment changes, empty directories, `/tmp` contents and background processes do not persist.
These constraints suit small tasks; builds that need larger storage require a separately reviewed runner.

Workspace exports have at most 2,000 files and 8 MiB of file contents. Command output defaults to
16 KiB, command execution to 20 seconds, and each attempt to 32 provider turns and 64 tool calls.
The frozen profile can lower these limits or raise them only within the implementation's fixed ceilings.
All agents share the turn, call, token and dollar budgets. The runner reserves evidence capacity before
another request, limits the trace to 8 MiB and leaves space for submission and grading artifacts.
It checks wall and summed agent time between operations and keeps time for command cleanup.
Provider socket deadlines and grading limits do not provide a hard whole-worker lifetime; retain an
outer GitHub job timeout. Docker limits do not measure total host/daemon memory, CPU or disk use.

Delegation runs serially, with at most four children including descendants. Each child receives the
same tool policy, a copy of the parent's current files and the supplied question. Parent conversation
history stays out of its prompt. The parent receives the child's final findings; child edits disappear.
A child failure fails the attempt and retains every launched agent and request. There is no automatic
retry, restart, transcript compaction or hidden integration step.

The loop replays OpenAI response items, including encrypted reasoning and message phases, with
`store: false`. Unsupported items, missing encrypted reasoning, duplicate tool IDs and incomplete
responses stop execution. Anthropic text/tool exchanges use assistant `tool_use` followed by user
`tool_result` blocks. Signed thinking remains unsupported. See the official
[OpenAI reasoning guide](https://developers.openai.com/api/docs/guides/reasoning),
[function calling guide](https://developers.openai.com/api/docs/guides/function-calling) and
[Anthropic tool definitions](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools).

An attempt retains gateway receipts, provider responses, the tool trace, final submission and private
grade under `attempts/artifacts/CELL/`, then writes `attempts/CELL.json` for the existing auditor.
The trace records host-dispatched calls and delivered results; tool output remains untrusted evidence.
Known charges close normally after failures. Uncertain charges keep their ledger hold and block spending.
The command exits unsuccessfully when execution or grading fails.

The runner counts dispatched tool calls, delivered result bytes, initial instructions/tool schemas/questions,
handoff question/answer bytes and zero automatic retries. The auditor recomputes these counters from
the retained trace and rejects disagreements. OpenAI peak context records the largest counted request
input plus its reported output; Anthropic's estimate leaves that measurement null.
Source reads inside arbitrary commands, fallback use, integration tokens and full-system CPU/RSS/disk/cache
peaks remain unmeasured. Wall time includes the conversation, tools and grade;
source/image preflight precedes admission. `report --require-complete` therefore remains unsuccessful.
These records test the machinery and expose missing measurements; they cannot establish efficiency gains.

### Bound and measure container work

`fr-study-loop-3` can freeze a shared resource budget for every command and private grader container
in an attempt. This includes containers launched for child investigations and all private grading cases.
The Python host, Docker daemon, disk use and cache growth remain outside this measurement.
Their attempt-wide counters stay null; this feature does not close the full-system resource requirement.

Generate a profile with the budget enabled before freezing the study:

```sh
python3 tools/agent-eval-host.py loop-profile openai sha256:IMAGE_DIGEST skills/fr \
  --container-resources > loop-profile.json
```

The resulting `runner.container_resources` defaults to 60 CPU seconds, 256 MiB of charged memory,
and 64 processes. Limits can be lowered before freezing. Fixed ceilings are 600 CPU seconds,
512 MiB and 64 processes; memory must be a multiple of 4,096 bytes and at least 16 MiB.
The process limit must be at least eight. The kernel CPU rate is fixed at half a core with no swap.
Omitting the flag freezes `container_resources: null` and explicitly leaves container resources unmeasured.
Older profiles cannot be silently upgraded; old attempt reports retain their original audit rules.

Run this setup on a disposable Linux worker with local rootful Docker, the systemd cgroup driver,
and a unified cgroup-v2 mount. Prepare images before the attempt. The host does not change daemon
settings, create administrative units or grant itself permissions. `scope-unit` prints a unit definition
from the frozen plan; it performs no system changes or provider calls.

For example, an operator can prepare a fresh unit on that worker:

```sh
study_slice="frstudy$(python3 -c 'import uuid; print(uuid.uuid4().hex)').slice"
python3 tools/agent-eval-host.py scope-unit frozen-plan.json "$study_slice" > "/tmp/$study_slice"
sudo install -m 0644 "/tmp/$study_slice" "/run/systemd/system/$study_slice"
sudo systemctl daemon-reload
sudo systemctl start "$study_slice"
sudo chown "$(id -u)" "/sys/fs/cgroup/$study_slice/cgroup.kill"
```

Then supply `--container-slice "$study_slice"` to the existing `run` command. Both the option and
the frozen resource profile are required together. The scope must be empty and unused, its kernel
settings must match, and the host must remain outside it. An exclusive host lock prevents concurrent
admission into the same scope. Arbitrary system slice names, remote Docker sockets and rootless
Docker refuse before admitting an attempt or contacting a provider.

Every container receives the common parent, which the host checks before starting it. The monitor
reads cumulative kernel CPU and memory counters every 50 milliseconds and between operations.
At the CPU budget, a memory-limit event, or a process-limit event, it kills the scope's processes and
stops further work. Descendants remain covered even if they create new process groups. CPU stopping
is sampled and can overshoot while the monitor is descheduled. Memory and process limits are enforced
by the kernel; memory accounting includes charged file pages and kernel memory, not just process RSS.
See the [Linux cgroup-v2 interfaces](https://cdn.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html)
and [Docker cgroup-parent rules](https://docs.docker.com/reference/cli/dockerd/#default-cgroup-parent).

The host retains `container-resources.json` beside the trace and grade. It includes the frozen limits,
scope identity, CPU microseconds, kernel memory peak, limit events, sample count and stop reason.
The report verifies that artifact and exposes its scoped CPU seconds and memory peak separately.
A resource stop makes the attempt failed and its grade inconclusive, even if a command or grader
returned a correct answer. Monitoring errors, backwards counters and failed cleanup cannot produce
a clean resource result. Unresolved provider charges retain their existing ledger holds.

After the attempt, the operator removes the dedicated unit on the disposable worker:

```sh
sudo systemctl stop "$study_slice"
sudo rm -- "/run/systemd/system/$study_slice"
sudo systemctl daemon-reload
rm -- "/tmp/$study_slice" "/tmp/$study_slice.lock"
```

Keep an outer worker deadline and cleanup step for host crashes. The resource adapter cannot report
after the host itself is killed, and it does not replace machine-wide isolation or disk quotas.
No paid trial should infer total resource savings from these container-only measurements.

### Read source a page at a time

The `fr-study-loop-2` profile adds the same source tool to both arms. Generate and freeze a new profile
when upgrading; the runner refuses old profiles rather than changing their tool policy silently.
Historical attempt records remain readable under their original audit rules.

For example, the agent can request:

```json
{"path":"src/parser.py","offset":0,"bytes":2048,"sha256":""}
```

The host reads only from the current exported workspace, never from a host filesystem path supplied
by the agent. It returns UTF-8 `text`, the file's `sha256` and `size_bytes`, exact `offset` and
`end_offset`, and `next_offset` when more content remains. Continue with that offset and hash.
If a command changed the file, the old hash produces `stale_source` without revealing its new text;
start again at offset zero to inspect the changed version. Missing files, non-UTF-8 content, offsets
inside a character and insufficient page budgets produce explicit refusals.

The requested source limit is at most 65,536 bytes. The complete JSON result must also fit the frozen
output budget, including metadata and escaped characters. Pages end at whole UTF-8 characters.
The profile requires at least 1,024 output bytes. A smaller requested page can refuse if it cannot
hold even the next character. An empty file or a read at end-of-file returns an empty terminal page.

Each reported attempt now includes `source_disclosure`:

| Field | What it counts |
|---|---|
| `source_read_bytes` | UTF-8 source bytes delivered through successful `read_source` calls |
| `repeated_read_bytes` | Delivered bytes already covered by earlier pages from any agent |
| `same_agent_repeated_bytes` | The subset also disclosed earlier to the receiving agent |
| `pages` | Successful source pages, including empty terminal pages |
| `opaque_command_calls` | Dispatched commands whose internal reads are not observed |
| `failed_read_calls`, `unfinished_read_calls` | Explicit refusals and dispatched reads without retained results |
| `complete` | Whether all source disclosure in this tool loop can be counted |

Overlap is the union of earlier byte intervals for the same path and full file-content hash. Reading
overlapping pages three times counts each repeated delivery once. Parent and child reads share the
attempt-wide history, while each agent also has its own history. Changed content and renamed paths
start new identities; unchanged spans inside a changed file are conservatively treated as new reads.
These are disclosure counts, not physical disk I/O, provider tokens or a measure of whether an agent
understood the content. Replayed conversation history is accounted for in provider usage separately.

If any command was dispatched, or a source call was interrupted, the attempt's total
`measurements.source_read_bytes`, `repeated_read_bytes` and `pages` remain null. The partial
`source_disclosure` counters stay visible. A `cat`, test command or `fr` call could read files internally;
the runner does not guess from executable names, output text or an agent's claim that it read nothing.
Without such gaps, those three measurements contain the exact host-page counts, including known zeros.

The auditor verifies trace structure, page extents, continuation identities, overlap and delivered-byte
counts. It rejects invented aggregate counters. The trace remains an attestation by the trusted host;
it does not independently prove the full file hash from a partial excerpt or recover hidden command reads.

## What remains before the pilot

| Ready in this repository | Still required for live evidence |
|---|---|
| Frozen pairs and complete-attempt auditor | Independent tasks, pinned repositories and reviewed graders |
| Provider gateway and serial parent/child loop | Paid protocol preflight with the exact model profiles |
| Retained conversations, audited source pages, tool calls, durations and handoffs | Reads inside commands and integration context |
| Shared container CPU, memory and process budgets with retained kernel counters | Host/daemon accounting, disk/cache measurement and complete worker resource enforcement |
| Pinned Git exports and isolated command workspaces | Independent task selection, suitable tool images and larger-workspace policy if needed |
| Private black-box grading in constrained containers | Reviewed cases and explanation/proof grading adapters |

No acceptance item changes from these tests. After preparing tasks and completing measurements, run a small paid
preflight before the 48-attempt pilot. Use its complete costs and independent grades to choose which
fr routes deserve improvement or removal.
