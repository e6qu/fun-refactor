# Application IR and HTTP adapters

`fr project application [TARGET] [--revision REV] [--feature ID]` returns the
versioned `fr-application-ir-1` hierarchy. Existing feature facts become nodes with
`id`, `kind`, `source`, `data` and `children`. Application, package, dependency,
route, handler, contract, schema, middleware, configuration, service and component
facts retain their identities and evidence. Unparented gap facts remain roots.

The builder drains bounded feature pages before assembling the hierarchy. Duplicate
identities, absent parents, cycles and excess size refuse the query. The model
admits at most 4096 nodes, depth 64 and 4 MiB of encoded JSON. Reader omissions
remain explicit in `omissions`; draining output pages cannot recover evidence
omitted by a source reader. Narrow the scope when those omissions matter.

Route nodes carry a portable `route` when their handler is exactly the admitted
literal JSON, path-binding and selected FastAPI input subset. The normalizer reads the shared semantic IR,
then recognizes explicit response wrappers for Next.js, FastAPI, Express and Go
standard HTTP. Equivalent admitted handlers produce the same response expression.
FastAPI additionally reads required `str`, `int` and `bool` parameters declared with `Query(...)`
or `Body(..., embed=True)`. An omitted alias or one literal `alias` is admitted. Defaults,
optionality, constraints, dynamic aliases, whole-body scalars and
other metadata make the route `manual`. FastAPI `Depends(provider)` and `Security(provider)`
parameters normalize into ordered route dependencies when the provider is one direct simple
callable without keyword arguments. Configured or computed providers stay `manual`. A response
referencing a dependency binding stays `manual` because provider values are opaque to the IR.
A FastAPI response of exactly `requests.METHOD("/literal-path").json()` (or `httpx`) normalizes
into a portable service call. Absolute URLs, computed paths and non-JSON access stay `manual`.
A call whose method and path resolve to anything but one sibling route is demoted to `manual`
during assembly. Other source request bodies, queries, errors and effects also remain
manual; their source evidence stays intact.

FastAPI applications also normalize an ordered middleware chain when every
`app.add_middleware(Name)` or `@app.middleware("http")` registration is a direct unconfigured
name. The portable chain runs outermost-first in reverse registration order. Configured,
computed or unresolved registrations keep the chain out of the IR with an explicit
`middleware_normalization` manual status; the per-entry evidence facts remain. Next.js
convention files stay evidence-only because their export shape is unchecked.

The
separately authored HTTP IR described below generates the same checked required-input subset. Component
nodes retain a rendering boundary. The richer checked Next.js/FastAPI feature
migration remains available for request and response schema cases outside this subset.

## Bounded access and object storage

Obtain the full directory or file handle from `project map`, then run:

```sh
fr project disclose HANDLE --view application --token-limit 4096
```

The frontier contains only the application model hole. `application_shortcuts`
provides exact continuations for `applications` and `omissions`. Continue through
the returned children to the needed route, schema or dependency. Each continuation
binds the project revision, target, view, profile and response ceiling. Its object
root equals the full application report's `object_digest`.

The Python `FrClient.context(..., view="application")` uses the same checked
session and Merkle object-store interface as the other disclosure views. Store
selected materialized subtrees by content digest. `ApplicationIr.from_data(...)`
provides matching typed nodes and an independent object digest. Whole applications can exceed
the per-materialization call limit; request the branch needed for the task.
Source handles remain available for separate maps and evidence disclosures.
Use those handles for code maps, call traces, impact and sources/sinks analysis.
`--proofs` adds verification paths for protocol tests or independent verification.

## Author HTTP behavior through the IR

`fr-http-application-1` is a separate executable subset for agent-authored HTTP
behavior. It contains `schema`, `routes` and an optional ordered `middleware` chain. Each
middleware entry supplies a dotted `name` and a unique 1-based `request_order`; the chain runs
outermost-first and admits at most 64 entries. Generated Express, Go and Next.js modules reference
middleware by name from a host-supplied `fr-middleware` module with the documented signatures.
The FastAPI target refuses chains because router-level middleware has no FastAPI equivalent.
Each route supplies `method`, `path`, optional `inputs`, optional `dependencies`,
`status` and `response`. Dependencies are ordered named providers invoked before the handler.
A failing provider refuses the request with status 401 and
`{"error":"dependency","provider":...}` on portable targets. FastAPI targets keep native
`Depends`/`Security` injection and provider failure semantics. Expressions use six variants:

| Kind | Fields | Meaning |
|---|---|---|
| `literal` | `value` | JSON null, Boolean, bounded string or safe signed integer |
| `path` | `name` | String value of a declared path parameter |
| `input` | `name` | Validated value of a declared request input |
| `service` | `method`, `path` | JSON body of a same-origin call to exactly one sibling route |
| `object` | `fields` | Named child expressions |
| `array` | `items` | Ordered child expressions |

