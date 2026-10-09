# Author selected code

`fr author` edits bodies, semantic deltas, scalar plans, Rust declarations and imports.
Batches accept 32 disjoint operations. Coordinate callers after signature changes.

Use `fr author guide` for machine-readable operations, limits and transitions.

Use `project select NAME... --source` for several declarations, including duplicate names.
Use `project find NAME --in FILE --source` for one; `root` is its file handle.
Module/trait rows select containers; method rows select their impl/trait.

Fragments fit 64 KiB. Operations need `op` and `handle`; fragments add `from`.
`edit-body-scalar` adds `scalar` with `operation`, `from` and `to`.
`edit-body-disclosed` adds `disclosed: {edit,to}`.
`edit-body-disclosed-ir` adds `disclosed_ir: {edit,value?}`; replacement/insertion values are inline;
deletions omit `value`. Both use their paired full handle. Short IDs need top-level `revision`;
`organize-imports` takes a file handle. Operations use original source; overlaps refuse.
Postconditions can constrain changed files, edits, operations and paths.
Fragment paths are project-relative or absolute.

Review the diff and retain `plan_context_basis`. Repeat it with
`--save-plan --plan-basis BASIS`; drift or clipping refuses before persistence. The result's
`transaction_context_basis` compacts forward application. Plan bases start with `frpb1`;
project bases (`frcb1`) cannot save plans.

Rust insertion preserves bytes and docs; only traits allow bodyless functions. Replace bodies in
Rust, Go, Java, Python and supported JavaScript/TypeScript/TSX bindings; arrows may change form.
Python fragments are relative suites, including decorated sync/async handlers.
For source-free replacements and smaller checked changes, see [Semantic](semantic.md).

```rust
/// Calls increment twice.
pub fn increment_twice(value: u32) -> u32 { increment(increment(value)) }
```

Save `<MANIFEST>` with the declaration handle and new body file:
```json
{"operations":[{"op":"replace-body","handle":"<DECLARATION_HANDLE>","from":"<BODY_FILE>"}]}
```

```sh
fr project find increment --in src/lib.rs --source --bytes 512
fr author batch --from '<MANIFEST>'
fr author batch --from '<MANIFEST>' --save-plan --plan-basis '<PLAN_CONTEXT_BASIS>'
fr history apply '<AUTHOR_TX>' --write --no-diff --context-basis '<TRANSACTION_CONTEXT_BASIS>'
```
Continue source with `next_offset`. Run [checks](checks.md); retain the transaction for [history](history.md) and [patch export](git.md).
