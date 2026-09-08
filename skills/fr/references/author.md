# Edit selected code

`fr author` accepts revision-bound handles:

- `replace-body`: one Rust, Go, TypeScript, or TSX block.
- `replace-declaration`: one same-named Rust function; callers need separate edits after signature changes.
- `insert-declaration`: one Rust function in a file, inline module, impl, or trait; bodyless functions are trait-only.
- `batch`: up to 32 disjoint operations saved as one transaction.

Find a declaration with `project find NAME --in FILE --source`. That report's `root` is the file handle. For module or trait insertion, use that container's row handle. For an exact impl or trait body, use a handle for any existing direct method in it. Empty impls currently have no selectable structural handle. Use `project map FILE --depth 0 --fields handle,kind,name --limit 1` if no file handle is available.

Fragments are UTF-8 files outside the project and at most 64 KiB. A batch manifest contains `operations` with `op`, `handle`, and `from`; short IDs also require top-level `revision`. All handles select the original source. Overlaps and shared insertion boundaries refuse.

Review the combined diff, then save and apply the checked transaction. A complete saved diff includes `transaction_context_basis` for compact forward apply/redo reports.
Repeating an identical saved plan reuses its transaction and reports `reused_transaction: true` with `saved: false`.

Go accepts named functions and receiver methods. TypeScript/TSX accepts supported function bindings; arrows accept an expression or block and can move between forms. Rust insertion accepts `///` or `/** */` docs, rejects other outer attributes or pending metadata, trims boundary whitespace, and preserves the remaining fragment bytes. A trait accepts a bodyless function declaration; files, modules and impls require a body. Unsupported declaration kinds refuse.

Example: save this outside the project as `<FRAGMENT>`:

```rust
/// Increments a value twice.
pub fn increment_twice(value: u32) -> u32 {
    increment(increment(value))
}
```

Use the lookup's file-scoped `root` as `<FILE_HANDLE>`:

```sh
fr project find increment --in src/lib.rs --source --bytes 512
fr author insert-declaration '<FILE_HANDLE>' --from '<FRAGMENT>' --save-plan
fr history apply '<AUTHOR_TX>' --write --no-diff
```

The source byte budget is shared across rows. Continue a non-null `source.next_offset` with `project show HANDLE --source --offset NEXT --bytes N`. Run [checks](checks.md) after applying. Keep the transaction for [history](history.md) and [patch export](git.md).