Service calls use a portable method and a bounded literal root-relative path without parameters.
The target must be exactly one other route in the same bundle. Ambiguous, unresolved, nonlocal
and self-recursive targets refuse. An upstream non-2xx answer or transport failure returns 502
with `{"error":"upstream","path":...}`. Service results are runtime values; static IR evaluation
refuses them.

Every input has a simple `name`, a `source` of `query` or `json-body`, and a `scalar` of
`string`, `integer` or `boolean`. Inputs are required, names are unique and distinct from path
parameters, and JSON-body inputs are admitted only on `POST`, `PUT` and `PATCH`. Query keys must
occur exactly once. Validation failures return status 422 and
`{"error":"validation","issues":[...]}` in declaration order. Each issue identifies the source,
name and expected scalar. Undeclared query keys and body fields do not affect the route.

The Python classes mirror those fields:

```python
from pathlib import Path as FilePath
from fr_ir.application import HttpInput, HttpRoute, Input, Literal, Object, Path, RouteBundle

application = RouteBundle([
    HttpRoute("GET", "/records/{id}", 200,
              Object({"id": Path("id"), "available": Literal(True)})),
    HttpRoute("POST", "/audit", 201,
              Object({"accepted": Literal(True), "title": Input("title")}),
              (HttpInput("title", "json-body", "string"),)),
])
application.write(FilePath("application.json"))
```

`write` creates the authored IR file exclusively and refuses to overwrite it.
The package initializer remains empty. No generated entrypoint module is required.

```sh
fr migrate application --ir application.json --to fastapi --out generated
fr migrate application --ir application.json --to fastapi --out generated --save-plan
fr history apply TRANSACTION --write
fr history patch TRANSACTION
fr history undo TRANSACTION --write
fr history redo TRANSACTION --write
```

An existing project can use the same writer without an agent rebuilding the IR:

```sh
fr --json project application > application-report.json
fr migrate application --ir application-report.json --to express --out generated
```

For the smallest agent flow, keep construction and migration in one immutable
project snapshot:

```sh
fr migrate application --project . --to go-net-http --out generated
```

`--project` also accepts a revision-bound project handle and supports `--feature`
for one feature branch. It is mutually exclusive with `--ir`.

The migration accepts a route bundle, a bare `fr-application-ir-1` model, or the
complete `fr-application-report-1` response. For a report it recomputes and checks
the model's object digest. Only normalized routes are generated; the report counts
manual route boundaries, and source files remain preserved.

The navigator advertises `application-migration` in `intent_action.operation_kinds`
when this common planner is selected. Agents can author the matching
`fr-intent-action-2` operation with `to`, `out`, optional registration/dependency
fields, named checks and delivery. Preview and execution rebuild the application IR
from the intent's exact revision-bound target; the action does not carry source text.

`project application` publishes a complete 5×5×4 source/target/feature matrix for
the five adapters and four feature kinds. It reports source readers and target writers separately.
All four HTTP adapters write `validated-json-route`. FastAPI reads the exact declaration subset
above. The other framework readers retain an explicit omission. Every
refused cell names an identical-adapter, missing-reader or missing-writer reason and retains
`runtime_proved: false`.

Preview first; retain `plan_basis` for an unchanged saved-plan or write request.
The input must be a JSON file captured in the analyzed project snapshot.
Existing destinations and paths crossing symlinks refuse. Every output belongs
to one strict-reparse transaction. `--check NAME` binds declared project checks.
FastAPI can connect an existing application and PEP 621 manifest in the same
transaction with `--register-with PATH::APP_SYMBOL`, `--dependency-manifest
pyproject.toml` and an exact `--dependency-requirement`. Express accepts the same
selector for a recognized TypeScript app/router and an exact `express@SPEC` for an
owning `package.json`. Go accepts `PATH::MUX_SYMBOL` for a package-level
`http.NewServeMux()` binding beneath a captured `go.mod`; it generates a small mount
file in the owning package and imports the generated handler by module path.
Recognized Next.js `app` placement beneath a captured package with a `next`
dependency is connected by placement. These edits share preview, review basis,
apply, patch, undo and redo with generated files.

`--cutover` is available only with `--project`. It removes exactly one source file
when the selected model contains one portable feature, the source has a recognized
whole-file ownership shape, target integration is connected, and the project index
has no resolved external reference to the source. This currently admits Next.js App
Router route modules and single default-export static React/Next components. Mixed
FastAPI, Express and Go application files refuse. The deletion shares the generated
files' preview, basis, checks, history, patch, undo and redo transaction.

| Adapter | Output beneath `--out` | Explicit integration |
|---|---|---|
| Next.js | Route files following the IR path hierarchy | Choose the owning App Router directory and package |
| FastAPI | `routes.py` exporting `router` | Include the router and declare FastAPI dependencies |
| Express | `routes.ts` exporting a default Router | Mount the router and declare Express dependencies |
| Go standard HTTP | `routes.go`, package `frgenerated`, exporting `Handler()` | Generate a checked owning-package mount for an explicit ServeMux |
| React | Refused for HTTP routes | React has no HTTP route writer |

