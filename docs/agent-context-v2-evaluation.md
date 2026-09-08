# Agent Context Protocol v2 evaluation

Agent Context Protocol v2 reduces repeated structured output and makes local Codex evaluation reproducible. It does not yet establish context parity with ordinary file tools. The fixed passing-cohort projection saves 15.4% of mean `fr` context, while the latest fresh low-effort pair failed acceptance for independent agent mistakes.

## Fixed passing-cohort projection

The checksum-bound [projection](../tests/agent-eval/context-protocol.json) starts from the four passing M4ab trials. It preserves their prompts, calls, outcomes, source states and timings, applies the shared compact-check policy, substitutes the current skill files, and changes only documented request and response fields.

| Measure | `fr` | Ordinary files | Difference |
|---|---:|---:|---:|
| Normalized starting mean | 13,278.5 | 6,810 | 6,468.5 |
| Protocol v2 projected mean | 11,229 | 6,810 | 4,419 |
| `fr` reduction | 2,049.5 (15.4%) | — | — |

The remaining `fr` premium is 64.9%. The projection is deterministic evidence about serialization costs. It does not predict agent choices or count system context, hidden reasoning, cache effects or billed usage.

The projected `fr` mean consists of 1,962 skill tokens, 2,935 inspection tokens, 2,133 checks tokens, 1,466 authoring tokens, 1,838 delivery tokens and 925 prompt tokens. Skill loading and reviewed authoring/delivery receipts account for most of the remaining difference from the file workflow.

## Fresh low-effort evaluation

Four development pairs exercised the coordinated `regex-escape-len` task with fresh ephemeral Codex CLI sessions. Each run ignored user configuration and repository rules, used low reasoning effort and received no human task correction. These attempts exposed retry idempotency and digest-copying defects that are fixed in the protocol, but no post-fix pair passed both arms.

| Attempt | Model | `fr` result | Files result |
|---|---|---|---|
| Initial | `gpt-5.6-luna` | Correct code and receiver; missed the original-state check | Correct code and receiver; verified the receiver before the final local check |
| Calibration | `gpt-5.6-terra` | Correct ordered workflow; saved the same successful plan twice | Passed, 10,914 context tokens |
| Retry | `gpt-5.6-luna` | Correct code and one transaction; copied a 63-character check basis twice | Passed, 15,507 context tokens |
| Retained diagnostic | `gpt-5.6-terra` | Correct code and receiver; saved an incomplete plan before the complete plan and omitted the sentinel | Ordered workflow; incomplete metacharacter handling failed the independent oracle |

The first three are development observations from local working sessions and are not immutable repository evidence. The last pair is retained in the [diagnostic manifest](../tests/agent-eval/results/2026-09-08-context-v2/manifest.json), including prompts, harness events, Codex JSONL, final messages, stderr, run commands, exact model settings, scores, patches and a frozen skill copy.

| Retained trial | Accepted | Context tokens | Calls | Request tokens | Agent seconds | Project oracle | Receiver oracle |
|---|---|---:|---:|---:|---:|---|---|
| `fr` | No | 15,979 | 40 | 2,617 | 275.5 | Pass | Pass |
| Ordinary files | No | 11,025 | 21 | 1,086 | 136.9 | Fail | Fail |

The `fr` score has 2,006 skill, 4,564 inspection, 2,445 check and 5,872 change/delivery output tokens. Its five refused calls include invalid artifact paths; it ultimately produced correct source and a correct receiver. The score rejects it because exact reversal evidence requires the unrelated-file sentinel and coordinated delivery requires one saved two-file plan.

The file score passes declared project checks at all required states, exact reversal, patch application and index checks. The independent 1,060-case oracle finds an escaped UTF-8 byte-count mismatch in both the project and receiver. This demonstrates why compiler and project checks remain separate from behavioral acceptance.

Because neither retained trial passed, their token difference is diagnostic only. It cannot support a context-efficiency claim or replace the earlier passing cohort.

## Reproduction and boundary

Audit the retained hashes and token counts without invoking an agent:

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-08-context-v2
```

`agent-eval.py replay` deliberately refuses this cohort at its first failed score. Passing historical cohorts remain replayable through the commands in [coordinated evaluation](agent-coordinated-evaluation.md#reproduction-and-validation).

Future paid trials should follow a material tool or protocol change. Repeating the same low-effort prompt would mostly sample model variance. A later acceptance claim requires a complete paired cohort in which both arms pass the ordered workflow, independent project and receiver oracles, exact undo/redo and index-preservation checks.
