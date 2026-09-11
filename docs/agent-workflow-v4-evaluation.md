# Agent workflow v4 evaluation

Roadmap PR 8 starts from the one fresh passing PR 7 `regex-escape-len` pair. The
`fr` arm completed the task with 15,458 measured context tokens, 42 instrumented calls
and seven refused or failed requests. The ordinary-file arm used 11,600 context tokens
and 20 calls. Both passed the independent project and receiver oracles, ordered checks,
exact reversal and index-preservation gates.

`tools/agent-workflow-v4.py` applies a prescribed workflow to that immutable `fr` trace.
It checks the retained evidence manifest and event-stream checksum, requires the accepted
42-call/15,458-token baseline, and then runs the current binary against the pristine pinned
workspace. The live query repeats the trace's broad `escape` lookup and feeds its current
handles to one `project select` request. Both exact declarations and their source must be
returned with `matched` status.

The projection removes thirteen calls:

- Three invalid `show`/`find` shapes now stated explicitly by the skill.
- Four single-symbol source lookups replaced by the successful two-handle selection.
- Two author help calls covered by the loaded author route and `author guide` contract.
- Two failed manifest attempts and one corrected manifest write replaced by the exact
  `fr_reference` values returned with the first artifact writes.
- One ordinary-file export attempt replaced by the history-patch route already loaded by
  the agent.

All source-changing calls, declared checks, plan preview/save/apply, patch export,
sentinel, undo/redo, receiver and finish steps remain in their original order. Current skill
payloads and the current harness prompt replace their frozen predecessors. The artifact
requests use the absolute paths returned by their writes. The resulting prescribed sequence
has 29 calls, no retained refusal, and 13,202 context tokens: 2,256 fewer than the observed
trace. It remains 1,602 tokens, or 13.8%, above the passing ordinary-file arm.

This is a counterfactual for one trace. It shows that the delivered commands can support the
shorter sequence and measures its serialized payloads. It does not show that an autonomous
agent will choose that sequence, predict latency or billed usage, or establish a population
effect. A fresh paired run is the adoption test.

## First fresh adoption diagnostic

The first PR 8 pair used Codex CLI 0.154.0 with `gpt-5.6-luna`, low reasoning and the default
service tier. Both agents made correct two-crate changes, passed the independent project and
receiver oracles, preserved both indexes and completed exact reversal. Both edited source before
running the original-state checks, so the ordered-workflow gate rejected both trials.

| Arm | Accepted | Context tokens | Calls | Refusals or failures |
|---|---:|---:|---:|---:|
| `fr` | No | 23,973 | 49 | 12 |
| Ordinary files | No | 13,852 | 21 | 1 |

The common failure showed that the harness stated the ordering invariant but enforced it only
after completion. It now refuses source-changing requests until every declared original-state
check passes. The `fr` failures also identify five over-limit skill reads, four duplicated
executable prefixes, one malformed `--contains` call, one project basis used as a plan basis and
one unavailable file-list request. The prompt and portable routes now state those boundaries.
The first placeholder manifest write also prompted an explicit single-final-manifest instruction.

The complete failed evidence remains at
[`2026-09-11-workflow-v4-diagnostic-1`](../tests/agent-eval/results/2026-09-11-workflow-v4-diagnostic-1/manifest.json).
Its token audit passes, while replay refuses it because acceptance failed. No agent received a
human correction or restart.

The token audit uses tiktoken 0.12.0, `o200k_base`, and the repository's checksum-pinned
vocabulary. Reproduce the retained report with:

```sh
target/agent-eval-venv/bin/python tools/agent-workflow-v4.py --fr target/debug/fr --tokens
```

The default command omits token fields and runs in ordinary deterministic CI without the
optional tokenizer package. The acceptance regression recomputes live byte and call evidence,
then compares those fields, every removal reason and each measurement-file checksum with
`tests/agent-eval/agent-workflow-v4.json`.
