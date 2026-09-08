# Edit a selected implementation

For insertion, a lookup explicitly scoped with `--in FILE_PATH` already returns that file's handle in `root`; retain it.
An unscoped or directory-scoped lookup has a different root. Use `project map FILE_PATH --depth 0 --fields handle,kind,name --limit 1` when the file handle is missing.

Choose an operation:

- `fr author replace-body HANDLE --from FILE`: one Rust, TypeScript or TSX block. Direct function bindings need block bodies; wrappers and expression bodies refuse.
- `fr author replace-declaration HANDLE --from FILE`: one complete Rust function with the same name; outer attributes remain. Callers need separate edits if the signature changes.
- `fr author insert-declaration FILE_HANDLE --from FILE`: append one Rust function, optionally preceded by `///` or `/** ... */` documentation. Other outer attributes and surrounding comments refuse. Duplicate direct names and pending outer metadata refuse.

Keep the UTF-8 fragment outside the project; the input and affected declarations/blocks must fit 64 KiB.
Use `--save-plan` to preview and freeze the edit. Inspect the bounded diff and omissions, then `fr history apply TX --write --no-diff` applies that exact transaction.
Run [project checks](checks.md) for compilation and behavior after applying the plan.

Keep TX for [undo/redo](history.md) and [patch export](git.md).
Nested insertion and other declaration kinds remain unsupported. A normal editor fallback does not automatically enter fr history.

For example, add a wrapper around an existing Rust `increment` function.
Save this fragment outside the project as `<FRAGMENT>`:

```rust
/// Increments a value twice.
pub fn increment_twice(value: u32) -> u32 {
    increment(increment(value))
}
```

Use the lookup's declaration handle as `<AUTHOR_HANDLE>` and its file-scoped `root` as `<FILE_HANDLE>`.
Inspect the implementation and saved diff before applying the returned `<AUTHOR_TX>`:

```sh
fr project find increment --in src/lib.rs --signature
fr project show '<AUTHOR_HANDLE>' --source --bytes 512
fr author insert-declaration '<FILE_HANDLE>' --from '<FRAGMENT>' --save-plan
fr history apply '<AUTHOR_TX>' --write --no-diff
```
