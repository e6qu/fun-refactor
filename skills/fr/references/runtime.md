# Keep agent state in Python

Keep large reports in Python and return only the fields needed for the next decision. For example, show one relevant caller instead of the whole call report.

The SDK is the Python library invoking `fr`. Reports stay in the Python process; a pointer selects the fields sent to the agent.

Use this route when Python and the zero-dependency `fr_ir` package are available. It keeps complete
reports local; print or inspect only the fields needed for the decision.

The following lookup requires an installed `fr_ir` package and a project with one function named `render`.
Run it from that project folder. An absent or ambiguous definition refuses; choose the intended function before continuing.

```python
from fr_ir.runtime import FrClient
client = FrClient(".")
found = client.project("find", "render", "--signature")
handle = found.definition_target().handle
```

Continue with `session = client.context(handle, view="evidence")`.
This keeps report pages in Python while your program opens the needed sections.
Use `session.materialize_section("code_map")` to obtain that section; other section names include
`call_traces`, `impact` and `sources_and_sinks`.
The session follows returned commands, checks each page's contents and stops at its 64-call limit.
Its Merkle digest fingerprints the report tree; that checks stored contents, not program correctness.
It fetches source only through the selected section's returned exact-source action.

Use `session.packet({"name": POINTER}, max_bytes=4096)` to expose only selected values plus their
revision, view and object identity. `MemoryObjectStore` caches in process. A
`DirectoryObjectStore(PATH)` can persist reusable objects; keep `PATH` outside the analyzed project
so cache writes do not invalidate its handles.

Use `client.compile(AgentIntent(handle, PURPOSE))` for `understand`, `trace`, `change`, `migrate` or `prove`.
Import `AgentIntent` and optional `IntentNeed` from `fr_ir.intent`.
This selects report fields in one native request. Set explicit needs and pointers to keep the result within its byte limit.
Use `client.prepare` when testing the page-by-page protocol or comparing the two routes.

For intent-bound operations, read [Intents](intents.md). Use `TaggedIntentAction` from
`fr_ir.intent_actions` to bind tasks, recipes, direct capabilities, migration and proof delivery.

Construct `TaskChange`, `TaskTarget` and `TaskDelivery` as shown in [Task](task.md). Preview with
`review = client.review(change)`, inspect selected values with `review.at(POINTER)`, then call
`client.execute(review)`. The runtime checks retained manifest and preview identities. Rust
independently rebuilds the complete basis and refuses drift before mutation.

Catch `FrRuntimeError` and preserve its `arguments`, `exit_code` and `report`. Do not turn a clipped,
omitted, stale or failed report into success.
