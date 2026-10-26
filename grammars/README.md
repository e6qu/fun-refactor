# grammars/

A user inspecting Zig `struct {}` should see an empty type. The original parser invented an empty field. The patches below fix such reading errors, so later commands start from the code the user wrote. Parsing correctly does not establish that every edit or analysis supports that code.

A grammar describes the syntax a parser recognizes. tree-sitter is the parser library used here.
Generated parser code turns that grammar into a tree of source locations and syntax kinds.
For a Zig empty structure, a parser inventing a field gives later analysis the wrong starting facts.
These patches fix particular reading errors; they do not establish complete language support.

Grammars this project compiles itself. Most are here because the published parser cannot
read code the language accepts. Lean is here because its crate links a different
tree-sitter, and two crates cannot both claim `links = "tree-sitter"`.

Everything under `vendor/` is reference material. Everything here is built into the
binary, so each directory carries the upstream source, the licence, whatever patch it
took, and the provenance to check all three.

## Adding one

Run grammar generation and real-project comparisons on a dedicated runner.
The [shared workstation limits](../docs/development.md#working-on-a-shared-desktop) apply to local inspection.
For example, a grammar fix for an empty Zig type must also keep ordinary nonempty types readable.
The comparison below checks supported neighbors as well as the failing form.

1. Fetch the upstream release the crate publishes, at its tag.
2. Patch `grammar.js` where the published parser is wrong. The patch must keep the parser
   correct for the language: it may accept what the language accepts, and nothing more.
   A grammar vendored for a version conflict takes no patch.
3. Regenerate with the `tree-sitter` CLI, and keep `src/parser.c` beside the grammar.
4. Save the diff against the untouched `grammar.js`, so the next upgrade can re-apply it.
5. Record the source, tag, checksum and licence in `PROVENANCE.toml`.
6. Compare the original and patched parsers on real code. Require the same trees where the original parser succeeds,
   apart from explicitly documented corrections. Test the failing form and nearby valid and invalid forms.
   This finite comparison can find regressions; it does not prove correctness for every program.

## Why generated parsers are committed

The `tree-sitter` CLI generates it from `grammar.js`, and its first line says so. Every
grammar here keeps the generated file anyway, so `cargo build` needs a C compiler and
nothing else. Generating at build time instead would make that CLI a hard dependency of
every clone and every CI run.

Generated files can be large even when their compressed download is much smaller.
Measure source size, Git download size and the built browser bundle separately when reviewing a grammar update.
An old size measurement is not a current download budget.

The language notes below record the examples and finite comparisons used for these patches.
Their counts describe those original checks. They do not describe all programs accepted by a language or the current browser bundle.

## lean

`mutual def a := b  def b := a  end` is the only form Lean has for mutual recursion.
The published grammar has no rule for it, and the declarations inside come back as
errors.
The patch adds `mutual <declaration>+ end` to `_command`, which is where every other
block-shaped command already sits.

This build's Lean writer sorts a module's declarations so that each comes after
everything it names, and a cycle has no such order. `mutual` is what it emits there, so
without the rule the reparse gate refuses the translation outright.

Lean also accepts `rcases invalid with left | right`, which splits an either/or
assumption into two proof cases. The reader incorrectly required `(left | right)`.
Both forms now have an explicit alternative-pattern node. The page-count example
keeps the unparenthesized form as a regression; Lean must still check the completed proof.

## zig

`struct {}` is ordinary Zig. The four container rules take `$._container_members`, which
needs at least one member, while `source_file` takes `optional($._container_members)` and
reads an empty file. Given `struct {}` the published grammar reports no error. It
returns a `container_field` whose name is zero bytes long: a member no line of the file
declares.
The patch gives the four containers the same `optional`. So `struct {}`, `enum {}`,
`union {}` and `opaque {}` come back as the empty containers they are.

Checked over `zls`, 77 files and 231,518 nodes: the patched parser returns the same tree
as the stock one everywhere, and the phantom field is the only thing that goes.

## python

Two forms of ordinary Python failed. A starred element in an unparenthesised tuple
parsed only when it was a name, because the grammar reads that position as a pattern:
`g = 1, *rest` worked and `g = 1, *[2]` did not. And a type parameter could carry no
default, so `type A[T = int] = float` failed, which PEP 696 added in Python 3.13.

`expression_list` now takes the choice Python's own star_expressions has, and each type
parameter takes an optional `= type`. Checked over `psf/requests` and `pallets/flask`,
119 files and 136,341 nodes: the patched parser returns the same tree as the stock one
everywhere the stock one parses.

## sass

Sass has two syntaxes, and this is the older one: blocks are indentation and statements
end at the line. It is a different language from the braced syntax in `.scss` files, so
it needs a grammar of its own, and no crate publishes one. `bajrangCoder/tree-sitter-sass`
is unreleased, so the pin here is a commit and the archive checksum is what makes it
checkable.

Six forms of ordinary Sass failed. The CSS colour, gradient and maths functions had name
tokens that outranked the identifier token, so `transition: color 0.2s` failed and so did
`color.adjust(…)`. A call took no named argument. A list in parentheses read as a value in
brackets. A selector list could not be written down the page. A hyphen could not join two
interpolations, `.#{$abbrev}-#{$size}`. And a combinator with spaces around it, `li + li`,
lost its left-hand selector to the descendant combinator, which is a run of spaces.

The last of those is the scanner's to decide, as it is in `tree-sitter-css`. A run of
spaces is the descendant combinator only when a selector follows it. The rest are rules.

Measured over `iv-org/invidious`, `peer-calls/peer-calls`, `HBM/jet` and the grammar's own
examples, 17 files: the published grammar fails on 8 and this one on 1. That one is a
Jekyll asset whose first line is YAML front matter. The grammar's own corpus passes whole,
42 of 42, with three trees regenerated for the rules the patch removes.

## go

`new` and `make` are predeclared identifiers in Go, not keywords, so a package may define
a function called `new` and call it. `tree-sitter-go` gives the name one argument list,
the special one whose first argument is a type, so `new("-10s")` and `new(err.Error())`
are error nodes. They account for 177 of the 178 Go files that fail to parse in
`grafana/grafana`.

The patch lets either argument list follow the name, and gives the one taking a type the
higher dynamic precedence, so every call the stock parser reads keeps the tree it had.
Checked against `spf13/cobra`, `gin-gonic/gin`, `sirupsen/logrus` and the grammar's own
examples: 181 files, 281,884 nodes, identical trees. The grammar's own corpus passes
whole, 67 of 67.

## typescript

Two forms of ordinary TypeScript failed. An import type, `import("@babel/types").Statement`,
was a whole `type` and nothing smaller, so it took no `[]` and no type arguments. And a
member called `in` ended the interface it sat in. The scanner never ends a line before
`in`, which is an operator in an expression. Both were found in `vuejs/core`.

The import-type forms move to `primary_type`, which is what an array type and a generic
type are built from, and `generic_type` takes one as a name. In a type there is no `in`
and no `instanceof` operator, so a line opening with either ends the member before it;
an identifier that merely begins with one, `in2`, ends it in an expression too, which the
published scanner also got wrong.

Checked against `vuejs/core` and `excalidraw/excalidraw`: 476 TypeScript files and 199
TSX files, 812,690 nodes, and the patched parser returns the same tree as the stock one
everywhere the stock one parses. Two files it reads that the stock one cannot, which are
the two the entries came from. Both grammars' own corpus passes, 49 of 49.

## scss

The published grammar failed on 203 of the 276 stylesheets in `twbs/bootstrap` and
`jgthms/bulma`; the patched one fails on none. Its gaps ran from `$m: (a: 1)` and
`!default` to `@use "x" as t`, so the patch is wide. Three parts of it are worth naming.

A colon opens a pseudo class when a `{` follows before the statement ends, and the
scanner took the brace of an interpolation for that one, so `color: #{$v}` read as a
selector. It now reads past an interpolation, and `plain_value` stops before one.

A map is told from a list by a colon of the bracket's own, which is further ahead than
any rule can see. The scanner looks for it and hands the parser a different bracket,
which is the lookahead Sass itself does. It offers that bracket only where whitespace
precedes the `(`, because a bracket flush against a name is a call's.

`%` is a unit glued to a number and the modulo operator when it stands apart, and `-` is
a sign glued to a variable and subtraction when it stands apart. Both are rules about
spacing, so both belong to the token and not to the grammar around it.

Checked over the 73 files the published grammar reads cleanly, 5,068 nodes: one tree
differs, `$return: ()`, where the published grammar invents a zero-width `integer_value`
inside a `parenthesized_value` and this one reads the empty list that is written.
