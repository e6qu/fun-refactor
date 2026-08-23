# grammars/

Grammars this project compiles itself, because the published one cannot parse code the
language accepts.

Everything under `vendor/` is reference material. Everything here is built into the
binary, so each directory carries the upstream source, the licence, the patch applied to
it, and the provenance to check all three.

## Adding one

1. Fetch the upstream release the crate publishes, at its tag.
2. Patch `grammar.js`. The patch must keep the parser correct for the language: it may
   accept what the language accepts, and nothing more.
3. Regenerate with the `tree-sitter` CLI, and keep `src/parser.c` beside the grammar.
4. Save the diff against the untouched `grammar.js`, so the next upgrade can re-apply it.
5. Record the source, tag, checksum and licence in `PROVENANCE.toml`.
6. Prove the patch is additive: parse a body of real code with the stock parser and the
   patched one, and compare the trees. They must agree everywhere the stock parser
   succeeds.

## zig

`struct {}` is ordinary Zig, and `tree-sitter-zig` cannot read it. Its four container
rules take `$._container_members`, which needs at least one member, while `source_file`
takes `optional($._container_members)` and reads an empty file. The patch gives the four
containers the same `optional`, so `struct {}`, `enum {}`, `union {}` and `opaque {}`
parse as the empty containers they are.

Checked against `zls`'s `DocumentStore.zig` and `offsets.zig` and this repository's own
sample: 14,463 nodes, and the patched parser returns the same tree as the stock one for
every byte of it.

## python

Two forms of ordinary Python failed. A starred element in an unparenthesised tuple
parsed only when it was a name, because the grammar reads that position as a pattern:
`g = 1, *rest` worked and `g = 1, *[2]` did not. And a type parameter could carry no
default, so `type A[T = int] = float` failed, which PEP 696 added in Python 3.13.

`expression_list` now takes the choice Python's own star_expressions has, and each type
parameter takes an optional `= type`. Checked over 102 files and 50,399 nodes: the
patched parser returns the same tree as the stock one everywhere the stock one parses.