## Static frontend components

React and Next.js function components also share a deliberately small executable
subset. A portable component contains one intrinsic JSX tree with lowercase HTML
tags, literal string attributes and explicit text children. A component may declare
up to 16 `useState` bindings with bounded JSON-primitive initial values and render state by name.
`on*` event attributes admit exactly `() => setState(literal)` or `() => setState(!state)`;
a toggle requires a Boolean state. State or events require an explicit client boundary:
a `"use client"` directive, or a standalone React application where client rendering is the only
mode. The model refuses props, effects, other hooks, computed event
handlers, style expressions, component calls, fragments,
spreads and arbitrary JavaScript expressions. Those facts remain in the hierarchy
with a manual normalization boundary.

One selected portable component writes `App.tsx` for React or `page.tsx` for Next.js. The writer
adds the `"use client"` directive only for client Next.js components and imports `useState` only
when state exists. Multiple components refuse until the agent selects one feature branch, avoiding an
invented page or component graph. The Python SDK mirrors `StaticComponent`,
`StaticElement`, `StaticText`, `StaticState`, `ComponentState` and the `ComponentEvent` variants.
Nodes admit depth 32, 1024 nodes and 1 MiB; unsafe
event and raw-HTML attributes refuse. React-to-Next.js and Next.js-to-React fixtures
produce the same static tree before generation, and a pinned React render fixture executes the
stateful subset through both writers.
Entity-bearing source text and attributes also stay manual until the reader can
decode and re-encode their exact JSX semantics without double escaping.

The generated integration status is `connected` only for a checked explicit FastAPI,
Express or Go mount, or recognized Next.js placement. Other targets report `manual`. Existing
source remains preserved by default; only the explicit guarded cutover above removes it.
Generation is framework-independent JSON construction; there are no domain or
fixture-name rules.

## Admission and evidence

Bundles require 1..256 endpoints and at most 1 MiB of encoded JSON. A route admits at most 32
request inputs. Query strings and string body fields admit at most 4096 UTF-8 bytes. Query integers
use canonical decimal spelling; all integers stay in the exactly representable range shared by the
adapters. JSON integers are mathematical integral numbers in that range. Boolean query values are
exactly `true` or `false`; JSON values retain their JSON scalar type. Duplicate query keys refuse.
Expressions
admit at most 1024 nodes and depth 32. Integer literals must lie within
`[-9007199254740991, 9007199254740991]`; floating-point and aggregate literals refuse.
Use explicit object and array expressions for aggregates. Literal strings admit
65536 UTF-8 bytes; field names admit 256.

Paths are bounded absolute ASCII paths with unreserved literal segments and unique
`{parameter}` names. Percent encoding, catch-all patterns, repeated slashes,
trailing slashes, dot segments and ambiguous matchers refuse. Same-path distinct
methods are allowed. Overlapping different paths refuse even across methods.
Methods are GET, POST, PUT, PATCH, DELETE and OPTIONS. HEAD refuses because the
IR models a JSON body. Statuses admit 200..599 except 204, 205 and 304.

Writers preserve reserved object keys and avoid parameter/import name collisions.
Pinned runtime tests execute valid and invalid requests and compare generated JSON values and
statuses with independent IR evaluation in FastAPI, Express, Go HTTP and Next.js. One runtime
fixture executes an ordered middleware chain and the 401 dependency contract through all four
adapters. Another forwards a sibling route's JSON through Express, Go HTTP and Next.js real
servers. Framework
installation, registration, URL decoding, implicit methods, configured middleware, provider
internals, external services, effects, nested
request schemas and deployment behavior remain outside this subset.

Reading FastAPI declarations preserves the admitted required inputs and successful response.
Generated targets use the IR's portable validation contract. Duplicate query keys and
noncanonical integer or Boolean query spellings refuse. Failures use the deterministic issue body.
Native FastAPI has broader coercions and its own detailed 422 payload. The runtime fixture checks
the shared accepted case and representative rejection statuses. The report keeps
`runtime_proved: false` because `fr` intentionally canonicalizes framework-specific failure details.

Lean proves separate reader and writer admission, compatibility, request-input admission, the
FastAPI declaration policy, the middleware chain and dependency admission policies, JSON status
safety, exact unique disposition coverage, validated
endpoint agreement and static-tree resource policies. Shared
finite Rust, Python and Lean cases check the executable policies. These are model and policy results.
Parser extraction and generated code behavior remain separate integration tests.
An integration fixture checks that equivalent Next.js, FastAPI, Express and Go
handlers normalize to equal response IR before writing. These checks do not prove
parser correctness.
`runtime_proved` is false.
