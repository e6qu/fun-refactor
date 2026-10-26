# Prepare one complete change

A function body holds its executable statements.
A parameter is an input in its declaration; an argument is a value supplied by a caller.
Use `project find NAME --in FILE --source` to select a declaration.
For insertion, use that query's `root` file handle for the destination file.

Choose an operation and save its contents in an extensionless fragment under `.fr/`:

| Change | Operation | Fragment |
| --- | --- | --- |
| Replace a body | `replace-body` | New body; include Rust/TypeScript braces, omit the Python function's outer indentation |
| Rename a function | `refactor` | `{"operation":"rename","name":"NEW_NAME","documentation":"preserve"}` |
| Remove an unused Rust parameter | `refactor` | `{"operation":"remove-parameter","index":0}`; positions start at zero |
| Add a Rust function | `insert-declaration` | Complete declaration and documentation comment |

Rename supports Rust, Python and TypeScript functions outside classes or type implementation blocks.
It updates resolved uses and preserves explicit import aliases.
Parameter removal updates declared direct callers. The discarded argument must be a literal, such as `false`.
Retained expressions keep their evaluation order. Refuse removal of an argument with effects, such as `record()`.
External consumers remain unknown; read warnings and review every affected file.

For example, replace `increment`'s body with `{ value + 1 }` and add this helper.
Save the body and declaration in separate fragment files:
```rust
/// Calls increment twice.
pub fn increment_twice(value: u32) -> u32 { increment(increment(value)) }
```
A manifest is a JSON file listing operations from one source revision.
Save this as `.fr/author`, replacing the fragment paths and declaration and file handles:
```json
{"operations":[{"op":"replace-body","handle":"<DECLARATION_HANDLE>","from":"<BODY_FILE>"},{"op":"insert-declaration","handle":"<FILE_HANDLE>","from":"<FRAGMENT>"}]}
```
Use `--save-plan` on the first author call. It prints the full preview and stores a plan without editing source.
Review that preview before applying it:
```sh
fr project find increment --in src/lib.rs --source --bytes 512
fr author batch --from .fr/author --save-plan
```
After saving, use [Workflow](workflow.md) to apply, check and deliver.
The saved plan's `transaction` identifies the edit; retain its `transaction_context_basis` too.
Drift, clipping and overlaps refuse. Limits: 32 operations and 64 KiB fragments.
Use [Semantic](semantic.md) for structured body changes; `fr author guide` lists other operations.
