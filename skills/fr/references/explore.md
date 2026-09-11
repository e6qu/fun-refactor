# Explore only the needed structure

The examples use a project containing `app.py`. Substitute the relevant path in the real task.
Keep the repository as the scan root when callers elsewhere matter; a single-file `-C` restricts discovery to that file.

```sh
fr project find greet --in app.py --signature --limit 12
fr project select greet render validate --signature --source --bytes 2048
fr project show '<HANDLE>'
fr project show '<HANDLE>' --relations --limit 8
fr project show '<HANDLE>' --source --bytes 256
fr project calls '<HANDLE>' --direction incoming --limit 8
fr project tests app.py --limit 8
fr project features --limit 12
fr project gaps --limit 8
```

Use exact `project find NAME` for known declarations; add `--contains` for a literal substring.
Find matches names before clipping and reports all candidates with pagination and source coverage.
Use `project select SELECTOR...` for several exact names or full handles so one revision,
coverage report, cursor and source budget cover the complete request. Read every per-selector
status before claiming absence. Handles select one exact node and can report `outside-scope`,
`not-a-declaration` or an omitted local; stale handles refuse instead of becoming names.
Use maps when the hierarchy itself matters. Choose `<HANDLE>` from the relevant declaration row. Full handles include their source revision.
Alternatively use a short ID with the returned `--revision`; never reuse a bare ID across revisions.
`show` gives the declaration's `position`, a 1-based line and column suitable for a refactoring target.
Its syntax header can contain defaults and attributes; it is not a complete semantic contract.

Request `--source` only when needed. Source offsets count bytes from the selected node's start, not lines or file-relative offsets.
For another slice, pass the returned `next_offset` with `--offset`; this preserves UTF-8 boundaries.
For another result page, reuse the same query and fields with `--cursor` and the returned `page.next`.
Changing source, manifests, scan options or query scope can invalidate a handle or cursor. Restart the relevant map or query after a stale response.

Maps show lexical containment, not inferred architecture.
Call results preserve confidence and unresolved or dispatch-candidate rows; candidates do not establish runtime dispatch.
Test associations are candidates for selecting checks, not proof of complete coverage or commands to execute blindly.
Use `project packages`, `dependencies`, `links` and `workspaces` for manifest declarations and local relationships when package boundaries matter.
Use `project routes`, `contracts` and `configuration` for their supported declaration patterns when the task needs those views.
Use `project features` for the parent-linked Next.js App Router or FastAPI application, package, route, handler, contract, execution-dependency, middleware and schema hierarchy.
Pass a returned `--feature ID` to retrieve one revision-bound subtree.
Middleware order and direct FastAPI providers are syntax candidates; inspect their source before making runtime or authentication claims.
Lifecycle, configuration and service facts also retain syntax evidence and explicit runtime gaps.
Service targets omit query strings, fragments and URL credentials; use a source slice only when the task needs request details.
Next.js page features include inherited layouts and bounded direct relative component imports.
Their React facts cover props, hooks, events, styles and render edges.
Unique same-file, default-import and named-import render targets include a source anchor; inspect unresolved targets directly.
These commands do not establish complete dependency resolution or framework semantics.
