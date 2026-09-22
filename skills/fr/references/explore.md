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
fr project styles --limit 12
fr project diagrams --limit 12
fr project gaps --limit 8
```

When the task only needs several read views, use the bounded manifest in [Batch](batch.md).
When those views select structural edit targets and declared checks, use [Task](task.md).
Keep the cache enabled. Cold calls may emit `indexing` progress on stderr; stdout remains the report.

For behavior discovery, start with `project explore TERM [--contains]`. Names mode returns no source
and at most twelve rows. Run a selected row's `next.arguments` for bounded source and relationships.
Follow truncation continuations. Use `--profile expanded` only through its reported action.

Put known multi-stage reads in one `project batch --profile compact` manifest. Reference prior
handles through `/rows/0/handle`; the batch shares one snapshot and report budget.

Use `project find NAME` for known declarations and `project select SELECTOR...` for several exact
names or handles under one revision and budget. `--contains` is a literal-substring flag. Read every
status before claiming absence. Handles may be `outside-scope`, `not-a-declaration` or omitted;
stale handles refuse instead of becoming names.
Use maps when hierarchy matters. Choose `<HANDLE>` from a declaration row. Full handles include their source revision.
Use a short ID only with its returned `--revision`. Targeted rows and `show` expose
`location.name` and `location.definition`, each with an exact byte span and a 1-based, half-open
line/column range. Add `location` to broad map fields when needed. A syntax header can contain
defaults and attributes; it is not a complete semantic contract.

Request `--source` only when needed. Offsets count bytes from the selected node's start. For another
slice, pass `next_offset` with `--offset`; this preserves UTF-8 boundaries.
For another result page, reuse the same query and fields with `--cursor` and the returned `page.next`.
Changing source, manifests, scan options or query scope can invalidate a handle or cursor. Restart the relevant map or query after a stale response.

Call results preserve confidence and unresolved or dispatch-candidate rows; candidates do not establish runtime dispatch.
Test associations are candidates for selecting checks, not proof of complete coverage or commands to execute blindly.
Use `project packages` and `dependencies` for Cargo, npm, Go module and Python project declarations.
Use `project resolutions` for source-free captured lock entries. Filter with `--manifest` or
`--lockfile`; the result reports observed versions and integrity metadata without running a solver.
Dependency rows already join the nearest ancestor lock and preserve all bounded version candidates.
Cargo's `package` field, npm aliases and normalized Python names participate in that match.
Use `project package-features --manifest Cargo.toml` for Cargo feature and dependency activation.
The progressive project's `packages` branch addresses the default selection.
Use `project verify-artifact` to compare local bytes with captured checksums. Go directories need `--go-prefix`.
`links` and `workspaces` add Cargo/npm local relationships.
Use `project routes`, `contracts` and `configuration` for their supported declaration patterns when the task needs those views.
Use [Surfaces](surfaces.md) for framework, style and embedded-diagram discovery, exact surface edits,
or the cross-stack Merkle view.
Lock evidence does not authenticate its repository or model framework behavior.
