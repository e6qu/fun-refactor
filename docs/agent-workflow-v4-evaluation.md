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
has 29 calls, no retained refusal, and 13,104 context tokens: 2,354 fewer than the observed
trace. It remains 1,504 tokens, or 13.0%, above the passing ordinary-file arm.

This is a counterfactual for one trace. It shows that the delivered commands can support the
shorter sequence and measures its serialized payloads. It does not show that an autonomous
agent will choose that sequence, predict latency or billed usage, or establish a population
effect. A fresh paired run is the adoption test.

The token audit uses tiktoken 0.12.0, `o200k_base`, and the repository's checksum-pinned
vocabulary. Reproduce the retained report with:

```sh
target/agent-eval-venv/bin/python tools/agent-workflow-v4.py --fr target/debug/fr --tokens
```

The default command omits token fields and runs in ordinary deterministic CI without the
optional tokenizer package. The acceptance regression recomputes live byte and call evidence,
then compares those fields, every removal reason and each measurement-file checksum with
`tests/agent-eval/agent-workflow-v4.json`.
