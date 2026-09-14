# Author selected code

`fr author` changes bodies, semantic deltas, scalar plans, Rust declarations and imports. Batches
accept up to 32 disjoint steps. Coordinate callers after signature changes.

Use `fr author guide` for machine-readable operations, limits and transitions.

For several declarations, use one `project select NAME... --source`; duplicate names
across files return together. Use `project find NAME --in FILE --source` for one declaration;
`root` is its file handle. A module/trait row selects that container; a direct method selects
its impl/trait.

Fragments fit 64 KiB. Steps need `op` and `handle`; fragment steps add `from`.
`edit-body-scalar` adds `scalar` with `operation`, `from` and `to`.
`edit-body-disclosed` adds `disclosed: {edit,to}` and uses its paired full handle.
`edit-body-disclosed-ir` adds `disclosed_ir: {edit,value?}`; typed replacement/insertion values are
inline and deletions omit `value`. Both capability routes use their paired full handle. Short IDs
need top-level `revision`; `organize-imports` takes a file handle. Steps use original source; overlaps refuse.
Optional postconditions cover changed files, edits, operations and paths. Fragment
paths can be project-relative or absolute.

Review diff and retain `plan_context_basis`. Repeat it with
`--save-plan --plan-basis BASIS`; drift or clipping refuses before persistence. The result's
`transaction_context_basis` compacts forward application. `plan_context_basis` starts with
`frpb1`; `frcb1` is a project basis and cannot save a plan.

Rust insertion preserves bytes and docs; only traits allow bodyless functions. Body replacement
covers Rust, Go, Java, Python and supported TypeScript/TSX bindings; arrows may change form.
Python fragments are relative suites and cover decorated synchronous or asynchronous handlers.
For source-free body replacement and smaller checked changes, use [Semantic](semantic.md).

```rust
/// Calls increment twice.
pub fn increment_twice(value: u32) -> u32 { increment(increment(value)) }
```

Use the lookup's file-scoped `root` as `<FILE_HANDLE>`:
```sh
fr project find increment --in src/lib.rs --source --bytes 512
fr author batch --from '<MANIFEST>'
fr author batch --from '<MANIFEST>' --save-plan --plan-basis '<PLAN_CONTEXT_BASIS>'
fr history apply '<AUTHOR_TX>' --write --no-diff --context-basis '<TRANSACTION_CONTEXT_BASIS>'
```
Continue source with `next_offset`. Run [checks](checks.md); retain the transaction for [history](history.md) and [patch export](git.md).
