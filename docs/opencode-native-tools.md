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


## Compare compact exploration with public guidance

Tool schema version 3 adds a three-arm comparison. `files` has ordinary list, search and source
reads. `fr` adds one public `project explore` tool with the compact profile. `fr-guided` has exactly
the same tools plus a frozen excerpt from `skills/fr/references/explore.md`. Ordinary tools remain
available in every arm. Guidance is optional; the task questions do not require agents to call `fr`.

Freeze this protocol with `--guidance skills/fr/references/explore.md`. The plan retains the full
public document, its hash, excerpt byte offsets, exact prompts, schemas and seeded cell order.
The base prompt makes no recommendation between ordinary and structured tools. The chosen excerpt
explains names, a selected row's next action, and truncation continuations. It excludes the expanded
profile and batch routes that this adapter does not expose.

`fr_explore` accepts a term, mode, full target handle, source-file filter and continuation fields.
It always requests the compact profile: twelve name rows, 2048 source bytes, eight relationships
and a 16 KiB report budget. Agent arguments never become shell text. Exact source from a behavior
result must match the frozen repository bytes, handle and offset before it can support a claim.
An absent declaration or failed call contributes no source evidence.

The report records configured agent-prompt bytes, user-prompt bytes, tool-schema bytes and guidance
bytes separately. The instructions appear in both the agent configuration and the user prompt;
the report exposes that duplication. These counts do not describe provider serialization, hidden
instructions or every repeated input. OpenCode token counters remain reported usage, not verified
billing. Versions 1 and 2 keep their original prompts, schemas, records and replay rules.


## Compact exploration results

The [three-arm comparison](../tests/agent-eval/opencode/results/2026-10-02-guided/README.md)
retains eighteen attempts with `fr` 0.46.0 and OpenCode 1.18.34. It used the same reviewed repository
tasks and fixed 120-second budget. The original records show three passes, nine citation failures and six
timeouts. All twelve completed answers had correct factual values. This does not erase missing
or altered citations: the original rubric and outcomes remain unchanged.

| Task | Model | Ordinary tools | Compact fr | Compact fr + guide |
|---|---|---|---|---|
| cache-expiration | Kimi K3 | Pass, 78.7 s | Pass, 46.2 s | Citation failure, 69.5 s; 1 fr call |
| cache-expiration | GLM 5.3 Flash | Timeout, 120.1 s | Timeout, 120.0 s | Timeout, 120.0 s |
| key-rotation | Kimi K3 | Pass, 46.0 s | Citation failure, 48.3 s | Citation failure, 56.4 s; 1 fr call |
| key-rotation | GLM 5.3 Flash | Citation failure, 75.2 s | Citation failure, 96.1 s | Citation failure, 112.0 s |
| retry-callbacks | Kimi K3 | Citation failure, 75.3 s | Citation failure, 69.4 s | Citation failure, 70.3 s |
| retry-callbacks | GLM 5.3 Flash | Timeout, 120.1 s | Timeout, 120.1 s | Timeout, 120.1 s |

Two of twelve attempts with `fr` available called it, both with guidance. They used names only;
neither followed the behavior continuation. Both failed the original single-page evidence rule. This sample does
not show that public guidance improves success or efficiency. The earlier 0.35.0 cohort used a
different prompt and tool surface; its eight reviewed passes out of twelve remain a separate result.

The optional guidance adds 273 bytes to each configured prompt, including its heading. The tool
schema adds 635 bytes over ordinary tools. The guided cache attempt found the class through `fr`,
then hit the ordinary read tool's hash requirement for a nonzero first offset. It recovered with
reads from the beginning. That cost belongs to the evaluator's read interface, not to program analysis.

Next remove avoidable source-copying and read restrictions in the evaluation interface while
preserving frozen source identities, required evidence and unchanged historical results. Then test
actual changes on independent tasks. Do not expand analysis or force tool use based on these results.
The technical status remains seven of eighteen items demonstrated and no completed milestone.


## Correct citations across source pages

The original quotation check required an entire quote inside one tool result. That rejected exact
source across adjacent pages even when every byte had reached the model before its answer.
New plans bind the `contiguous-source-v1` policy. Grading joins verified adjacent or consistently
overlapping spans with the same file and content hash. Unread gaps, conflicting overlaps and changed
identities refuse. Source from the submission's own step or a later step remains unavailable.
Factual values, required anchors and the 16-to-2048-byte quotation bound stay the same.

`report` keeps the frozen policy and original outcomes. `review --contiguous-source` writes a
separate assessment with the correction. It cannot turn a resource-stopped attempt into a pass.
The [retained review](../tests/agent-eval/opencode/results/2026-10-02-guided/coverage-review.json)
recovers three Kimi answers: ordinary-tools retry, guided cache and guided key rotation. The reviewed
total is six passes, six citation failures and six timeouts; the original three passes remain in
`report.json`. This corrects evidence accounting without showing improved model reasoning or a
benefit from `fr`. No additional model call produced these reviewed results.

## Cite retrieved code without copying it

`freeze --source-references` selects tool schema version 4. It keeps the same source,
tool-call, time and memory limits. Earlier schemas and retained reports keep their original rules.
The flag works with the two ordinary/fr arms or with `--guidance` and all three arms.

