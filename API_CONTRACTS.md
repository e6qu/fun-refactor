# HTTP contracts and framework translation

A framework migration must preserve the behavior its callers rely on.
The current tool extracts declared HTTP contracts, converts selected route handlers,
and generates service skeletons from OpenAPI documents.
The [roadmap](PLAN.md) extends these capabilities into incremental project migration.

## Contract surface

| Element | Evidence |
|---|---|
| URL template | Router declarations or framework directory conventions |
| HTTP method | Router method, annotation or exported handler |
| Path and query parameters | Declared route fields and recognized parameter access |
| Request schema | Declared types, supported schema validators or an OpenAPI document |
| Response schema | Explicit declarations where the reader supports them |
| Status codes | Recognized returns and contract declarations |

Missing declarations remain missing. An inferred skeleton must not imply complete behavior.
Authentication, middleware order, side effects and dependency behavior need additional evidence.

## Implemented readers and writers

`fr openapi` reads supported route patterns from Next.js, FastAPI, Express, Flask, axum, gin and Spring.
Recognition of a route does not imply whole-framework translation support.

| Conversion | Current scope |
|---|---|
| Next.js to FastAPI | App Router handlers, selected server functions, route parameters, recognized models and handler bodies |
| FastAPI to Next.js | Decorated endpoints and models rendered into an App Router route tree |
| OpenAPI to FastAPI | Router skeletons and supported schema models |
| OpenAPI to Next.js | Route skeletons and supported TypeScript shapes |

Next.js route extraction uses directory structure as well as source text.
A handler under `app/api/users/[id]/route.ts` names a parameterized endpoint.
Catch-all path segments need the target framework's catch-all behavior.
The converter handles these supported forms and reports unsupported constructs.

## Working with a contract

```sh
fr openapi --yaml
fr translate app/api/pets/route.ts fastapi
fr translate app.py nextjs
fr translate openapi.yaml fastapi
```

These commands preview output. Use `--write` only when the planned result is suitable.
Consult `fr translate --help` for output paths and overwrite options.
[CLI.md](CLI.md#write-guarantees) describes commit recovery and its limits.

Record the source contract before migration and compare it with the result.
Review methods, paths, parameter types, validation behavior, responses and status codes.
Compile and run the result with its project dependencies.
Add request/response tests for behavior the static contract does not describe.

A status code in a handler and a status code in generated OpenAPI may differ.
FastAPI contract output depends on decorator metadata as well as returned responses.
Review the converter's notes before accepting the generated contract.

## Worked fixture

`tests/petstore/` contains a Next.js App Router API with eight route files.
It covers nested resources, query parameters, schema validation and catch-all paths.
`tests/scaffold_petstore.yaml` supplies a contract for scaffold tests.
The fixture and translation tests exercise supported routes without claiming a complete application migration.

Useful gates include `tests/translate_nextjs_routes.rs`, `tests/nextjs.rs`,
`tests/server_functions.rs`, `tests/scaffold_from_openapi.rs` and `tests/routes.rs`.
The broader translation suite compiles and executes selected results with real toolchains.

## What still needs project knowledge

Handler-body translation uses the shared code IR.
Foreign libraries, database access, authentication providers and framework services may require manual implementation.
A scaffold starts that work; it cannot derive an implementation from a schema alone.

Frontend framework migration needs additional models for components, state, effects, rendering and lifecycle behavior.
Backend migration needs middleware ordering, dependency injection and authentication boundaries.
Adapters must state supported patterns and preserve everything they cannot translate as visible work.

## Verification and migration plans

The planned workflow selects a feature or route group, records its contract and reports feasibility.
The resulting change plan should include generated files, dependency updates, unresolved decisions and validation evidence.
The same plan should support patch export, apply and undo/redo.

Formal properties can cover supported URL mappings, schema transformations and selected handler lowerings.
A proof about these models needs a correspondence argument before it establishes implementation behavior.
Contract equality alone does not prove equivalent authentication, persistence or runtime effects.
See [docs/lean-specs.md](docs/lean-specs.md) for the evidence levels.

## Related documents

- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md): references and language conversions.
- [IR.md](IR.md): the executable code representation.
- [RECIPES.md](RECIPES.md): composing current transformations.
- [PLAN.md](PLAN.md): project and framework migration milestones.
