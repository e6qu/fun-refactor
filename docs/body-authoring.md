# Bounded Rust function-body authoring

`fr author replace-body HANDLE --from FILE` replaces the block of one Rust function through a current project handle.
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

The input file contains exactly one complete Rust block, including braces:

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

Supported targets include ordinary Rust functions, methods, default trait method bodies and nested function items.
Generics, modifiers and surrounding attributes stay in place.
A bodyless declaration, closure, variable, constant, file or non-Rust function refuses.
The target file must have no parser errors before the edit.
The replacement must parse as exactly one block in a temporary function, and the resulting destination file must also parse without errors.
Additional declarations outside the replacement block, trailing comments and unmatched braces refuse.
Nested declarations inside the new block are allowed.

The command does not update callers, signatures or imports and does not check types, control flow or behavior.
Macros retain the parser's syntax coverage limits.
Choose project compiler and test commands that establish the intended behavior after applying.
An implementation change may deliberately change behavior; syntax acceptance does not validate that intention.
Other languages, declaration insertion and whole-declaration replacement remain roadmap work.

## Review and transactions

Both output modes return JSON with schema `fr-author-1`.
The report includes the reviewed revision and handle, bounded path and signature, coverage and absolute body byte spans.
Body fingerprints use SHA-256 over the exact block bytes; byte counts describe the old and new blocks.
`validation: reparse-strict` and `behavior_checked: false` separate syntax evidence from behavioral checks.

The default diff budget is 4,096 UTF-8 bytes. `--diff-bytes` accepts 0 through 65,536.
A complete diff is a string. A clipped diff contains `text` and `omitted_bytes`, matching project text clipping.
The cap applies before JSON escaping; serialized output can therefore be larger.
A clipped diff alone does not show the full change. Read the selected body, the proposed fragment or the saved history record as needed.
The budget limits output, not workspace indexing or internal validation work.

Previewing writes neither source nor a history record.
`--save-plan` stores the exact validated replacement and returns a source-history transaction ID.
Apply that ID after review; later changes to the fragment file do not change the saved transaction.
Applying checks the recorded source basis and affected snapshots, including existence and modes.
A direct `--write` computes and applies a new plan from the current fragment. It conflicts with `--save-plan`.
An identical block produces no transaction and returns `changed: false` and `applied: false`.

Use source-history undo, redo, recovery and patch export with the returned ID.
Undo/redo preserve unrelated edits and refuse conflicts in affected files.
History retains the full file snapshots even when the authoring preview clips its diff.
Project handles become stale after the edit; refresh them before another structural selection.
History's existing lock, sync, recovery and non-atomic filesystem limits apply.
Source verification and recording are separate observations; this command does not claim a global atomic snapshot against uncooperative writers.

## Evidence

Eight CLI scenarios cover saved replacement identity, compiled behavior, undo/redo, patch export, stale handles and revisions, unsupported targets and malformed input.
They also check exact size limits, diff omission, method and nested-function contexts, Unicode and CRLF preservation, no-op writes, symlink inputs and Unix permissions.
The size predicate has a source anchor and signature map into Lean, with 64 shared boundary cases including machine limits.
Lean proves its lower and upper bounds and symmetry between old and new body sizes.
The existing edit model describes a splice as an unchanged prefix, replacement and unchanged suffix.
These proofs do not establish general correspondence for AST selection, Rust parsing, type correctness, filesystem operations or the full authoring command.
