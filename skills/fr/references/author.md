# Author selected code

`fr author` accepts revision-bound handles. It can replace supported bodies, replace a same-named Rust function declaration, insert a Rust function into a file/module/impl/trait, or combine up to 32 disjoint operations in one batch. Signature changes need coordinated caller edits. Unsupported targets refuse.

Use `project find NAME --in FILE --source`; its `root` is that file's handle. A module or trait row selects that container. Any direct method selects its exact impl or trait. If no file handle is available, use `project map FILE --depth 0 --fields handle,kind,name --limit 1`. Source changes expire handles.

Fragments are external UTF-8 files of at most 64 KiB. Batch operations need `op` and `handle`; fragment operations also need `from`. Short IDs require top-level `revision`. `organize-imports` takes a file handle. All steps use the original source, and overlaps refuse. Add exact `postconditions` when the intended counts and paths are known:

```json
{
  "operations": [
    {"op": "replace-body", "handle": "<DECL_HANDLE>", "from": "<BODY_FRAGMENT>"},
    {"op": "insert-declaration", "handle": "<FILE_HANDLE>", "from": "<DECL_FRAGMENT>"}
  ],
  "postconditions": {"files-changed": 2, "edits": 2, "changed-operations": 2}
}
```

Review the complete diff and retain `plan_context_basis`. Repeat the same plan with `--save-plan --plan-basis BASIS`; drift or a clipped preview refuses before persistence. The saved result supplies `transaction_context_basis` for the compact forward apply.

Rust insertion preserves fragment bytes, accepts outer doc comments, and limits bodyless functions to traits. Body replacement supports Rust, Go functions/methods, Java methods/constructors/default methods, and supported TypeScript/TSX function bindings. TypeScript arrows may switch between expression and block bodies.

Example insertion fragment:

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

Continue a source slice with its `next_offset`. Run [checks](checks.md) after applying; keep the transaction for [history](history.md) and [patch export](git.md).
