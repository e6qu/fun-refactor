# Agent Context Protocol v3 evaluation

Protocol v3 projects the current portable skill and transaction-basis namespace onto the same
four passing coordinated trials used by [v2](agent-context-v2-evaluation.md). The projection is bound to the cohort manifest, the
frozen v2 report, every measurement dependency and each loaded skill file. It audits every
changed request and response path against an embedded allowlist.

## Result

| Measure | `fr` | Ordinary files | Difference |
|---|---:|---:|---:|
| Normalized starting mean | 13,278.5 | 6,810 | 6,468.5 |
| Protocol v2 mean | 11,229 | 6,810 | 4,419 |
| Protocol v3 mean | 10,960 | 6,810 | 4,150 |

V3 removes another 269 mean tokens, or 2.4% of the v2 `fr` total. The complete fixed
projection is 2,318.5 tokens or 17.5% below the normalized starting mean. The remaining `fr`
premium is 60.9%.

The contribution is entirely the smaller five-file skill route: its projected reads fall from
1,962 to 1,693 tokens. Changing the transaction namespace from `frtb1` to `frtb2` changes no
token count when the opaque digest body is held fixed.

## Applicability boundary

The passing transcripts contain no author preview followed by an identical persistence call,
so reviewed-plan compaction contributes zero projected savings. Their exact-name lookups use
different scopes or options, so none can be combined into one `project select` call without
changing agent behavior. Multi-select also contributes zero here.

The projection preserves every recorded call, outcome, source state and timing. It does not add
synthetic calls to make new features look useful. Production tests separately establish exact
reconstruction and stale-basis refusal for multi-select, authoring, migration and transaction
reports. A fresh agent cohort is required to measure whether agents choose the new flows.

Frozen visible records do not contain the complete after-snapshot bytes needed to recompute a
production `frtb2` digest. V3 therefore keeps the frozen opaque digest body and measures only
the new prefix and field placement. This supports a serialization count, not identity
correspondence for that historical transaction.

## Exact changes

The allowlist covers current skill-read text, project-context fields, author transaction context,
compact successful check fields, patch-artifact metadata and compact redo fields. Each changed
event records original/projected request and response hashes, generalized JSON paths and the
exact request suffix. Any other change makes projection generation fail.

The retained report is
[`tests/agent-eval/context-protocol-v3.json`](../tests/agent-eval/context-protocol-v3.json).
Recompute byte evidence with the standard Python environment:

```sh
python3 tools/agent-context-protocol-v3.py
```

For the published token counts, install `tools/agent_eval/requirements.txt`, fetch the pinned
`o200k_base` vocabulary as documented in [agent acceptance](agent-acceptance.md#reproduction),
then run:

```sh
target/agent-eval-venv/bin/python tools/agent-context-protocol-v3.py --tokens
```

These counts exclude system context, hidden reasoning, cache effects and billed usage. They are
deterministic serialization evidence over prior successful behavior, not new autonomous-agent
results or a latency estimate.

## First fresh diagnostic pair

A fresh pair used Codex CLI 0.154.0 with `gpt-5.6-luna`, low reasoning effort and the default
service tier. Both runs were sequential and ephemeral, with user configuration and rules ignored.
Neither received a human correction.

| Arm | Accepted | Context tokens | Calls | Result |
|---|---|---:|---:|---|
| `fr` | No | 6,523 | 26 | No source change; the batch manifest used invalid operation names. |
| Ordinary files | No | 12,749 | 22 | Both behavior oracles passed; the original-state check was omitted. |

The failed pair supports no context comparison. Its complete evidence is retained in
[`2026-09-11-context-v3-diagnostic`](../tests/agent-eval/results/2026-09-11-context-v3-diagnostic/manifest.json).

The `fr` trace exposed a handoff regression: the compressed author reference described the
operations but no longer named `insert-declaration`. The harness also returned one broad error
before the CLI could report its accepted schema. The reference now names every operation, and
the harness refusal reports received kinds and exact expected postconditions.

## Fresh passing pair

A second fresh pair used the same archive, task, runner and model configuration after that
instruction correction. Neither run received a correction or restart. Both arms pass every
acceptance gate, including the independent 1,060-case project and receiver oracles, checks in the
original and changed states, exact undo and redo, patch delivery and Git-index preservation.

| Measure | `fr` | Ordinary files | Difference |
|---|---:|---:|---:|
| Context tokens | 15,458 | 11,600 | 3,858 (33.3%) |
| Prompt tokens | 1,324 | 1,133 | 191 |
| Tool-request tokens | 2,282 | 1,184 | 1,098 |
| Calls | 42 | 20 | 22 |
| Tool time | 119.114 s | 11.049 s | 108.065 s |
| Session elapsed time | 263.758 s | 92.137 s | 171.621 s |
| Refused or failed calls | 7 | 1 | 6 |

The context total follows the established protocol and counts the prompt plus visible tool
payloads. Tool-request tokens are reported separately. The `fr` output comprises 1,693 skill,
5,727 inspection, 2,413 check and 4,301 change-and-delivery tokens. The ordinary-file output
comprises 7,220 inspection, 2,487 check and 760 change-and-delivery tokens.

The `fr` agent adopted the compact reviewed-plan flow and the `frtb2` transaction basis. Its
extra calls came largely from help, path correction and repeated inspection. This pair therefore
establishes successful autonomous adoption and a 33.3% premium for one sample; it does not
establish parity or a population-level improvement over prior cohorts.

The scorer originally rejected the successful `fr` trace because it required `files_changed`
inside the compact saved response. The production protocol deliberately omits that field after a
reviewed preview. The evaluator now accepts it only when one earlier batch preview has the same
cryptographic plan basis and supplies the required two-file count; missing and conflicting
previews remain rejected.

Complete checksum-bound evidence is retained in
[`2026-09-11-context-v3`](../tests/agent-eval/results/2026-09-11-context-v3/manifest.json).
Its token audit and full patch replay pass without rerunning an agent.

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py audit-tokens tests/agent-eval/results/2026-09-11-context-v3
python3 tools/agent-eval.py replay tests/agent-eval/results/2026-09-11-context-v3
```
