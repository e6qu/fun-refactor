# Author selected code

`fr author` can replace source or semantic bodies, apply typed semantic deltas, replace or insert
Rust declarations, and organize imports. Batches accept up to 32 disjoint steps. Coordinate callers
after signature changes. Unsupported targets refuse.

Use `fr author guide` for machine-readable operations, limits and transitions. After reading
this route, skip subcommand help.

For several exact declarations, use one `project select NAME... --source`; duplicate names
across files return together. Use `project find NAME --in FILE --source` for one declaration;
`root` is its file handle. A module/trait row selects that container; a direct method selects
its impl/trait. Otherwise use
`project map FILE --depth 0 --fields handle,kind,name --limit 1`. Source changes expire handles.

UTF-8 fragments are limited to 64 KiB. Steps need `op` and `handle`; fragment steps add
`from`, short IDs need top-level `revision`, and `organize-imports` takes a file handle. Steps
use original source; overlaps refuse. Exact `postconditions`: `files-changed`, `edits`,
`changed-operations`, and `paths-changed`.
Paths are project-relative. Copy absolute paths from external artifact writers verbatim into
fragment `from` fields and batch `--from`.

Review the complete diff and retain `plan_context_basis`. Repeat it with
`--save-plan --plan-basis BASIS`; drift or clipping refuses before persistence. The result's
`transaction_context_basis` compacts forward application. `plan_context_basis` starts with
`frpb1`; `frcb1` is a project basis and cannot save a plan.

Rust insertion preserves bytes and doc comments; only traits allow bodyless functions. Body
replacement covers Rust, Go, Java, and supported TypeScript/TSX bindings; arrows may change form.
For source-free body replacement and smaller checked changes, use [Semantic](semantic.md).

```rust
/// Increments a value twice.
pub fn increment_twice(value: u32) -> u32 {
    increment(increment(value))
}
```
Use the lookup's file-scoped `root` as `<FILE_HANDLE>`:
```sh
fr project find increment --in src/lib.rs --source --bytes 512
fr author batch --from '<MANIFEST>'
fr author batch --from '<MANIFEST>' --save-plan --plan-basis '<PLAN_CONTEXT_BASIS>'
fr history apply '<AUTHOR_TX>' --write --no-diff --context-basis '<TRANSACTION_CONTEXT_BASIS>'
```
Continue source with `next_offset`. Run [checks](checks.md); retain the transaction for [history](history.md) and [patch export](git.md).
