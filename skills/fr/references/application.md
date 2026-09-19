# Application hierarchy and HTTP authoring

Use `project application [TARGET] [--feature ID]` to obtain an application IR.
Nodes retain fact identity, source confidence, gaps and reader omissions. `children`
expresses the hierarchy. Admitted literal JSON/path handlers from Next.js, FastAPI,
Express and Go HTTP carry a portable `route`. Other route and component boundaries
stay explicit and manual.

For bounded inspection, obtain a full map handle and start
`project disclose HANDLE --view application`. Follow an exact `application_shortcuts`
continuation, then reveal only the required children. The SDK accepts
`client.context(handle, view="application")`; `ApplicationIr.from_data(...)` mirrors
the hierarchy and independently computes its object digest. Source-reader omissions
remain unknown.

For explicit JSON HTTP behavior, construct the IR with Python:

```python
from pathlib import Path as FilePath
from fr_ir.application import HttpRoute, Literal, Object, Path, RouteBundle

RouteBundle([
    HttpRoute("GET", "/records/{id}", 200, Object({"id": Path("id")})),
]).write(FilePath("application.json"))
```

The constructors mirror `fr-http-application-1`: a bundle of `routes`, an optional
`middleware` chain of dotted names with unique 1-based `request_order`, each route
with `method`, `path`, `status`, `response` and optional `inputs`/`dependencies`.
Expressions are `Literal(value)`, `Path(name)`, `Input(name)`, `Object(fields)`,
`Array(items)` or `Service(method, path)` resolving to one sibling route.
The IR file must be captured in the project snapshot. The SDK refuses overwriting.

Preview with `migrate application --ir application.json --to fastapi --out generated`.
The input may also be the complete JSON output of `project application`; its model
digest is rechecked and its normalized routes feed the same writers.
Prefer `migrate application --project TARGET --to ADAPTER --out DIRECTORY` when no
intermediate artifact is needed. Targets also include `nextjs`, `express` and
`go-net-http`. React refuses HTTP authoring. Retain the exact plan basis before
`--save-plan` or `--write`. Use the returned history transaction for declared checks,
apply, patch export, undo and redo.

React and Next.js components share one smaller subset: intrinsic lowercase tags,
literal string attributes, explicit text, and `useState` declarations with literal
set or toggle `on*` events behind an explicit client boundary. Props, effects,
computed handlers, styles, spreads and component calls stay manual. Select one
feature branch before conversion when several components exist.

Generation requires new owned paths. FastAPI can add one explicit app mount and PEP
621 dependency edit in the transaction. Express can add a recognized TypeScript
app/router mount and exact npm dependency. Go can generate an owning-package mount
for an explicit ServeMux beneath `go.mod`. Recognized Next.js `app` placement
connects by convention. Existing source remains.

For an admitted navigator route, use the advertised `application-migration` tagged
intent operation; the selected intent handle supplies `--project`. Use `cutover=True`
only for one recognized wholly owned source feature after target integration and
external-reference checks. Mixed application files refuse.

The application report's adapter rows cover every source, target and feature cell.
Inspect `status` and `reason`; do not infer support from a shared host language.

Paths use literal ASCII segments and unique `{name}` parameters. Overlapping routes,
HEAD, bodyless statuses, floating-point literals and integers outside ±(2^53−1)
refuse. Use object/array expressions for aggregates. Bundles admit 1..256 routes
and 1 MiB; expressions admit 1024 nodes and depth 32.

Lean proves policy admission and agreement. Runtime fixtures test generated behavior.
Neither establishes general source equivalence, configured middleware, provider
internals, external services or deployment correctness; preserve
`runtime_proved: false` and every conversion boundary.
