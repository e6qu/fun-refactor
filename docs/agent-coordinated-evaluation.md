# Coordinated agent evaluation on regex

Four fresh agents completed two paired trials of `regex-escape-len` on the pinned regex workspace.
All four pass independent acceptance. Mean retrieved context is 2.7% lower for fr, with near parity in the first pair.
Verbose check output from both ordinary-file agents accounts for much of this comparison; it does not establish a general context advantage.
The [task preparation](agent-workspace-evaluation.md#coordinated-task-preparation) defines the API contract, independent oracle and edit boundaries.
The prescribed rehearsal supplies infrastructure evidence and does not count as an agent trial.

## Paired results

| Repetition | Tool surface | Passed | Retrieved context tokens | Tool calls | Trial seconds |
|---|---|---|---:|---:|---:|
| 1 | fr | Yes | 13,539 | 29 | 215.6 |
| 1 | Ordinary files | Yes | 13,487 | 19 | 70.9 |
| 2 | fr | Yes | 13,018 | 28 | 186.6 |
| 2 | Ordinary files | Yes | 13,818 | 22 | 107.0 |

Mean retrieved context is 13,278.5 tokens for fr and 13,652.5 for ordinary files.
The fr arm uses 0.4% more context in repetition one and 5.8% less in repetition two.
All four have zero recorded refusals or tool failures and zero human task corrections.
No agent trials were interrupted, restarted or excluded, and no agent received implementation guidance after its initial prompt.

Both fr agents saved one three-step batch covering the two source files, then applied, exported, undid and redid that transaction.
Both file agents made three source edits and delivered one combined Git patch.
All trials pass declared checks in original, changed, undone and redone states, in order.
Source bytes and modes restore exactly, unrelated edits survive, and project and receiver indexes stay unchanged.
All four changed projects and all four patch receivers pass the independent 1,060-input byte/allocation oracle.
Only the two permitted source files change.

The [retained manifest](../tests/agent-eval/results/2026-09-08-coordinated/manifest.json) binds all prompts, transcripts, scores, patches and frozen skill files by checksum.
It also records execution order, runtime details and evaluator fingerprints.

## Where context went

| Visible output category | fr r1 | Files r1 | fr r2 | Files r2 |
|---|---:|---:|---:|---:|
| Skill reads | 2,851 | 0 | 2,851 | 0 |
| Project/source inspection | 3,428 | 2,768 | 3,200 | 2,135 |
| Declared checks | 2,133 | 8,974 | 2,133 | 9,967 |
| Changes and delivery | 4,202 | 847 | 3,909 | 818 |

These categories exclude the prompt: 925 tokens per fr trial and 898 per file trial.
Generated requests add 1,503 and 1,491 tokens for fr, and 859 and 890 for files; the retrieved-context totals exclude them.
Both fr agents read the core skill and authoring, checks, history and Git references.
Both used bounded lookup, `checks --quiet-success --no-declarations` and `--no-diff` for history writes after review.
Both file agents used default check output; the second also repeated the check listing before later stages.
These were agent choices within the shared check interface, with compact-check guidance available through the fr skill.

The fr agents spend more tokens on inspection and changes/delivery than their paired file agents, before adding skill reads.
The smaller check reports offset those costs in the overall totals.
The study evaluates the complete tool-and-skill workflow and cannot isolate a causal benefit from batching alone.
Because the task changed, comparing these totals with earlier cohorts does not isolate the intervening tool changes.
The [prescribed batch comparison](project-context-evaluation.md#coordinated-authoring-measurement) remains a separate experiment with a fixed command sequence.
The next comparison should control check-output policy across both arms and examine repeated inspection and transaction metadata.

## Matched check-output projection

The retained transcripts permit a controlled projection because both arms use the same structured checks command.
The projection leaves every prompt, request, check listing, call and non-check payload unchanged.
It changes only the four executed check reports in each trial and retains the original scores alongside the projected totals.

| Policy | fr r1 | Files r1 | fr r2 | Files r2 | fr mean | Files mean | fr difference |
|---|---:|---:|---:|---:|---:|---:|---:|
| Quiet success | 14,455 | 7,562 | 13,934 | 7,890 | 14,194.5 | 7,726 | 83.7% more |
| Quiet success, declarations omitted | 13,539 | 6,646 | 13,018 | 6,974 | 13,278.5 | 6,810 | 95.0% more |

The first policy suppresses stdout and stderr from successful checks but retains declarations in every execution report.
The second also removes the check list and each result's `argv`, `cwd` and `covers` fields after a matching listing has established the same basis.
The extra listing requested by the second file agent remains in both projections.
Under the compact policy, execution and listing output is 2,133 tokens in the first three trials and 3,123 in file repetition two because of those extra listings.

Compared with their recorded totals, the file trials lose 5,925 and 5,928 tokens when successful streams are suppressed.
Omitting already reviewed declarations saves another 916 tokens in every trial.
The fr trials already used the second policy, so their compact totals are unchanged.
The resulting mean gap is 6,468.5 tokens in favor of the ordinary-file workflow on this fixed action sequence.

This result explains the apparent aggregate advantage in the original scores: retained successful check logs were large enough to mask the skill, inspection and transaction payloads in the fr arm.
It does not replace the measured outcomes or show how an agent would behave if both prompts prescribed the same policy.
It also does not isolate individual fr commands; the next controlled work should target repeated project and authoring metadata while retaining their reviewed source bases and guards.

The projection requires the complete checksum-valid cohort, an untruncated structured check payload and a prior listing with matching basis, root, configuration and declarations.
It rejects contradictory success states and preserves failed output, exit status, timeouts, capture limits, errors and raw stream byte totals.
A live fixture with successful and failing checks matches actual CLI reports under both policies, apart from elapsed time, including invalid UTF-8 replacement and bounded failure diagnostics.
The retained [projection report](../tests/agent-eval/checks-policy-context.json) includes each transformed execution and source checksum.

## Context Protocol v2 projection

The follow-up [protocol projection](../tests/agent-eval/context-protocol.json) starts from the same immutable four trials. It first applies the compact shared check policy above. It then substitutes the current requested skill files and adds only the production context-basis fields and matching request flags.

The projected fr mean falls from 13,278.5 to 11,184 tokens. The normalized file mean remains 6,810 tokens, leaving a 4,374-token gap. The current protocol therefore saves 2,094.5 mean fr tokens, or 15.8%, on this fixed action sequence. It does not yet make the workflows context-competitive.

The projection changes patch export to `--output ../artifacts/change.patch`. The artifact retains the exact patch while the visible report carries its identity and byte count. The retained transcripts still contain the original patch text and supply the projection's hash and size.

The report separates prompt, skill, inspection, checks, authoring, delivery, requests and recorded tool time. Prompts, tool-call counts, outcomes, source states and timings stay unchanged. This remains a fixed projection; fresh agents may select a different action sequence.

Reproduce it with the pinned tokenizer:

```sh
target/agent-eval-venv/bin/python tools/agent-context-protocol.py --tokens
```

Measured fr tool time is 73.0 and 82.8 seconds, versus 8.7 and 10.9 seconds for files.
Project and author commands account for 62.9 and 70.4 seconds of the fr totals under the disabled-cache policy.
The shared-host trial times above include agent work between calls and are not production latency guarantees.

## Task and cohort

The task requires changes to `src/lib.rs` and `regex-syntax/src/lib.rs`.
Both crates must expose an allocation-free escaped byte-length API, and the lower crate must preallocate its escaped output.
Existing escaping and append behavior, documentation requirements and builds without default features must remain valid.
Both arms receive the same source, task, permitted files and declared project checks.
The fr arm uses one saved authoring batch and its history transaction; the other arm uses ordinary file edits and Git patches.

The complete upstream snapshot contains seven packages, 227 Rust files and 5,553,380 Rust source bytes.
Preparation uses the existing pinned archive and dependency lock, without injected source or artificial background files.
All four preflights pass upstream checks and reject the original workspace for its missing requested APIs.
The original tracked snapshots match across sessions.

The frozen CLI is the validated M4aa binary, SHA-256 `ad3cf2a6e3fc31b33079f289eab9630131f0a8f5cb37b847b41ca99d16b40375`.
Preparation commit is `1c96eb1`.
The harness, oracle, prompts and skill snapshots remain frozen across both pairs.
The fr fact cache stays disabled, matching the earlier autonomous cohorts.

## Execution and measurement

Each agent starts without conversation history and reads only its assigned generated prompt before using the instrumented tools.
All four inherit the same parent model and effort, with no overrides or pinned serving-model build available.
Agents receive no rehearsal solution or other trial results. The second pair starts after the first pair finishes.
The boundary is cooperative, rather than an operating-system sandbox.

Each pair shares a host and dependency cache, with preflight-warmed project builds.
The parent reviews protocol metadata and edits documentation during the trials.
Full repository checks and scoring wait until all four agents finish, so they do not compete with active trials.
Tool times include uncached project analysis and cannot establish a general latency comparison.

Retrieved context counts the generated prompt and each visible instrumented payload once, including skill reads.
Generated requests have separate token counts. System context, hidden reasoning, commentary and transport framing are outside the measurement.
The count does not measure repeated prefix processing, caching or billed usage.
Scoring uses the existing pinned tiktoken 0.12.0 and `o200k_base` vocabulary.

The independent oracle checks 1,060 inputs through both crates' APIs in each changed project and its patch receiver.
Required workflow evidence includes original/changed/undone/redone project checks, exact source and mode restoration, unchanged indexes and an unrelated-file sentinel.
The fr score additionally requires one saved two-file batch and its matching apply, undo, redo and patch commands.
Two repetitions of one task on one host cannot establish general context efficiency or a reliable success rate.

## Reproduction and validation

Prepare new sessions with `agent-eval.py prepare --project regex-coordinated --repetitions 2`, as described in the task protocol.
Use fresh agents for future trials; replay checks the retained evidence without rerunning agents.

```sh
python3 tools/agent-eval.py replay tests/agent-eval/results/2026-09-08-coordinated
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-08-coordinated
```

Replay requires the pinned workspace dependencies, Cargo, Rust and Git; token auditing additionally needs the pinned tokenizer environment.
The explicit workspace replay test covers this cohort and the earlier regex cohort after dependency preparation.
Behavioral replay and exact token auditing pass for all four retained trials.
The full native/WASM repository gate passes, including 311/311 capability coverage and documentation checks.
Historical evidence bundles remain unchanged.

Reproduce the fixed projection with the frozen binary and pinned tokenizer:

```sh
target/agent-eval-venv/bin/python tools/checks-policy-context.py --tokens --fr target/agent-eval-bin/fr-m4ab
```
