# Explore only the needed structure

```sh
fr project explore greet --in app.py
fr project explore greet --mode behavior --target '<HANDLE>'
fr project select greet render validate --signature --source --bytes 2048
fr project show '<HANDLE>'
fr project show '<HANDLE>' --relations --limit 8
fr project show '<HANDLE>' --source --bytes 256
fr project calls '<HANDLE>' --direction incoming --limit 8
fr project tests app.py --limit 8
fr project features --limit 12
fr project technologies --limit 13
fr project styles web --limit 12
fr project diagrams docs --limit 12
fr project disclose '<PROJECT_HANDLE>' --view project --token-limit 4096
fr project gaps --limit 8
```

When the task only needs several read views, use the bounded manifest in [Batch](batch.md).
When those views select structural edit targets and declared checks, use [Task](task.md).
Keep the cache enabled. Cold calls may emit `indexing` progress on stderr; stdout remains the report.

For behavior discovery, start with one `project explore TERM [--contains]`. It returns no source in
names mode and caps the page at twelve rows. Execute the selected row's `next.arguments` exactly to
obtain its bounded source and direct relationships. Execute truncation continuations instead of
raising limits. Use `--profile expanded` only through the reported explicit expansion action.

When two stages are known, put `explore` requests in one `project batch --profile compact`
manifest and reference the selected handle through `/rows/0/handle`. This reuses one project
snapshot and enforces an aggregate report budget. Independent cold processes coalesce identical
resolution work.

Use exact `project find NAME` for known declarations. `--contains` is a boolean literal-substring flag.
Use `project select SELECTOR...` for several exact names or handles under one revision and budget.
Read every selector status before claiming absence. Handles can report `outside-scope`,
`not-a-declaration` or an omitted local; stale handles refuse instead of becoming names.
Use maps when hierarchy matters. Choose `<HANDLE>` from a declaration row. Full handles include their source revision.
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
Use `project features [--feature ID]` for a bounded Next.js, standalone React, Express.js or FastAPI hierarchy. Treat middleware,
providers, render targets and test associations as syntax candidates that need source inspection.
Use `project technologies` to distinguish host languages and frameworks before choosing a reader.
Use `project styles` for CSS definitions and direct HTML/JSX class relationships, and `project diagrams`
for Markdown heading and embedded Mermaid graph hierarchy. Follow their source handles only when the
high-level fact or explicit gap is insufficient.
When a returned row has `edit.schema = fr-surface-edit-1`, use its exact preview arguments and
replace only `<VALUE>`. Review the diff before repeating with `--write`; retain the transaction for
undo, redo and patch export. Rows without an edit capability are read-only boundaries.
When several cross-stack domains may matter, obtain a full map handle and start with
`project disclose HANDLE --view project`. Read the four-entry catalog, reveal the narrow domain, and
reuse its object digest with the Python Merkle object-store helpers when persistence is useful.
Service targets redact credentials, queries and fragments. Reports retain explicit runtime and resolution gaps.
These commands do not establish complete dependency resolution or framework semantics.
