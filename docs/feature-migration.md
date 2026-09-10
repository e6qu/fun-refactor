# Verified feature migration

`fr migrate feature` turns one revision-bound framework feature into an inspectable source-history transaction.
The first subset crosses between Next.js App Router route files and FastAPI route files.

Start with a compact feature query, then pass its ID to the migration command:

```sh
fr --json project features
fr --json migrate feature frff1:... --to fastapi --out services/telemetry.py
fr --save-plan --json migrate feature frff1:... --to fastapi --out services/telemetry.py
```

The selected feature must exist in the current project revision, contain at least one route and keep every selected method in one source file.
The output must stay within the workspace.
FastAPI output names a `.py` file, while Next.js output names the destination `app` directory.
The destination cannot replace the source.

The report binds the feature, source revision, source and target frameworks, source paths and generated paths.
It lists the exact method and URL contract and refuses when the framework reader and file translator disagree.
It also lists canonical declared models, field names and supported field types parsed from source and generated code.
Migration refuses when a translated source shape is missing or different after independently parsing the destination.
The migration preview also carries a bounded diff and translation fidelity notes.

Each selected semantic fact has one disposition:

- `automatic` covers the bounded application, feature, route, handler and schema shapes that the translator can carry.
- `agent-decision` covers recognized facts whose project-specific use needs review.
- `unsupported` covers explicit semantic gaps and translator notes.

The first automatic step translates the selected route file.
Application registration and final cutover remain agent decisions because composition and deployment conventions vary by project.
The plan does not claim full framework runtime, middleware, authentication, validation, lifecycle or wire-schema equivalence.

The command keeps the source route and adds the destination.
This supports an incremental period where both frameworks remain in the workspace.
Executable coexistence still depends on project registration and independent behavior checks.

Preview changes no files.
`--save-plan` records the migration, and `--write` records and applies it.
The resulting history ID supports checked apply, patch export, undo and redo:

```sh
fr history patch <ID> --check
fr history apply <ID>
fr history undo
fr history redo
```

The anchored migration-direction policy accepts only a transition between the two supported framework classes.
The anchored disposition policy maps gaps to unsupported work, supported automatic kinds to automatic work and all other recognized facts to agent decisions.
The anchored schema policy accepts a generated shape set exactly when it contains every expected shape.
Lean proves these policies, and shared execution compares their bounded domains with Rust.
Parser recognition, endpoint extraction, kind assignment, translation, filesystem history and framework runtime behavior remain outside those proofs.

## Runtime evidence

The integration suite executes generic source and generated handlers in both directions.
One case crosses from Next.js TypeScript to FastAPI Python, and one crosses from FastAPI Python to Next.js TypeScript.
The cases use telemetry and parameterized metrics routes, then compare status and JSON bodies exactly.
Node and Python execute the handlers, while small stubs supply decorator registration.
This evidence covers handler behavior in the supported constructs.
It does not cover framework middleware, dependency injection, validation, startup, routing registration or deployment.

## Declared schema evidence

The migration report canonicalizes records into names and sorted fields whose supported types use language-neutral spellings.
It reparses the generated Python or TypeScript and checks the resulting declarations against the translated source records.
FastAPI migration includes only models reached through selected handler signatures.
The framework writers retain source field spellings because those names can be JSON keys.
This check covers declarations and supported types.
It does not establish aliases, requiredness, defaults, validators, serialization settings, OpenAPI output or runtime request and response validation.

Current follow-up work adds registered framework fixtures, runtime schema comparisons, connected build and test edits, and a reviewed cutover stage.
