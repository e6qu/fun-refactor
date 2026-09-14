# Framework semantic model

`fr project features` reports a bounded source model for Next.js, standalone React, Express.js and FastAPI.
It treats every application and feature as a candidate and records what source evidence cannot establish.

## Versioned syntax witnesses

| Reader | Executable fixture | Pinned real source |
| --- | --- | --- |
| Next.js App Router | Next.js 16 package declarations with `route.ts`, `route.js`, `page.tsx`, `layout.tsx`, Proxy and instrumentation forms | `shadcn-ui/taxonomy` at `298a8857c7128a0d121e7f699dfd729f23b3966d`, whose manifest declares Next.js `13.3.2-canary.13` |
| React components | React 19 package declarations with direct function components, `use client`, relative imports and common hooks | The taxonomy pin declares React `^18.2.0` and supplies the App Router handlers |
| Express.js | Express 5 package declarations with direct route registrations and named handlers | Route patterns are covered by the generic framework corpus and the executable package fixture |
| FastAPI | Direct `FastAPI` and `APIRouter` assignments, verb decorators, dependencies, middleware and lifecycle forms | `fastapi/full-stack-fastapi-template` at `750d3d0bc6dfece4dec2d6ef8c3ff7e64f72545d`, whose project requires FastAPI `>=0.141.1,<1.0.0` |

The checked-in sources and hashes live in the [translation corpus provenance](../tests/corpus/PROVENANCE.md) and [framework corpus provenance](../tests/framework-corpus/PROVENANCE.md).
Fixture tests exercise the newer declared subsets.
Real-source tests compare route output with hard-coded contracts that do not call the production readers.

## Modeled boundaries

The Next.js reader derives App Router paths from captured file placement.
It recognizes direct HTTP exports, page and inherited layout components, and bounded relative component imports.
It also reports Proxy, legacy middleware, instrumentation, environment access and HTTP service candidates.

The React reader recognizes direct function components and their props, hooks, events, style shapes and render edges.
Outside Next.js, a captured React dependency and valid JSX or TSX component declaration establish a
standalone candidate. Root files in the relative-import graph form features and imported component
files expand beneath them. Standalone components use the ordinary React client default. Inside
Next.js, static import reachability can propagate a client candidate from a `use client` entry.
Ambiguous declarations, missing imports and package crossings produce gap facts.

The Express reader groups existing source-level route declarations beneath the nearest captured npm
package. It preserves exact route and handler evidence. Mounted-router prefixes, middleware order,
factories and runtime registration remain explicit application gaps.

The FastAPI reader requires an observed framework import and one direct constructor assignment.
It recognizes top-level verb decorators and joins a valid literal constructor prefix with each literal decorator path.
An empty prefix is valid.
A nonempty prefix must start with `/` and must not end with `/`, matching the [FastAPI router contract](https://fastapi.tiangolo.com/reference/apirouter/).
Dynamic or invalid prefixes suppress affected route facts and produce explicit gaps.

FastAPI dependencies retain application, route or parameter scope.
Middleware facts retain declaration order and the reverse request order.
Lifecycle, configuration and sanitized outbound HTTP facts remain attached to their source anchors.
Root-relative HTTP facts also carry bounded route candidates from every inferred application in the
selected project scope. Exact paths are required. A known method must also match; an unknown method
keeps every path match. Unique and ambiguous candidates remain labeled as static evidence.

## Evidence boundary

The readers validate captured syntax and project revision identity.
They do not import a framework, start a server or prove runtime reachability.
Application facts state those gaps.
FastAPI include-router and mounted prefixes can change a final runtime path, so the current application fact names them as unresolved.
Next.js rewrites, `basePath`, package resolution, bundling and runtime configuration also remain unresolved.

The framework policy helpers have anchored Lean models.
Their theorems cover output caps, middleware ranks, hook compatibility, standalone React admission,
configuration visibility, service classification, redaction flags and FastAPI prefix validity.
The service-route predicate proves that every emitted link has a local target and equal path.
It also proves the known-method equality rule.
The shared executable comparison includes all Boolean combinations for standalone React admission.
Parser recognition, route concatenation, import-edge construction and report assembly retain fixture evidence only.

Future code that claims runtime behavior needs an executable framework fixture for that claim.
The current model makes source-level candidate claims and exposes the runtime boundary instead.
