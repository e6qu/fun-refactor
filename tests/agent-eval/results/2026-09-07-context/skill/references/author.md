# Edit a selected implementation

Find a known name with `fr project find NAME --signature`. Add `--in PATH` to disambiguate.
Use `--contains` for a literal partial name; read only the relevant body with `project show HANDLE --source --bytes N`.
To insert into an existing file, `project map PATH --depth 0 --fields handle,kind,name --limit 1` returns its file handle.

Choose an operation:

- `fr author replace-body HANDLE --from FILE`: one Rust, TypeScript or TSX block. Direct function bindings need block bodies; wrappers and expression bodies refuse.
- `fr author replace-declaration HANDLE --from FILE`: one complete Rust function with the same name; outer attributes remain. Callers need separate edits if the signature changes.
- `fr author insert-declaration FILE_HANDLE --from FILE`: append one Rust function without outer attributes or surrounding comments. Duplicate direct names and pending outer metadata refuse.

Keep the UTF-8 fragment outside the project; the input and affected declarations/blocks must fit 64 KiB.
Use `--save-plan` to preview and freeze the edit. Inspect the bounded diff and omissions, then `fr history apply TX --write` applies that exact transaction.
A saved plan leaves source unchanged. Parsing checks syntax; run [project checks](checks.md) for compilation and behavior.

Keep TX for [undo/redo](history.md) and [patch export](git.md). These commands do not need refreshed project handles.
Obtain a fresh handle only before another source query or authoring operation needs one.
Nested insertion and other declaration kinds remain unsupported. A normal editor fallback does not automatically enter fr history.