A first `read_source` call can start at a known byte offset with an empty `sha256` because
the host owns an immutable snapshot. Continuations still check a supplied hash. Invalid UTF-8
boundaries, stale hashes, missing files and path escapes refuse. This rule applies to the
native evaluation interface; the general study reader keeps its existing contract.

Source-bearing results include `source_refs`: IDs with file paths and exact byte ranges.
Each ID binds the file hash and a 16-to-2048-byte UTF-8 slice. Names and relationship metadata
do not create evidence. A model can submit `{"source":"src1:..."}` instead of copying that
slice into a quotation; exact `{path, quote}` citations remain valid. Slices shorter than
16 bytes have no reference. Ordinary pages can contain several reference slices.

Replay recomputes the IDs from frozen source and checks their presence in the retained tool
results. Grading resolves only source delivered before the answer's assistant step, then uses
the same factual values, required anchors and maximum six citations per claim. A guessed ID,
missing anchor or incorrect value fails. References can make broad citations cheaper to submit;
they do not establish deeper understanding, a proof or an efficiency advantage.

The audit records reference-metadata bytes produced and submitted-answer bytes separately.
Those counters do not capture the full provider context. Task instructions must permit reference
citations; the older tasks explicitly required quotations. The new
[`source-references.json`](../tests/agent-eval/opencode/source-references.json) manifest changes
that format instruction for one already-reviewed cache task, with the same factual rubric.
It is an integration pilot, not an independent comparison.

The [retained six-attempt pilot](../tests/agent-eval/opencode/results/2026-10-02-source-references/README.md)
has two passes and four 120-second timeouts. Both passes used source IDs. Guided Kimi used fr names,
behavior and a continuation; the other passing attempt used only ordinary tools. The guided pass
delivered less source but produced more total tool output. This pilot does not establish an efficiency benefit.
The incomplete attempts have no complete usage accounting. The report retains every original failure.

## Make changes and grade the submitted code separately

`tools/native-changes.py` collects native OpenCode edits without executing candidate code locally.
Both arms have list, search, bounded reads and exact replacements. The `fr` arm also has compact
public discovery. An edit must name an existing UTF-8 file, supply its current whole-file hash,
and replace one unique exact string. Stale hashes, ambiguous matches, additions, deletions and
workspace growth beyond 1 MiB refuse. This comparison measures discovery plus ordinary edits;
it does not yet compare public `fr author` edit routes or delegated work.

Each attempt owns a stable private source path so public handles survive successive reads.
An accepted edit refreshes that snapshot and discards its old caches. The snapshot is inside the
monitored attempt directory. It is removed after the process group stops; transcripts and the
submitted regular-file bundle remain. No candidate code, public tests or private tests run locally.
OpenCode keeps the existing 120-second attempt, 12-step, 24-call and sampled process limits.

`submit_patch` binds the original and resulting snapshots and lists changed paths. A completed
collection is `submitted`, never `passed`. Replay checks the native stream, exported model/session
identity and ordered host calls, then reconstructs every accepted edit and verifies the submitted
files. It counts edit arguments, source overlap and tool results separately. CLI token counters
remain reported usage; full context, billing and host/cache costs remain incomplete.

```sh
python3 tools/native-changes.py freeze tests/agent-eval/opencode/changes/manifest.json --binary /path/to/fr
python3 tools/native-changes.py run PLAN CELL OUTPUT --binary /path/to/fr --opencode /path/to/opencode --confirm-agent-spend
python3 tools/native-changes.py replay PLAN OUTPUT
```

Use the workstation guard around local commands. Commit the plan and exact runner before calls.
Collection exposes only source, requirements and allowed tools to the model. Private grader code,
case inputs and expected results stay outside that tool surface. They are retained in the plan for
reproducibility, not claimed to be confidential after publication.

On GitHub, `grade PLAN OUTPUT` runs the pinned black-box cases through the existing network-free,
read-only, unprivileged Docker grader. The image digest, command, cases and limits are frozen before
collection. Grading uses the exact frozen runner and leaves original attempt records unchanged.
CI uploads the resulting grades separately. Agent failures remain results; broken grader controls,
changed evidence and invalid submissions fail CI.

The first task pack contains two reviewed tasks: empty signing-key collections in `itsdangerous`
and signal-name validation in `blinker`. The former project appeared in earlier explanations;
the latter is new to these trials. Each grader has an unchanged-source control, a reference fix and
three wrong-fix controls. The checks examine behavior, not patch equality. These integration cases
do not close independent task review, unfamiliar-project generalization or efficiency requirements.

The [first frozen collection](../tests/agent-eval/opencode/results/2026-10-02-code-changes/README.md)
retains seven behavior passes and one 120-second timeout. No attempt called fr.
The largest sampled process-group RSS was 684.6 MiB, with the workstation guard also active.
Separate GitHub containers checked the exact submitted files against the frozen cases. Both
unchanged-source controls failed, both reference fixes passed, and all six wrong fixes failed.
The original collection records remain ungraded; separate retained GitHub reports hold the results.
CI reruns those cases and compares their outcomes with the retained report. These checks do not
establish general correctness or an efficiency benefit.
