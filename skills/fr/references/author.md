# Prepare one complete change

A body holds executable statements.
A parameter declares an input; an argument supplies its value at a call.
Use `project find NAME --in FILE --source` to select a declaration.
Read each `rows` entry using the matching `columns` names.
For insertion, the query's `root` selects the file.

A fragment file holds new code or refactoring JSON.
Save the table's JSON as `.fr/refactor` and set `"from":".fr/refactor"` in the manifest.
Paths start at the project root.

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

Replace `increment`'s body with `{ value + 1 }` in `.fr/body`. Save this helper in `.fr/helper`:
```rust
/// Calls increment twice.
pub fn increment_twice(value: u32) -> u32 { increment(increment(value)) }
```
A manifest is a JSON file listing operations for one source revision.
Save `.fr/author` with the returned declaration and file handles:
```json
{"operations":[{"op":"replace-body","handle":"<DECLARATION_HANDLE>","from":".fr/body"},{"op":"insert-declaration","handle":"<FILE_HANDLE>","from":".fr/helper"}]}
```
`--save-plan` prints the preview and saves a plan without editing source.
Review that preview before applying it:
```sh
fr project find increment --in src/lib.rs --source --bytes 512
fr author batch --from .fr/author --save-plan
```
After saving, use [Workflow](workflow.md) to apply, check and deliver.
The saved plan's `transaction` identifies the edit; retain its `transaction_context_basis` too.
Changed source, incomplete selections and overlapping edits refuse. Limits: 32 operations and 64 KiB fragments.
Use [Semantic](semantic.md) for structured body changes; `fr author guide` lists other operations.
