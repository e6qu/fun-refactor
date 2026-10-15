# Inspect an application before moving it

For a route migration, inspect its URL, required inputs, handler and response together.
A route maps an HTTP method and URL to a handler function. A schema describes expected data.
IR means the shared structured record used to describe supported application behavior.

Run `fr project application [TARGET] [--feature ID]` to read that record.
`children` groups related facts. Read `omissions` and each adapter's `status` and `reason`.
An unknown handler remains `manual`; recognizing its URL does not supply its implementation.

For a smaller section, obtain a full handle with `project map`, then run:

Run `fr project disclose '<HANDLE>' --view application`.

Follow returned `application_shortcuts` and continuations. The Python client uses
`client.context(handle, view="application")`; `ApplicationIr.from_data(...)` checks typed records.

## Describe a small HTTP response

This Python example writes an input file for a GET endpoint whose response repeats its path argument.
It requires the `fr_ir` Python package. `Object` builds a JSON object; `Path("id")` reads `{id}`.

```python
from pathlib import Path as FilePath
from fr_ir.application import HttpRoute, Object, Path, RouteBundle

RouteBundle([
    HttpRoute("GET", "/records/{id}", 200, Object({"id": Path("id")})),
]).write(FilePath("application.json"))
```

The file must be part of the analyzed project before obtaining a review. Existing output refuses replacement.
Preview generation with:

Run `fr migrate application --ir application.json --to fastapi --out generated`.

Inspect the whole diff. Use its returned plan basis for `--save-plan` or `--write`.
Use the resulting transaction with [checks](checks.md) and [history](history.md).
Generation supports selected FastAPI, Next.js, Express and Go HTTP forms. React does not accept HTTP generation.
`--project TARGET` can read the application directly instead of using an intermediate file.

## Keep unsupported behavior visible

Responses can use literals, path values, validated inputs, objects, arrays and supported sibling service calls.
Middleware wraps request handling; dependencies supply values before a handler runs.
Configured middleware, unknown providers and external services need separate source and behavior checks.
Portable FastAPI generation refuses middleware chains.

Routes require literal ASCII segments and unique `{name}` parameters. Overlapping routes, HEAD,
bodyless statuses, floating-point literals and integers outside ±(2^53−1) refuse.
Bundles accept 1–256 routes and 1 MiB; expressions accept 1,024 nodes and depth 32.

Generated files require new owned paths. Framework registration and dependencies need explicit supported integration.
`cutover=True` removes an original feature only after ownership, integration and external-reference checks.
Mixed source files refuse. Keep `runtime_proved: false`: generation and model proofs do not establish deployment correctness.
