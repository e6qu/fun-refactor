# Explore only the needed structure

For report pages, use [Disclosure](disclosure.md).

For a one-file lookup, copy its path as `<KNOWN_FILE>` and pass it as `-C`.
`--in` filters project results; it does not narrow the initial scan.

A signature lists arguments. Copy each returned code handle unchanged.

```sh
fr -C '<KNOWN_FILE>' project find greet --signature --limit 1
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

Names match bare declarations, such as `read`. Scope methods with `--in FILE_OR_HANDLE`.
Empty results for qualified names do not establish absence.

Use `project explore TERM [--contains]`: at most twelve names, no source.
Run `next.arguments` for source and relationships. Pages shrink to fit the budget;
profile sizes are maxima. Follow cursors and offsets.
Source continuations use `--view source`; relationships use `--view relationships`.
Each returns its view, identity and coverage. An omitted view does not establish absence.
Use `--profile expanded` only through its reported action; it keeps the current view and page position.

Batch known reads with `project batch --profile compact`, referencing prior handles through
`/rows/0/handle`. Requests share one snapshot and report budget.

Use `project find NAME` for known declarations and `project select SELECTOR...` for several exact
names or handles under one revision and budget. `--contains` is a literal-substring flag. Read every
status before claiming absence. Handles may be `outside-scope`, `not-a-declaration` or omitted;
stale handles refuse instead of becoming names.
Choose `<HANDLE>` from a declaration row; full handles include the revision.
Short IDs require the returned `--revision`. Targeted rows and `show` expose `location.name` and
`location.definition`: exact source byte ranges and 1-based, half-open line/column ranges. Add `location`
to broad map fields when needed. A syntax header is not a semantic contract.

Use `--source` when needed. Offsets count bytes from the node's start.
Pass `next_offset` as `--offset` for another slice that preserves UTF-8 boundaries.
For another result page, reuse the same query and fields with `--cursor` and the returned `page.next`.
Source or scope changes can invalidate handles and cursors; repeat stale queries.

Unresolved call targets and dispatch candidates do not establish runtime dispatch.
Test associations are candidates for selecting checks, not proof of complete coverage or commands to execute blindly.
Use `project packages` and `dependencies` for Cargo, npm, Go module and Python project declarations.
Use `project resolutions` for captured lock entries. Filter with `--manifest` or
`--lockfile`; the result reports observed versions and integrity metadata without running a solver.
Dependencies use the nearest lock, with bounded candidates and package aliases.
Use `project package-features --manifest Cargo.toml` for Cargo feature and dependency activation.
Use `project verify-artifact` to compare local bytes with captured checksums. Go directories need `--go-prefix`.
`links` and `workspaces` add Cargo/npm local relationships.
Use `project routes`, `contracts` and `configuration` for supported declaration patterns.
[Surfaces](surfaces.md) covers frameworks and cross-stack edits.
Lock evidence does not authenticate its repository or model framework behavior.

[Plans](investigations.md).
