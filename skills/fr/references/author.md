# Edit a selected implementation

For file insertion, a lookup explicitly scoped with `--in FILE_PATH` already returns that file's handle in `root`; retain it.
An unscoped or directory-scoped lookup has a different root. Use `project map FILE_PATH --depth 0 --fields handle,kind,name --limit 1` when the file handle is missing.

Choose an operation:

- `fr author replace-body HANDLE --from FILE`: one Rust, Go, TypeScript or TSX block, including supported wrapped function bindings.
- `fr author replace-declaration HANDLE --from FILE`: one complete Rust function with the same name; outer attributes remain. Callers need separate edits if the signature changes.
- `fr author insert-declaration HANDLE --from FILE`: add one Rust function to a file or inline module, optionally preceded by `///` or `/** ... */` documentation. Other outer attributes and surrounding comments refuse. Duplicate direct names and pending outer metadata in the selected container refuse.

For coordinated edits, use `fr author batch --from MANIFEST --save-plan` to save one transaction for up to 32 disjoint operations.
The JSON manifest has `operations` entries with `op`, `handle` and `from`, using the operations above. Short IDs need a top-level `revision`.
Relative fragment paths resolve from the workspace root. Keep the manifest and fragments outside the project; each file must fit 64 KiB.
All handles refer to the original source. Overlapping selections and shared insertion boundaries refuse; later steps cannot target newly inserted code.
Review the combined diff. Step spans describe original source; coverage appears once. Apply, undo/redo and export the single returned transaction.

For module insertion, use the module row's handle from `project find NAME --in FILE --source`, rather than its file-scoped `root`.
Insertion goes before the module's closing brace and preserves existing bytes.
Fragment boundary whitespace is trimmed; remaining bytes stay verbatim, with no automatic indentation.

For Go, select a named function or receiver method handle. Interface specifications and variables containing function literals refuse.
Receiver headers, type parameters and directives outside the body stay unchanged; imports and type correctness need compiler checks.

TypeScript/TSX function bindings accept parentheses, `as`, `satisfies`, postfix `!` and TypeScript angle-bracket assertions; the body must be a block.
Use the variable or field handle, with `project find NAME --in FILE --locals` for variable bindings.
Calls such as `memo(...)`, conditionals, comma expressions and expression-bodied arrows refuse.

Keep the UTF-8 fragment outside the project; the input and affected declarations/blocks must fit 64 KiB.
Use `--save-plan` to preview and freeze the edit. Inspect the bounded diff and omissions, then `fr history apply TX --write --no-diff` applies that exact transaction.
Run [project checks](checks.md) for compilation and behavior after applying the plan.

Keep TX for [undo/redo](history.md) and [patch export](git.md).
Insertion into impl, trait or function bodies and other declaration kinds remain unsupported. External `mod name;` declarations require selecting their source file. A normal editor fallback does not automatically enter fr history.

For example, add a wrapper around an existing Rust `increment` function.
Save this fragment outside the project as `<FRAGMENT>`:

```rust
/// Increments a value twice.
pub fn increment_twice(value: u32) -> u32 {
    increment(increment(value))
}
```

Use the lookup's file-scoped `root` as `<FILE_HANDLE>`.
Inspect the `source` column and saved diff before applying the returned `<AUTHOR_TX>`:

```sh
fr project find increment --in src/lib.rs --source --bytes 512
fr author insert-declaration '<FILE_HANDLE>' --from '<FRAGMENT>' --save-plan
fr history apply '<AUTHOR_TX>' --write --no-diff
```

The source budget is shared across page rows; an empty slice can mean the budget ran out.
For a non-null `source.next_offset`, continue with `project show HANDLE --source --offset NEXT --bytes N` using that row's handle.
