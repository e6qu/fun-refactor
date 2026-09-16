# Application hierarchy and HTTP authoring

Use `project application [TARGET] [--feature ID]` to obtain an application IR.
Nodes retain fact identity, source confidence, gaps and reader omissions. `children`
expresses the hierarchy. Admitted literal JSON/path handlers from Next.js, FastAPI,
Express and Go HTTP carry a portable `route`. Other route and component boundaries
stay explicit and manual.

For bounded inspection, obtain a full map handle and start
`project disclose HANDLE --view application`. Follow an exact `application_shortcuts`
continuation, then reveal only the required children. The SDK accepts
`client.context(handle, view="application")`; selected subtrees use its checked
Merkle object-store interface. `ApplicationIr.from_data(...)` mirrors the returned
hierarchy and independently computes its object digest. Use source handles with evidence disclosure
for code maps, calls, impact or sources/sinks. Source-reader omissions remain unknown.

For explicit JSON HTTP behavior, construct the IR with Python:

```python
from pathlib import Path as FilePath
from fr_ir.application import HttpRoute, Literal, Object, Path, RouteBundle

RouteBundle([
    HttpRoute("GET", "/records/{id}", 200, Object({"id": Path("id")})),
]).write(FilePath("application.json"))
```

The constructors mirror `fr-http-application-1`: a bundle of `routes`, each with
`method`, `path`, `status` and `response`. Expressions are `Literal(value)`,
`Path(name)`, `Object(fields)` or `Array(items)`. The IR file must be captured in
the project snapshot. The SDK refuses overwriting an existing authored file.

Preview with `migrate application --ir application.json --to fastapi --out generated`.
The input may also be the complete JSON output of `project application`; its model
digest is rechecked and its normalized routes feed the same writers.
Targets also include `nextjs`, `express` and `go-net-http`. React refuses HTTP
authoring. Retain the exact plan basis before `--save-plan` or `--write`. Use the
returned history transaction for declared checks, apply, patch export, undo and redo.

Generation requires new owned paths. Integration is manual: mount generated routers,
declare framework dependencies or choose the Next.js app directory explicitly.
Existing source cutover and registration use the earlier checked Next.js/FastAPI
feature workflow where applicable.

Paths use literal ASCII segments and unique `{name}` parameters. Overlapping routes,
HEAD, bodyless statuses, floating-point literals and integers outside ±(2^53−1)
refuse. Use object/array expressions for aggregates. Bundles admit 1..256 routes
and 1 MiB; expressions admit 1024 nodes and depth 32.

Lean proves policy admission and agreement. Runtime fixtures test generated behavior.
Neither establishes general source equivalence, middleware behavior or deployment
correctness; preserve `runtime_proved: false` and every conversion boundary.
