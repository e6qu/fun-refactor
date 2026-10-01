# Test OpenCode with native source tools

The earlier [repository explanation trials](opencode-source-evidence.md) exposed an interaction
problem: five of twelve attempts stopped because a model replied with prose instead of one JSON
action. The native adapter lets OpenCode call tools during one session. It keeps the same frozen
source snapshots and private factual rubrics. These reviewed projects are development tests;
they are not an independent evaluation corpus.

`tools/native-rehearsal.py` provides `freeze`, `run`, `report` and `review`. This adapter belongs to the
evaluation tooling. The product remains the `fr` CLI.

## Tools and limits

Both arms can list files, search literal text and read source pages. The `fr` arm also has bounded
public `project map`, `project find` and `project show` commands. Models choose which tools to use.
A separate submission tool accepts the requested claim object once. Each claim needs an exact
value and exact quotations from retrieved source. The rubric is unavailable to the model.

The adapter uses the [MCP stdio transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)
and [tool interface](https://modelcontextprotocol.io/specification/2025-06-18/server/tools).
OpenCode supports [local MCP commands](https://opencode.ai/docs/mcp-servers/).
The configuration denies other tools and disables automatic compaction, snapshots and project
configuration. Provider credentials stay in the existing OpenCode configuration.

Each attempt has twelve model steps, twenty-four tool calls and 120 seconds total wall time.
The earlier runner allowed eight JSON-action responses. This protocol change also changes the
interaction budget, so differences between these cohorts cannot isolate the effect of transport.
Each monitored process group has a twenty CPU-second limit, 768 MiB sampled RSS and a 1 MiB
output limit. Attempt storage growth is limited to 16 MiB. Source reads request at most 8192
bytes; `fr show` requests 4096. Tool results have a 16 KiB serialized limit.

Local calls must also run through `/Users/zardoz/.codex/tools/fr-local-guard.py`. That guard keeps
the workstation's stricter disk reserve, serial execution and CPU throttling. A stopped attempt
stays failed. Neither runner retries it or raises its limits.

Sampling can miss brief peaks. Process groups exclude escaped processes and global OpenCode
storage; the outer workstation guard also tracks descendants. These are trusted local integration
tests with restricted tools, not an operating-system sandbox for hostile code.

## Retained evidence

A frozen plan binds the tasks, model names, binary hash, implementation, tool schemas and limits.
Every attempt retains the prompt, schemas, host tool log, CLI stream, session export and process
measurements. An existing attempt directory cannot be overwritten. Missing and interrupted cells
remain visible in the report.

The auditor matches each host result to a native tool call in both the CLI stream and session
export. It checks the actual provider/model identity and CLI usage for every assistant message.
Matching includes tool errors, so a model can recover from a refused request. Source-hash guidance
now specifies the empty string for a first read; retained plans keep their original tool schemas.
Ordinary reads and searches are replayed against the snapshot. Source from `fr show` must match
the frozen bytes at its reported location.

A result can support a submitted answer only when its tool call belongs to an earlier assistant
message. A read and submission requested together cannot use each other's results. Counts describe
source available in retained conversation history. They do not capture every provider request,
system prompt, schema token or internal tool read. CLI cost counters are not verified billing.

Private grading checks the finite claims and their required source anchors. It does not judge
arbitrary explanations or prove program behavior. The earlier JSON-action runner and all its
results remain unchanged, including failures and the corrected disclosure accounting.

## Run a comparison

Freeze `tests/agent-eval/opencode/explanations.json` with `--binary` pointing to the installed `fr`.
Run one frozen cell at a time with explicit `--base`, `--binary`, `--opencode` and
`--confirm-agent-spend`. The runner cannot enforce a dollar cap. Use `report` to replay retained
attempts without calling a model. Use `review` for a separate assessment after an auditor fix;
it retains original outcomes and cannot promote a resource-stopped attempt.
Full gates and repository runtime controls belong on CI.

The offline tests exercise real stdio framing, argument refusals, source replay, model substitution,
missing records and results unavailable before submission. They run in the existing study CI job.
Live model services are not called by CI.

## October 2 results

The [retained records](../tests/agent-eval/opencode/results/2026-10-02-native/README.md) contain
twelve repository attempts and two separate tool-access controls. The installed tools were
OpenCode 1.18.34 and `fr` 0.35.0; the plans record the binary hash. No local build ran.

| Task | Model | Ordinary tools | fr available |
|---|---|---|---|
| Cache expiration | Kimi K3 | Pass, 41.6 s | Pass, 50.4 s; no fr calls |
| Cache expiration | GLM 5.3 Flash | Pass, 100.0 s | Pass, 114.8 s; no fr calls |
| Key rotation | Kimi K3 | Citation failure, 53.9 s | Citation failure, 30.8 s; no fr calls |
| Key rotation | GLM 5.3 Flash | Citation failure, 72.4 s | Pass, 51.0 s; no fr calls |
| Retry callbacks | Kimi K3 | Pass on corrected audit, 82.8 s | Pass on corrected audit, 71.3 s; one fr map |
| Retry callbacks | GLM 5.3 Flash | Pass, 116.3 s | 120-second timeout; no recorded fr calls |

The original total was six passes. The first auditor mishandled native tool errors in both Kimi
retry attempts, even though the model recovered. Offline review verifies their answers and source,
bringing the reviewed total to eight of twelve. Original failed records remain unchanged.

All three citation failures have correct factual values. They omit the required call connecting
signing to key derivation. The timeout remains failed. These distinctions matter when choosing a fix.

The separate Kimi access controls both passed. When explicitly requested, the model called native
`fr find` and `fr show`, and the returned source passed the audit. That confirms tool access;
it does not explain the low voluntary use of `fr` in the repository trials.

Across all fourteen attempts, peak sampled process-group RSS was 678.3 MiB. The tests remained
serial under the workstation guard. Retained evidence occupies about 3.3 MiB after removing disposable workspaces.
The earlier JSON-action trials remain separate. Changed interaction budgets, reviewed tasks and
small sample sizes prevent an efficiency claim or a general success-rate estimate.
