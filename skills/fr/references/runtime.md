# Keep agent state in Python

Use this route when Python and the zero-dependency `fr_ir` package are available. It keeps complete
reports local; print or inspect only the fields needed for the decision.

```python
from fr_ir.runtime import FrClient
client = FrClient(".")
found = client.project("find", "render", "--signature")
handle = found.at("/rows/0/0")
```

Start a local progressive workspace with `session = client.context(handle, view="semantic" |
"evidence" | "project")`. Use `session.materialize_section("code_map" | "call_traces" | "impact" |
"sources_and_sinks")`, or `session.materialize(POINTER)`. It follows exact returned actions,
reconstructs pages under one 64-call ceiling and verifies the advertised Merkle digest. It does not
fetch source unless the selected branch contains the exact-source action.

Use `session.packet({"name": POINTER}, max_bytes=4096)` to expose only selected values plus their
revision, view and object identity. `MemoryObjectStore` caches in process. A
`DirectoryObjectStore(PATH)` can persist reusable objects; keep `PATH` outside the analyzed project
so cache writes do not invalidate its handles.

Prefer `client.compile(AgentIntent(handle, PURPOSE))` when the goal is `understand`, `trace`,
`change`, `migrate` or `prove`. Import `AgentIntent` and optional `IntentNeed` from `fr_ir.intent`.
Native `fr` expands the purpose and selects the evidence in one project snapshot. It returns one
bounded `CompiledIntent` and a Merkle object digest for each selection. Use explicit needs and
pointer suffixes when the default section values would exceed the packet budget. Use
`client.prepare` only to exercise the progressive action protocol or compare both implementations.

To join evidence and one reviewed write, wrap a direct `TaskChange` in `IntentAction`, pass it to a
`change` intent and inspect `/action/review/author/diff`. Then call
`client.execute_intent(compiled)`. The task must have no project requests and its only literal
target must equal the intent target. Native `fr` accepts only the unchanged returned `fraa1:` basis
before it runs checks, requested reversal and patch delivery.

Construct `TaskChange`, `TaskTarget` and `TaskDelivery` as shown in [Task](task.md). Preview with
`review = client.review(change)`, inspect selected values with `review.at(POINTER)`, then call
`client.execute(review)`. The runtime checks retained manifest and preview identities. Rust
independently rebuilds the complete basis and refuses drift before mutation.

Catch `FrRuntimeError` and preserve its `arguments`, `exit_code` and `report`. Do not turn a clipped,
omitted, stale or failed report into success.
