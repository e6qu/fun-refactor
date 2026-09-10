# Verified feature migration

`fr migrate feature` turns one revision-bound framework feature into an inspectable source-history transaction.
The first subset crosses between Next.js App Router route files and FastAPI route files.

Start with a compact feature query, then pass its ID to the migration command:

```sh
fr --json project features
fr --json migrate feature frff1:... --to fastapi --out migrated/pets.py
fr --save-plan --json migrate feature frff1:... --to fastapi --out migrated/pets.py
```

The selected feature must exist in the current project revision, contain at least one route and keep every selected method in one source file.
The output must stay within the workspace.
FastAPI output names a `.py` file, while Next.js output names the destination `app` directory.
The destination cannot replace the source.

The report binds the feature, source revision, source and target frameworks, source paths and generated paths.
It lists the exact method and URL contract and refuses when the framework reader and file translator disagree.
The migration preview also carries a bounded diff and translation fidelity notes.

Each selected semantic fact has one disposition:

- `automatic` covers the bounded application, feature, route, handler and schema shapes that the translator can carry.
- `agent-decision` covers recognized facts whose project-specific use needs review.
- `unsupported` covers explicit semantic gaps and translator notes.

The first automatic step translates the selected route file.
Application registration and final cutover remain agent decisions because composition and deployment conventions vary by project.
The plan does not claim runtime, middleware, authentication, validation, lifecycle or schema equivalence.

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
Lean proves these finite policies, and shared execution compares their complete Boolean domains with Rust.
Parser recognition, endpoint extraction, kind assignment, translation, filesystem history and framework runtime behavior remain outside those proofs.

Current follow-up work adds executable source and destination fixtures, independent schema and behavior comparisons, connected build and test edits, and a reviewed cutover stage.
