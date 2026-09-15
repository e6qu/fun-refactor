# Keep agent state in Python

Use this route when Python and the zero-dependency `fr_ir` package are available. It keeps complete
reports local; print or inspect only the fields needed for the decision.

```python
from fr_ir import FrClient
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

Construct `TaskChange`, `TaskTarget` and `TaskDelivery` as shown in [Task](task.md). Preview with
`review = client.review(change)`, inspect selected values with `review.at(POINTER)`, then call
`client.execute(review)`. The runtime checks retained manifest and preview identities. Rust
independently rebuilds the complete basis and refuses drift before mutation.

Catch `FrRuntimeError` and preserve its `arguments`, `exit_code` and `report`. Do not turn a clipped,
omitted, stale or failed report into success.
