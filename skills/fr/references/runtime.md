# Keep agent state in Python

Use this route when Python and the zero-dependency `fr_ir` package are available. It keeps complete
reports local; print or inspect only the fields needed for the decision.

```python
from fr_ir import FrClient
client = FrClient(".")
found = client.project("find", "render", "--signature")
handle = found.at("/rows/0/0")
```

Start progressive disclosure with `client.disclose(handle, view="semantic" | "evidence" |
"project")`. Select an exact returned continuation with `report.actions(domain=...)` and pass that
`DisclosureAction` to `client.follow`. This cannot invent a reveal command and does not fetch source
unless you select the exact-source action.

Construct `TaskChange`, `TaskTarget` and `TaskDelivery` as shown in [Task](task.md). Preview with
`review = client.review(change)`, inspect selected values with `review.at(POINTER)`, then call
`client.execute(review)`. The runtime checks retained manifest and preview identities. Rust
independently rebuilds the complete basis and refuses drift before mutation.

Catch `FrRuntimeError` and preserve its `arguments`, `exit_code` and `report`. Do not turn a clipped,
omitted, stale or failed report into success.
