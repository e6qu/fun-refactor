# Explore only the needed structure

The examples use a project containing `app.py`. Substitute the relevant path in the real task.
Keep the repository as the scan root when callers elsewhere matter; a single-file `-C` restricts discovery to that file.

```sh
fr project map app.py --fields handle,parent,kind,name,line --limit 12
fr project show '<HANDLE>'
fr project show '<HANDLE>' --relations --limit 8
fr project show '<HANDLE>' --source --bytes 256
fr project calls '<HANDLE>' --direction incoming --limit 8
fr project tests app.py --limit 8
fr project gaps --limit 8
```

Choose `<HANDLE>` from the row for the relevant declaration. Full handles include their source revision.
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
These commands do not establish complete dependency resolution or framework semantics.
