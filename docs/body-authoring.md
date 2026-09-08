# Bounded function authoring

`fr author replace-body HANDLE --from FILE` replaces one function block through a current project handle.
It supports Rust, TypeScript and TSX.
It retains the signature, outer attributes, documentation and every byte outside that block.
This native command complements the existing refactorings when an agent needs to write a new implementation.

```sh
fr project map src/lib.rs --fields handle,parent,kind,name,line --limit 12
fr project show '<HANDLE>' --source --bytes 512
fr author replace-body '<HANDLE>' --from /tmp/body.txt
fr author replace-body '<HANDLE>' --from /tmp/body.txt --save-plan
fr history show '<TX>'
fr history apply '<TX>' --write
```

`HANDLE` identifies the function, not its file or enclosing type.
A short ID requires the map's `--revision`; a full handle includes its revision.
Use the same workspace root and scan options as the map.
Source, manifest and inventory changes invalidate handles; obtain a new map after those changes.

## Input and supported scope

The input file contains exactly one complete block in the target language, including braces.
For Rust:

```rust
{
    let doubled = value * 2;
    doubled
}
```

Both the old and new blocks must fit 2 through 65,536 UTF-8 bytes.
The input file itself must fit 65,536 bytes, including whitespace outside the block.
The reader trims surrounding whitespace and preserves all bytes inside the block.
It refuses non-UTF-8 input, NUL bytes, symlink inputs and nonregular files.
Relative input paths resolve from the workspace root. Input files can reside outside the workspace.
Writing the fragment outside the project avoids invalidating a previously obtained map through inventory changes.

Rust targets include ordinary functions, methods, default trait method bodies and nested function items.
TypeScript and TSX targets include named function declarations, generators, class and object methods, accessors and constructors.
They also include variable or class-field initializers containing block-bodied arrows, ordinary function expressions or generator expressions.
The function can sit inside nested parentheses, `as`, `satisfies`, postfix non-null `!` assertions and TypeScript angle-bracket assertions.
Angle-bracket assertions belong to the TypeScript grammar; TSX retains its JSX grammar and does not support that assertion syntax.
Use the variable or field's handle, including when a function expression has a separate inner name.
Variable lookups need `--locals`; `project find NAME --in FILE --locals --source` supplies the handle and selected declaration.
The operation retains the binding, wrappers, function expression, parameters and arrow token; it replaces only the braces and their contents.
Nested declarations, exports, generics, modifiers, signatures and surrounding attributes stay in place.
Select the specific implementation handle when multiple declarations share a name, such as overloads or getter/setter pairs.
Expression-bodied arrows, destructured bindings, bodyless declarations, nonfunction variables and file handles refuse.
Calls such as `memo(...)`, conditionals, comma expressions and other initializer forms refuse, including inside otherwise supported wrappers.
Selection follows only the expression operand of each supported wrapper; it does not search callbacks, type operands or alternatives for a function.
Object properties containing function expressions and anonymous callbacks remain outside this selection path.
Computed and quoted method names are outside the indexed method subset.
JavaScript extensions use the existing TypeScript grammar; JSX extensions use TSX.
This does not impose JavaScript-only syntax rules on `.js` files.
The target file must have no parser errors before the edit.
The replacement must parse as exactly one block in a temporary function, and the resulting destination file must also parse without errors.
Additional declarations outside the replacement block, trailing comments and unmatched braces refuse.
Nested declarations inside the new block are allowed.

