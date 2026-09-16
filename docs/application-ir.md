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

Route and component nodes currently retain a `boundary` instead of an executable
program. Captured syntax evidence does not establish response or rendering behavior.
The application projection does not automatically migrate an existing project.
The earlier checked Next.js/FastAPI feature migration remains available.

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
behavior. It contains `schema` and `routes`. Each route supplies `method`, `path`,
`status` and `response`. Expressions use exactly four variants:

| Kind | Fields | Meaning |
|---|---|---|
| `literal` | `value` | JSON null, Boolean, bounded string or safe signed integer |
| `path` | `name` | String value of a declared path parameter |
| `object` | `fields` | Named child expressions |
| `array` | `items` | Ordered child expressions |

The Python classes mirror those fields:

```python
from pathlib import Path as FilePath
from fr_ir.application import HttpRoute, Literal, Object, Path, RouteBundle

application = RouteBundle([
    HttpRoute("GET", "/records/{id}", 200,
              Object({"id": Path("id"), "available": Literal(True)})),
    HttpRoute("POST", "/audit", 201, Literal("accepted")),
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

Preview first; retain `plan_basis` for an unchanged saved-plan or write request.
The input must be a JSON file captured in the analyzed project snapshot.
Existing destinations and paths crossing symlinks refuse. Every output belongs
to one strict-reparse transaction. `--check NAME` binds declared project checks.
Changes outside generated files are absent from this authoring operation.

| Adapter | Output beneath `--out` | Explicit integration |
|---|---|---|
| Next.js | Route files following the IR path hierarchy | Choose the owning App Router directory and package |
| FastAPI | `routes.py` exporting `router` | Include the router and declare FastAPI dependencies |
| Express | `routes.ts` exporting a default Router | Mount the router and declare Express dependencies |
| Go standard HTTP | `routes.go`, package `frgenerated`, exporting `Handler()` | Use its handler in the owning Go application |
| React | Refused for HTTP routes | React has no HTTP route writer |

The generated integration status is `manual`. This operation does not register
an existing application, change dependency manifests or perform source cutover.
Generation is framework-independent JSON construction; there are no domain or
fixture-name rules.

## Admission and evidence

Bundles require 1..256 endpoints and at most 1 MiB of encoded JSON. Expressions
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
Pinned runtime tests compare generated JSON values and statuses with independent
IR evaluation in FastAPI, Express, Go HTTP and Next.js. Framework installation,
registration, URL decoding, implicit methods, errors, middleware, authentication,
request schemas and deployment behavior remain outside this subset.

Lean proves adapter admission, compatibility, JSON status safety, exact unique
disposition coverage and endpoint agreement policies. Shared finite Rust, Python
and Lean cases check the executable policies. These are model and policy results.
Parser extraction and generated code behavior remain separate integration tests;
`runtime_proved` is false.
