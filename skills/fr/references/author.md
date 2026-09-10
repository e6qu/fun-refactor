# Author selected code

`fr author` accepts revision-bound handles for supported body replacement, same-named Rust function replacement, Rust function insertion into a file/module/impl/trait, and batches of up to 32 disjoint operations. Signature changes need coordinated caller edits; unsupported targets refuse.

Use `project find NAME --in FILE --source`; its `root` is the file handle. A module/trait row selects that container, while any direct method selects its impl/trait. If needed, get a file handle with `project map FILE --depth 0 --fields handle,kind,name --limit 1`. Source changes expire handles.

External UTF-8 fragments are limited to 64 KiB. Batch operations need `op` and `handle`; fragment operations add `from`, short IDs need top-level `revision`, and `organize-imports` takes a file handle. All steps use original source; overlaps refuse. Use `postconditions` for exact `files-changed`, `edits`, `changed-operations`, or `paths-changed` expectations.

Review a complete diff and retain `plan_context_basis`. Repeat the same plan with `--save-plan --plan-basis BASIS`; drift or clipping refuses before persistence. The saved result supplies `transaction_context_basis` for compact forward application.

Rust insertion preserves fragment bytes, accepts outer doc comments, and permits bodyless functions only in traits. Body replacement covers Rust, Go functions/methods, Java methods/constructors/default methods, and supported TypeScript/TSX bindings. TypeScript arrows may change body form.

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