The command does not update callers, signatures or imports and does not check types, control flow or behavior.
Macros and language context rules retain the parser's syntax coverage limits.
Choose project compiler and test commands that establish the intended behavior after applying.
An implementation change may deliberately change behavior; syntax acceptance does not validate that intention.
Further languages, additional initializer forms and insertion into impl, trait or function bodies remain roadmap work.
For complete Rust function changes, see [declaration replacement](#declaration-replacement).

For a TSX component, the fragment may contain JSX:

```tsx
{
    return <span>{value * 2}</span>;
}
```

Use a handle from a `.tsx` or `.jsx` file for JSX bodies; a `.ts` target retains its TypeScript grammar.

## Declaration replacement

`fr author replace-declaration HANDLE --from FILE` replaces a complete Rust function, including its signature and body.
It retains the exact function-name spelling and every byte outside the function item, including outer attributes and documentation.
Use an ordinary function, method, default trait method or nested function handle.
Other declarations, bodyless signatures and non-Rust targets refuse.

The fragment must contain exactly one function with the same name and a body.
For example, `pub fn calc(n: i64) -> i64 { n * 2 }` can replace an existing `calc` function.
Exclude outer attributes, leading or trailing comments and other declarations from the fragment.
Both complete declarations and the raw input file must fit 64 KiB; whitespace outside the fragment is trimmed.
Generics, visibility, qualifiers, parameter types and return types may change.
Callers and imports stay as they were. Choose existing refactorings when a change should update callers automatically.
Changing the signature may break compilation even though both parses are clean.

The report uses query `replace-declaration`, with `declaration` spans, byte counts and fingerprints.
It includes the old `signature` and bounded `replacement_signature`.
The saved-plan, diff-budget and history rules below apply to all authoring operations.

## Declaration insertion

`fr author insert-declaration HANDLE --from FILE` adds one Rust function to an indexed Rust file or selected inline module.
Select a file or module handle from the project map or lookup. Function, impl, trait, directory and non-Rust handles refuse.
An external `mod name;` declaration has no inline body; select its source file instead.
Empty files are supported. The fragment must contain one function with a body, optionally preceded by outer Rust documentation comments.
Leading `///` and `/** ... */` comments attach to the inserted function, including under a crate-level `deny(missing_docs)` lint.
Inner documentation, ordinary surrounding comments and explicit outer attributes, including `#[doc]`, refuse. Replacement declarations still preserve existing documentation and reject new leading comments.
The raw input and trimmed fragment, including documentation, must fit 64 KiB.

Every existing source byte is preserved. File insertion appends at EOF; module insertion goes before the selected body's closing brace.
When that brace is on a whitespace-only line, insertion goes before the line's indentation. Otherwise, it goes immediately before the brace token.
The operation chooses LF or CRLF from the file's first newline, defaulting to LF when there is none.
It adds a leading newline when the preserved prefix is nonempty and does not end in LF, and always adds a trailing newline.
The trimmed fragment remains verbatim, including line endings and multiline string contents; insertion does not indent or format it.
The added separators can total four bytes beyond the fragment limit.
Trailing ordinary comments remain intact. Pending outer documentation or attributes refuse because they could attach to the new function.
Existing inner attributes and documentation continue to apply to their file or module.

The name check refuses matching direct item names in the selected file or module, treating `calc` and `r#calc` as the same spelling.
It checks direct syntax and does not distinguish Rust namespaces or evaluate conditional compilation.
Names in parents, siblings or nested modules do not block insertion. Imports, macro expansion and full name resolution remain unchecked; run the compiler after applying.
An identical existing function also refuses; insertion has no no-op case.

The report uses query `insert-declaration` and provides the new declaration's name, span, byte count and fingerprint.
`insertion` contains the empty original span, complete added span, separator strings, added-byte count and fingerprint.
Its `signature` describes the new function, excluding leading documentation even when that documentation exceeds the header output budget.
For documented insertions, `documentation` identifies the leading comment region and intervening whitespace by span, byte count and fingerprint.
The declaration span and fingerprint cover the complete fragment, including that documentation region.
Module reports also include `container` with kind `inline-module`, its name and `before_span` for the original body including both braces.
`name_resolution_checked: false` makes the name-check boundary explicit.
Use the returned source-history transaction for exact application, undo/redo and patches.

## Review and transactions

Both output modes return JSON with schema `fr-author-1`.
The report includes the reviewed revision and handle, bounded path and signature, coverage and absolute body byte spans.
For function bindings, the signature starts at the selected declarator or field and excludes neighboring bindings and their bodies.
It stops before the body; postfix wrappers and type assertions remain visible in the selected source, rather than in this header excerpt.
Fingerprints use SHA-256 over the exact bytes of the reported fragment or insertion.
`validation: reparse-strict` and `behavior_checked: false` separate syntax evidence from behavioral checks.

The default diff budget is 4,096 UTF-8 bytes. `--diff-bytes` accepts 0 through 65,536.
A complete diff is a string. A clipped diff contains `text` and `omitted_bytes`, matching project text clipping.
The cap applies before JSON escaping; serialized output can therefore be larger.
A clipped diff alone does not show the full change. Read the selected body, the proposed fragment or the saved history record as needed.
The budget limits output, not workspace indexing or internal validation work.

Previewing writes neither source nor a history record.
`--save-plan` stores the exact validated replacement and returns a source-history transaction ID.
Apply that ID after review; later changes to the fragment file do not change the saved transaction.
Saving checks the current project revision. Later history application checks the recorded affected-file snapshots, including existence and modes.
Unrelated manifest or source changes can invalidate project handles without blocking an already saved transaction; rerun project checks after applying.
A direct `--write` computes and applies a new plan from the current fragment. It conflicts with `--save-plan`.
An identical replacement produces no transaction and returns `changed: false` and `applied: false`.

Use source-history undo, redo, recovery and patch export with the returned ID.
Undo/redo preserve unrelated edits and refuse conflicts in affected files.
History retains the full file snapshots even when the authoring preview clips its diff.
Project handles become stale after the edit; refresh them before another structural selection.
History's existing lock, sync, recovery and non-atomic filesystem limits apply.
Source verification and recording are separate observations; this command does not claim a global atomic snapshot against uncooperative writers.

## Evidence

Thirty-nine CLI scenarios cover saved edit identity, compiled behavior, undo/redo, patch export, stale handles and revisions, unsupported targets and malformed input.
They also check exact size limits, diff omission, method and nested-function contexts, Unicode and CRLF preservation, no-op writes, symlink inputs and Unix permissions.
TypeScript and TSX fixtures compile with `tsc --strict` and run in Node before and after saved replacements.
The tests cover JSX, supported declaration forms, extension aliases and unsupported selections without editing an enclosing function.
Function-binding cases preserve neighboring declarations, shadowed bindings, lexical receivers, named recursion and generator behavior through saved transactions.
Wrapped cases add nested assertions, comments between operands, TSX rendering and wrapper changes that invalidate handles and saved plans.
Brace-token spans preserve external comments and semicolons; duplicate names retain separate implementation selections.
The size predicate has a source anchor and signature map into Lean, with 64 shared boundary cases including machine limits.
Lean proves its lower and upper bounds and symmetry between old and new body sizes.
The existing edit model describes a splice as an unchanged prefix, replacement and unchanged suffix.
The same size guard and edit model apply to all three languages.
Declaration cases cover compiled signature changes, preserved outer attributes, exact name spelling and complete-item size limits.
Lean proves both prefix and suffix preservation for valid splice boundaries, including replacements that change length.
An edit reported by the declaration CLI also passes through Rust and Lean with matching results.
The same splice comparison now checks a reported TSX body replacement inside nested wrappers, retaining Unicode, CRLF and a neighboring declaration.
An incompatible signature fixture confirms that syntax acceptance can still leave a compiler error in a caller.
Insertion cases cover EOF and module placement, empty files, separators, duplicate names and unattached outer metadata.
Module fixtures check same-named selections, raw identifiers, nested scopes, closing-brace indentation and verbatim multiline strings.
A documented nested function compiles and calls its private sibling after saved application and redo; undo restores the original bytes.
Lean proves that removing inserted characters at a valid boundary recovers the original source.
Reported file and nested-module insertions, including their separators, also produce matching Rust and Lean results.
These proofs do not establish general correspondence for AST selection, parsing, type correctness, filesystem operations or the full authoring command.
