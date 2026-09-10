# Agent Context Protocol v3 evaluation

Protocol v3 projects the current portable skill and transaction-basis namespace onto the same
four passing coordinated trials used by v2. The projection is bound to the cohort manifest, the
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
