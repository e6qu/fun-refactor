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

Combine reads with [Batch](batch.md); select edits and checks with [Task](task.md).
Keep the cache enabled. Cold calls may report `indexing` on stderr.

For behavior discovery, start with `project explore TERM [--contains]`. Names mode returns no source
and at most twelve rows. Run a selected row's `next.arguments` for bounded source and relationships.
Pages shrink to fit the serialized response budget. Follow returned cursors and offsets;
profile sizes are maxima.
Follow truncation continuations. Source continuations use `--view source`; relationship continuations
use `--view relationships`. Each returns only the requested view, plus identity and coverage metadata.
An omitted view is not evidence that no source or relationships exist.
Use `--profile expanded` only through its reported action; it keeps the current view and page position.

Batch known reads with `project batch --profile compact`, referencing prior handles through
`/rows/0/handle`. Requests share one snapshot and report budget.

Use `project find NAME` for known declarations and `project select SELECTOR...` for several exact
names or handles under one revision and budget. `--contains` is a literal-substring flag. Read every
status before claiming absence. Handles may be `outside-scope`, `not-a-declaration` or omitted;
stale handles refuse instead of becoming names.
Use maps for hierarchy. Choose `<HANDLE>` from a declaration row; full handles include the revision.
Short IDs require the returned `--revision`. Targeted rows and `show` expose `location.name` and
`location.definition`: exact byte spans and 1-based, half-open line/column ranges. Add `location`
to broad map fields when needed. Syntax headers do not establish complete semantic contracts.

Request `--source` only when needed. Offsets count bytes from the selected node's start. For another
slice, pass `next_offset` with `--offset`; this preserves UTF-8 boundaries.
For another result page, reuse the same query and fields with `--cursor` and the returned `page.next`.
Source, manifest, scan or scope changes can invalidate handles and cursors. Repeat stale queries.

Call results preserve confidence and unresolved or dispatch-candidate rows; candidates do not establish runtime dispatch.
Test associations are candidates for selecting checks, not proof of complete coverage or commands to execute blindly.
Use `project packages` and `dependencies` for Cargo, npm, Go module and Python project declarations.
Use `project resolutions` for source-free captured lock entries. Filter with `--manifest` or
`--lockfile`; the result reports observed versions and integrity metadata without running a solver.
Dependency rows match the nearest ancestor lock, retaining bounded candidates and package aliases.
Use `project package-features --manifest Cargo.toml` for Cargo feature and dependency activation.
The progressive project's `packages` branch addresses the default selection.
Use `project verify-artifact` to compare local bytes with captured checksums. Go directories need `--go-prefix`.
`links` and `workspaces` add Cargo/npm local relationships.
Use `project routes`, `contracts` and `configuration` for supported declaration patterns.
Use [Surfaces](surfaces.md) for framework, style and embedded-diagram discovery, exact surface edits,
or the cross-stack Merkle view.
Lock evidence does not authenticate its repository or model framework behavior.

[Plans](investigations.md).
