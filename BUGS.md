# BUGS

Known defects and limitations, and their status. Updated alongside PLAN.md at every
stage.

Format: `- [ ] B<N>: <symptom>`, then where it happens, then status and notes.

Open entries are characterised limitations: the behaviour is reported and no operation
silently does the wrong thing.

Every open entry is pinned by a test, so a claim that stops being true fails a build
instead of sitting here. B11 said `@content` was a gap after it had stopped being one, and
nothing noticed. The grammar limits are pinned by `tests/known_grammar_gaps.rs`, from both
sides, the failing form and the neighbouring forms that work. The ones that are this
tool's own behaviour are pinned by `tests/open_defects.rs`. Each asserts the whole entry:
what the tool does not do, and what it reports instead. Every one of these stands on the
second half. A test that checked only the first would pass just as well if the report went
away.

## Open

Re-triaged against this branch. The entry below still reproduces. Where a published
grammar could not read source the language accepts, this build compiles a patched copy
instead of recording the gap: `grammars/` holds one for Go, Python, Sass, SCSS,
TypeScript and Zig, each with its upstream pin, licence, patch and the measurement that
shows the patch additive. What is left below is one limit of this tool's own analysis.

- [ ] B5: `find_unused` and the call graph follow what the source shows, and no further.
  A call whose receiver nothing types is fanned out to the definitions the workspace
  admits. Four declarations each fan such a call out to every implementation:

  * a Rust `impl Trait for Type`, supertraits included;
  * a Go interface whose method set a type covers by name and arity;
  * a TypeScript `implements`/`extends` clause;
  * a Python base class.

  A fifth reaches through a value. A function is assigned to a name,
  `Held { run: candidate }`, and called through it, `(h.run)()`. Every such edge carries
  the tag `field-based`. `fr graph` counts it apart from resolved edges, and the report
  names it as the reason a symbol was spared. TypeScript also falls back to matching
  a method name alone where no `implements` is written, under the label `method-name`.

  A receiver the source settles narrows all five. An annotation, an initializer, a
  loop over a typed sequence, `self` and `this`: `held_by` works the type out with its
  evidence. The fan-out keeps only that type's kin. A settled member resolves in
  the index, and a stranger sharing the name gets no edge. A value called through a
  typed record reaches only that record's bindings. An edge the receiver itself
  licenses is labelled `receiver-type`. The tier stays `field-based`: which body runs is still
  the program's choice, and an inferred type is evidence, never a rewrite licence.

  What remains is what no evidence settles. A `dyn` object or interface-typed value
  arriving from outside. An untyped parameter, or one assigned two types. A type
  the workspace never declares, and a generic parameter. And what remains undecidable is
  undecidable from the source: a function this workspace never names.
  Either a caller outside it supplies the value, or the name is assembled at runtime from
  pieces no string literal spells. A symbol used only from a file that failed to parse is
  invisible for a third reason. `delete::plan` reports that file as possibly hiding
  uses. Zig (comptime duck typing) and Bash declare no implements-relationship at all, so
  neither has a hierarchy to read.

## Fixed

- [x] B747: **two bindings assigned from each other overflowed the stack.** The
  derivation chain in `analysis/types.rs` counted its hops, but every route back into
  a symbol's answer restarted the count at zero: `x = y` above `y = x` recursed until
  the process died. It surfaced the day the dispatch layer began asking about every
  receiver in the workspace. `of` now threads the depth through every route,
  `MAX_CHAIN` ends the loop, and the cycle answers nothing instead of aborting.
  Pinned in `tests/types.rs`.

- [x] B748: **two type answers picked by indexing order.** `resolve_in_workspace`
  answered with the first of several same-file candidates. The field arm of
  `infer_expression` answered with the first same-named field anywhere in the
  workspace. Both are the shape this file's own comments ban: an answer picked by
  indexing order is not an answer. Both are unique-or-none now, and where several
  candidates share the name, the receiver's own type picks or nothing does. Pinned in
  `tests/types.rs`.

- [x] B749: **whole-workspace questions re-did whole-workspace work per ask.**
  Four spellings of one shape. The type analysis re-read and re-parsed a file at
  every derivation hop. `fr usages` re-scanned the hierarchy, a parse of every
  family file, once per symbol asked about. `fr impact` rebuilt the call graph per
  analysis. `Index::find_symbols` scanned every symbol despite the name buckets
  built for that scan. An index now carries a generation number, the parses,
  scans, graphs and receiver answers are cached against it, and `find_symbols` reads
  the buckets. The repository-sized test file dropped from minutes per question to
  one build per index.

- [x] B750: **the namesake widening ate the receiver's answer.** `find_unused`
  widens any name matched at field-based confidence to every same-named symbol.
  The resolver's pick of the nearer twin is a guess that must not kill the
  other twin. A member picked because the receiver's settled type owns it is not a
  guess. Widening it marked every same-named stranger live again, which would
  have undone the narrowing the same week it landed. Receiver-picked members keep
  their definition group and widen no further. Pinned in
  `tests/hierarchy_reachability.rs`.

- [x] B738: **normalize walked half the tree.** `map_expr` recursed into
  calls, fields, binaries and templates and stopped there. A tuple, list, map,
  variant, record, keyword, cast, ternary, coalesce, lambda or comprehension
  hid its insides. Nothing inside one was ever normalized. Zig format arguments inside `.{ … }` reached the writers
  raw and Rust printed `::identifier` where a tag belonged. Every composite
  expression walks now.

- [x] B739: **the Zig reader dropped every null comparison.** tree-sitter-zig
  spells `null` as a keyword, not a named node, so `x != null` reached the binary
  arm with one operand and carried whole, along with every `if` that tested one.
  The keyword side now reads as the null it is.

- [x] B740: **`!x` in value position read as a type.** The grammar parses a
  bang before an expression as an error-union type. The reader believed it.
  So `if (!is_ok)` carried whole everywhere it appeared. A one-operand error-union
  node is the negation it looks like.

- [x] B741: **Go's `struct{}` did not survive the round trip.** The go writer
  spells a unit parameter `struct{}` and the reader read that back as a foreign
  named type, so a zig `_: void` parameter changed type crossing go and back.
  `struct{}` reads as the unit it spells.

- [x] B742: **Rust destructured a hole mutably.** A tuple binding with `_` in it
  wrote `let (mut _, mut loc) = …`. rustc refuses that: `mut` must be followed
  by a named binding. The hole takes no `mut`.

- [x] B743: **a Python branch of only comments had no body.** Comments are not
  statements in Python. An `else:` whose lowered body was one comment emitted no
  `pass` and the file stopped compiling. Comment-only branches count as unwritten
  and take their `pass`.

- [x] B744: **the Zig writer returned `anytype`.** `anytype` is a parameter's
  word; in return position `zig ast-check` refuses it. An inferred-nothing return
  spells `@TypeOf(undefined)` and stays counted as unannotated.

- [x] B745: **Java overloads collided in a Zig container.** Four constructors
  named `JsonPrimitive` became four container members spelled alike, which Zig
  refuses. Later overloads take a numbered name, noted once in the output.

- [x] B746: **a labeled break's flag test escaped its loop.** Lowering
  `break :blk v` through an intervening loop plants a flag. The flag is tested
  after the loop. But when the run-once wrapper itself was scanned as "an
  intervening loop", the test landed outside any loop at all. Python refused
  the bare `break`. The wrapper's own breaks settle before it goes on.

- [x] B735: **a Rust module alias resolved to nothing.** `use a::{b, c as d};` recorded
  only `b`. The aliased entry in a use-group was not among the query's shapes, so `d::f()`
  resolved to nothing, `fr unused` called `f` dead, and `fr delete` planned a deletion
  that would not compile, warning about "unresolved occurrences" it should have counted
  as callers. Running that deletion as a recipe against this repository found it.
  `provenance::applies_to` is called from `tests/provenance.rs` through such an import,
  and the tool reported the function unused.

  Two rules were missing. The use-group's aliased entry now records `local <- original`
  the way every other language's aliases do, at the top level and one group down. And
  where several files share a stem, the segments before it now decide between them.
  `analysis.provenance` names the file under `analysis/`, and a `tests/provenance.rs`
  only shares the name. Any ambiguous stem used to resolve to nothing at all.
  The recipe now refuses the deletion, naming both call sites.

- [x] B736: **the scaffolder re-cased wire names.** A schema property, `petName`, is the
  JSON key every request carries. The generated Pydantic model spelled it `pet_name`,
  and that model serialises a different contract than the document it came from. The generated TypeScript interface camel-cased fields the same way, and a query
  parameter's spelling changed the URL key FastAPI accepts. Path parameters were the
  only names that are internal, and they were also the only ones it was right about.

  Names are kept as the document spells them now. A name Python cannot declare is left
  out of the model and reported, which shrinks the contract out loud instead of serving
  a different one. TypeScript can quote any key, so nothing is left out there.

- [x] B737: **a bare `return` translated into a handler that answers nothing.** FastAPI
  serialises `return` as a JSON null with status 200. The Next.js handler it became just
  returned, which is a handler that resolves with no `Response` at all. A bare return
  now answers `Response.json(null)`, the same body either way.

- [x] B734: **the browser build freed memory with the wrong allocator.** The grammars' C
  has no libc on `wasm32-unknown-unknown`, so `crates/wasm-libc` supplies one.
  tree-sitter's core is pointed at Rust's allocator, through the C hook alone. The Rust
  binding keeps its own copy of the free function, and uses it for every buffer the C side
  hands back. That copy still pointed at the scanners' bump arena.

  So a string the parser allocated and the binding freed crossed two heaps.
  `Node::to_sexp` is the call that does it. One use of it in `restructure` reached the
  playground sweep as `assertion failed: psize >= size + min_overhead`, raised by the
  arena's own `free`. The allocator goes through `tree_sitter::set_allocator` now, which
  sets both halves of the pairing.

  No `cargo test` can see this. `src/wasm.rs` compiles only for wasm32, and the sweep that
  caught it needs a wasm toolchain and a Node run.

- [x] B733: **`fr` could not write the changes it is made of.** A pattern had to
  be an expression, a statement or an item. A variant of an enum, a field of a
  struct, an arm of a match and the pattern on its left are none of those. Adding a
  language to this tool needs all four, and each was refused the same way: `'Scss,' is
  not valid rust; check for unbalanced brackets.`

  A member is written with the separator that puts it in its list. Most grammars leave
  that separator out of the member's own node. The fragment reached one byte past the node
  that held it, and every wrapper was rejected. Members have wrappers of their own
  now, in Rust, Go, TypeScript, Java, Python and Zig. They cover an enum's variants, a
  struct's fields, a switch's cases, an object's properties and a match's arms. A match
  takes the target's separator with it, so rewriting `Scss,` as two variants leaves two
  commas rather than three. A trailing separator is optional after the last member, and its
  absence matches too.

  A second refusal sat behind the first. `A | B` is a bitwise or and an or-pattern, and
  the wrapper that parsed first decided which. It picked the expression, and the arm
  written `Language::Css | Language::Scss =>` matched nothing. Every shape that parses now
  searches, and the first to match a node anywhere is the one the caller wrote.

  Rust macros were the third. No grammar knows what a macro does with its arguments, so
  `matches!(l, A | B)` holds a flat run of tokens where the source holds an or-pattern.
  This source is 1876 `format!` calls and 378 `matches!` calls deep, so a tool blind to
  macro bodies is blind to much of it. A pattern's own tokens are compared against runs of
  macro tokens, counting brackets. A metavariable binds `item.name()` whole rather than
  stopping at the comma inside it. A shape that matched a node wins over one that matched
  only tokens, since every shape of a pattern has the same tokens.

  One more thing came out of it. A node can carry the whitespace that follows it, and Go's
  switch case runs to the line the next case starts on. Rewriting that far pulled the
  closing brace onto the last line of the case. A match now stops at the last byte that is
  not whitespace. `tests/restructure_members.rs` holds all of it, one member kind at a
  time, each rewrite put through the same reparse gate the CLI uses.

- [x] B731: **a stylesheet variable crossed no file boundary.** One declared in
  `_theme.scss` and used in `style.scss` reached nothing. Sass splits a codebase into
  files and gives each one a namespace, and none of that was read. `@use "theme" as t` bound no name and `t.$brand` resolved to nothing.
  `fr rename` rewrote the declaration and reported every use site as an occurrence it
  could not place. Both syntaxes, and the same for a mixin and a function reached the same
  way.

  What the module system makes visible now resolves, and what it does not stays
  unresolved. `@use "theme"` binds the namespace `theme`, `as t` renames it, and a name
  reached through either is import-qualified. `@use ... as *` and the older `@import` bind
  every name with no namespace at all, which is a glob import. A bare `$brand` reaches the
  file through one of those. Under a plain `@use` it still reaches nothing, because that
  is an undefined variable in Sass, and a guess would be worse than the report.
  `@forward` hands a name on, so a namespace reaches through the file that forwards it.

  Three things stood in the way. A partial is written `_theme.scss` and named `"theme"`,
  so no import ever spelled the file it named. The resolver puts the underscore back and
  tries each stylesheet extension. The braced grammar's plain-value token matched
  `theme.$brand` and won the tie against the variable token. So the use site was not a
  variable at all. The variable token outranks it now. And the namespace had to reach
  resolution as the receiver. The indented syntax gives it a node of its own; the braced
  one writes it into the same token. So the name is read from after the last dot, and a
  rename rewrites that and leaves the namespace alone.

- [x] B732: **the fact cache did not notice a grammar change.** Entries are keyed by the
  file's bytes and the query set. The namespace they live in is fingerprinted from the
  sources that decide what a fact means, and the grammars were not among them. So a
  patched grammar changed what the tree looks like, and every file already scanned kept
  the answer from the old one. Raising one token's precedence moved `theme.$brand` from a
  plain value to a variable. The cache went on reporting a file with no variable in it. That reads like a fix that did not work. `build.rs` hashes what each grammar
  is generated from. A change moves every entry to a new namespace, and the stale ones are
  never looked up again.

- [x] B283: **the indented Sass syntax, which is a language of its own.** `.sass` mapped
  to `Language::Scss` and could not be parsed. The SCSS grammar reads the braced syntax
  only, and the two differ in how every block and every statement ends. So this needed a
  grammar and not a rule.

  `grammars/sass` carries one, from `bajrangCoder/tree-sitter-sass`. Nothing upstream is
  released, so the pin is a commit, under an archive checksum. Six forms of
  ordinary Sass failed it. The CSS colour, gradient and maths functions had name tokens
  that outranked the identifier token, so `transition: color 0.2s` failed and so did
  `color.adjust(…)`. A call took no named argument. A list in parentheses read as a value
  in brackets. A selector list could not be written down the page. A hyphen could not join
  two interpolations. And `li + li` lost its left-hand selector to the descendant
  combinator, which is a run of spaces and is now the scanner's to decide.

  `.sass` is `Language::Sass` now, with `queries/sass/facts.scm` reading its nodes into
  the same kinds and the same spellings `queries/scss/facts.scm` uses. A name declared in
  one syntax and used in the other resolves across the two. The matrix has a column for
  it: 13 capabilities of the 24, against SCSS's 14. The one that differs is `fr translate`,
  which rewrites a file only where one grammar contains another, and neither syntax
  contains the other.

  Measured over `iv-org/invidious`, `peer-calls/peer-calls`, `HBM/jet` and the grammar's
  own examples, 17 files: the published grammar fails on 8 and this one on 1. That one is
  a Jekyll asset whose first line is YAML front matter. Two defects in the braced syntax
  turned up while writing the indented one, and both are fixed for both. `fr extract`
  wrote a new `$variable` above the declarations it reads, and it wrote a `@mixin` above
  the `@use` rules. Sass rejects each.

- [x] B11: **the Sass a stylesheet is written in.** `tree-sitter-scss` 1.0.0
  failed on 203 of the 276 stylesheets in `twbs/bootstrap` and `jgthms/bulma`. Its gaps
  ran from `$m: (a: 1)` and `!default` to `@use "x" as t`, and this entry listed seven of
  them; measuring against the two corpora found twenty more, among them a variadic
  parameter, a named argument over two lines, `:nth-child(n + 3)`, an escape in a name,
  and `@container`. `grammars/scss` reads all of them, and fails on none of the 276 files.

  Three parts of the patch are worth naming. A colon opens a pseudo class when a `{`
  follows before the statement ends. The scanner took the brace of an interpolation for
  that one, so `color: #{$v}` read as a selector. It now reads past an interpolation.

  A map is told from a list by a colon of the bracket's own, which is further ahead than
  any rule can see. So the scanner looks for it and hands the parser a different bracket.
  That is the lookahead Sass itself does.

  `%` is a unit glued to a number and the modulo operator when it stands apart. `-` is a
  sign glued to a variable and subtraction when it stands apart. Both are rules about
  spacing, so both belong to the token.

  The masking B280 added is gone with the gap. The parser reads the declaration, so
  nothing is filled in and nothing has to be read back. Checked over the 73 files the
  published grammar reads cleanly, 5,068 nodes: one tree differs. It is `$return: ()`,
  where the published grammar invents a zero-width `integer_value` and this one reads the
  empty list that is written.

- [x] B15: **a Go package that defines its own `new`.** `new` and `make` are predeclared
  identifiers in Go and not keywords. A package may define a function called `new` and
  call it. `tree-sitter-go` 0.25.0 gave the name one argument list, the special one whose
  first argument is a type, so `new("-10s")` and `new(err.Error())` were error nodes.
  They account for 177 of the 178 Go files that fail to parse in `grafana/grafana`.

  `grammars/go` lets either argument list follow the name. The one taking a type has the
  higher dynamic precedence, so every call the published parser reads keeps the tree it
  had. Checked over `spf13/cobra`, `gin-gonic/gin`, `sirupsen/logrus` and the grammar's
  own examples: 181 files, 281,884 nodes, identical throughout. The grammar's own corpus
  passes 67 of 67.

- [x] B231, B232: **an import type, and a member called `in`.** Both are ordinary
  TypeScript and both were found in `vuejs/core`. `import("@babel/types").Statement` was a
  whole `type` and nothing smaller, so it took no `[]` and no type arguments. A member
  called `in` ended the interface it sat in. The scanner never ends a line before `in`,
  which is an operator in an expression.

  `grammars/typescript` moves the import-type forms to `primary_type`, which an array
  type and a generic type are built from. `generic_type` takes one as a name.
  In a type there is no `in` and no `instanceof` operator, so a line opening with either
  ends the member before it. An identifier that only begins with one, `in2`, ends a
  statement in an expression too. The published scanner got that wrong as well. Checked
  over `vuejs/core` and `excalidraw/excalidraw`, 476 TypeScript files and 199 TSX files,
  812,690 nodes: identical trees, plus the two files the published grammar cannot read.

  Corrected: an earlier attempt on these two claimed a fix that regeneration alone had not
  made. The check behind that claim was wrong twice over. It used a repro typed with semicolons
  the entry does not have. And it took an error count from output where the parser had
  failed to load. The vendored copy was reverted rather than shipped.

- [x] B13: **the rest of the Helm `--set` family.** An answer about supplied values is
  only as complete as the description the caller gives. Four edges were missing from that
  description.

  `--set ports[0].name` and `--set ports[1].name` addressed the same key path and were
  ranked against each other. Each element now holds a competition of its own, and neither
  overrides the other. `--set x=null` removes a key in Helm and ranked as a source that
  supplies it. It is now reported as removing it. `--set-file` and `--set-json` were
  refused by name and are now read. The JSON is expanded to one assignment per leaf, so
  the keys beneath it rank like any other. And `{a,b}` was refused. It is the list
  `key[0]=a,key[1]=b`, which is how Helm reads it.

  The order between two different flags is still not recoverable from the flag lists. So
  two assignments to one path under different flags are refused, as they always were.

- [x] B233, B234: **valid Python the grammar could not read.** A starred
  element in an unparenthesised tuple failed unless it was a name:
  `g = 1, *rest` parsed and `g = 1, *[2]` did not, because the grammar
  reads a starred element there as a *pattern*, which takes a name, a
  subscript or an attribute. A type parameter could carry no default, so
  `type A[T = int] = float` failed, which PEP 696 added in Python 3.13.
  `grammars/python` gives `expression_list` the choice Python's own
  star_expressions has, and gives each type parameter an optional
  `= type`. Over 102 Python files and 50,399 nodes, the patched parser
  returns the tree the stock one returns. The five forms that used to fail
  now parse.

- [x] B133: **an empty Zig container came back holding a field that is not
  written.** `const Foo = struct {};` is ordinary Zig. Its four container
  rules take `_container_members`, which needs at least one member, while
  `source_file` takes `optional($._container_members)` and reads an empty
  file. The published 1.1.2 is the newest release and master carries the
  same rule, so this project compiles its own copy. `grammars/zig` holds
  the upstream source, its licence, the four-line patch and the provenance
  to check them.

  Corrected: this entry used to say the grammar *rejected* the form. It
  does not. It fills the gap with a `container_field` whose name is zero
  bytes long, and reports no error. That is the worse of the two: a member
  no line of the file declares, with nothing to say it was invented. The
  patch is additive over `zls`, 77 files and 231,518 nodes. The only trees
  that change are the ones holding that phantom. The Zig writer no longer
  pads an empty record with a `comptime {}` block to get past its own
  check.

- [x] B14: **a class name reached the markup three ways and a rename
  rewrote one.** `className="btn"` resolved to the CSS selector, while
  `cx("btn", active && "on")` and `` `btn ${size}` `` did not, so a
  rename rewrote the plain attribute and reported the rest. Across
  grafana/grafana's 4,400 TSX files the helper form outnumbers the plain
  one, 381 to 224. The queries capture both now: a string handed to
  `cx`, `clsx`, `classnames`, `classNames`, `cva`, `twMerge` or
  `twJoin` is a class reference, under a condition as well as beside
  one, and so is every literal part of a template literal in a class
  attribute. CSS-in-JS `styles.x` is a different subject, because no
  stylesheet selector stands behind it.

- [x] B730: **the comments carried banners, history and hedging.** 277
  rules of dashes separated sections in the source, and 86 more in the
  site scripts, the tooling and the tree-sitter queries. Comment bodies
  told the story of the defect behind them, in the past tense, with an
  opinion about how bad it had been. The petstore fixtures the contract
  page renders carried doc comments repeating what the code did, and one
  repeating the page's own note almost word for word. All of it went.
  `docs/style.md` now says a comment is timeless, gives the reason
  rather than the behaviour, and states what is true rather than what is
  absent.

- [x] B727: **the contract page called a creation patch a diff.** Every
  endpoint on `contract.html` showed a Diff pane taken from the
  translate report. A translation writes a file beside the one it read,
  so that patch adds every line and removes none. The pane repeated the
  FastAPI pane next to it. The page now diffs the route against the
  file it became, so a reader sees which block of TypeScript each block
  of Python answers. `edit::unified_diff_between` renders it, because
  the two sides carry different names. The generator asserts that each
  diff has both signs, which is the check the old pane would have failed.

- [x] B728: **four capability drivers could not fail.** The coverage test
  says a cell nothing drives is a claim nothing checks. Four of its
  drivers read nothing back. The restructure driver passed a pattern
  chosen to match nothing and dropped the plan. The call-graph driver
  dropped the graph, so an empty one passed. The entry-point driver
  dropped a `Result`, so a language whose detection had failed reported
  success. The dead-code driver dropped the list of unused symbols. Each
  one reads its answer now. The pattern that matches nothing must
  produce no edits. A graph over a fixture whose `caller` calls `width`
  must have an edge. Detection must succeed, and the unused list must
  name only symbols the index holds.

- [x] B726: **the project wrote one way and its tutorial wrote another.**
  The type-safety page had learned to put a doer in every subject. It
  gave an instruction wherever a reader can act. The rest of the repository still
  reported arrangements, hid its actors, and reached for the phrases a
  language model reaches for. This pass carried the doctrine across
  everything. It covered nine documents, from the README to the glossary,
  nine site pages, and the comment bodies of the source. `docs/style.md`
  gained the doctrine, the sentence rhythms to ration, and the phrase
  list. Long sentences fell from 947 to 484 and em-dashes from 52 to 34.
  Self-reference fell from 50 to 34 and false comparisons from 19 to 13.
  `tools/PROSE-DEBT` records every new number.

- [x] B725: **the tutorial described its own lessons instead of giving
  them.** Every verdict paragraph reported an arrangement.
  "With `Pence` and `Rate` as types of their own, that call can't exist
  any more" has nobody acting in it. A sentence like that reads as
  passive however the grammar parses. The page now tells the reader what to do and what comes
  back. 56 of its 221 sentences now open with an instruction, and one
  sentence pairs a form of "to be" with a participle. The stative tic
  ran through eleven paragraphs and is gone. Two examples went with it. The
  filter-and-map cell taught an idiom rather than a type, and the
  pipeline cell repeated the parsing lesson.
  Sections 6 and 7 merged into one border section, so the count of
  numbered sections fell from twelve to eleven.

- [x] B724: **the tutorial read like a document with nobody in it.** Its
  prose had no people, so its sentences had no subjects. The same
  rhetorical beat closed one sentence in ten, and 25 of 242 ended on a
  negation. The books now belong to Albert Hargreaves, and the mistakes to
  his clerk Ernest. Ernest typed `darft` for draft, and swapped the two
  ids in `bill`. Each lesson states what can no longer exist, rather than what
  the checker objects to. The reader does the work now, and the checker
  only enforces it. Each unit keeps its three parts apart: the
  problem above the cell, the cell as the answer, the conclusion and the
  run-time cost below. Every claim about an example was checked against
  the file, and six paragraphs that described the wrong example got
  rewritten. `docs/style.md` carries the doctrine and the phrase list.

- [x] B723: **the capability log tore under concurrent writers.** `record`
  wrote its line with `writeln!`, which may split one line across several
  write calls. A dozen test processes append to the same log, two halves
  interleaved, and both lines died. The coverage report then blamed an
  innocent cell for having no driver, once in dozens of runs. The line now
  goes out in a single `write_all`, whole under `O_APPEND`. The report
  script also skipped malformed lines in silence, which had hidden the
  tear; now it fails and prints them.

- [x] B722: **the tutorial wrote like a document, and people talk.** The
  page still hid its doers. A status "was misspelled", a string "became"
  an `EmailAddress` by itself, and "there is no server" had nobody in it.
  Every such spot now names the actor: whoever wrote `advance` typed
  `darft`, only `parse_email` builds an `EmailAddress`, and the tab is
  the whole machine. The body text contracts the way speech does,
  five-item lists stand as bullets, and the paragraph that explained the
  epigraph's joke is gone. The widgets grew with the prose: softer cards,
  a leading check button, zebra tables in their own scroll box, a
  two-column contents list.

- [x] B721: **the type-safety tutorial disclaimed its own examples.** Its
  monads section called hand-rolled monads friction and said only `Result`
  earns its keep. It then demonstrated a Writer and an IO anyway. The literal-flag
  block repeated the status-literal lesson, and the alias-as-names block
  taught what the section had already said. All four went, with their
  before-and-misuse files: twelve examples, twenty-four files. The first
  example now carries no annotations at all. Both checkers refuse it for
  that alone, which is where the tutorial's journey starts.
  The prose lost its passive voice on the way.

- [x] B719: **the parallel build compiled the query set once per file.** The
  comment beside it said the compilation was paid once per thread. The code
  built the parsers and the extractor inside the per-file closure. A
  thread-local makes the comment true; the source build joined the same
  parallel path. Cold indexing of this repository fell from 50 to 24 seconds,
  on top of the last pass's gains.

- [x] B720: **every warm command re-resolved the workspace.** Resolution is
  a pure function of the merged facts. It was most of a warm run. An
  agent issuing ten commands paid seventeen seconds ten times. The
  resolution is a cache entry now, keyed by every file's path, language and
  content hash. Any change anywhere resolves afresh; an untouched workspace
  answers in a fifth of a second.

- [x] B715: **inlining a wrapped binding left a line of whitespace.** The
  removal compared only the first line. A `let` wrapped over several lines
  was cut from its keyword to its `;`, and the first line's indentation stayed
  behind as trailing whitespace. The whole lines go when nothing else sits on
  them.

- [x] B716: **`guard-clause` negated one atom and called it the condition.**
  `!path.is_empty() && !seen` guarded a push. The double-negative rule
  stripped the leading `!`, so the guard became `path.is_empty() && !seen`.
  Every duplicate went through silently, on this repository's own
  `values_paths`. The `!` covers one atom. A top-level `and`/`or` means the
  negation goes round the outside.

- [x] B717: **a two-step recipe over this repository never finished.** The
  engine rebuilt the whole index from scratch after every step. That is
  extraction of every file, sequentially, uncached, at minutes apiece. Extraction is
  per-file and depends only on the file's bytes. Unchanged files now cost a
  lookup, and the run finishes in the time its steps take.

- [x] B718: **resolution spent two minutes scanning the workspace.**
  `definition_group` walked every symbol per candidate. The dotted-import
  arm walked every file key per reference, and `names_a_type` every symbol
  per receiver. All three go through by-name buckets now, and the
  token-tree walk added in the last pass runs only for Rust, behind a cheap
  text check. Indexing this repository fell from 162 to 50 seconds.

- [x] B711: **a module docstring crossed as raw prose.** The Python reader
  stored the whole docstring as one entry. Every writer puts its comment
  marker in front of each entry, so the marker covered the first line and the
  rest landed bare. The parse gate caught it and refused the write. One entry
  per line fixes the class for every target at once.

- [x] B712: **a constant's type was decoration.** With no annotation, Rust's
  writer typed every constant `&str`: `RETRY_LIMIT: &str = 3` refused to
  build. A literal says its own type now, and a list of literals becomes a
  slice of one.

- [x] B713: **a list constant lost its case and its buildability.** A list
  of literals was spelt as a value. `NAMES` came out `names`, and its
  `vec![…]` never evaluates in a `const`. A list of literals is as constant
  as a scalar. Rust writes `&[…]`; Go writes a `var`, its `const` holding
  scalars and nothing else. A run-time value keeps its draft declaration:
  dropping it to a comment lost the entity on every round trip.

- [x] B714: **pathlib's `/` became float division.** `ROOT / "tools"` reached
  the true-division repair. It came out `float64(Root) / float64("tools")`,
  which is not a number and was never a division. A `/` with a string operand
  is no arithmetic; the draft carries it marked.

- [x] B706: **a path written inside a macro resolved name-only.**
  `assert_eq!(fun_refactor::model::anchor_slug(x), y)` spells the whole path.
  A macro body is tokens, so no rule read it. The reference fell to the
  weakest tier, and `fr signature` refused the change. The tokens are walked
  now, and the path becomes the receiver. A trailing module segment resolves
  to the file it names, the way the module tree names files.

- [x] B707: **`fr signature` refused every call written inside a macro.**
  Half of a crate's call sites live in its tests' `assert_eq!`. The command
  was unusable on real Rust. The argument tokens have the shape of a call,
  and the token tree's top-level commas split them exactly. The change
  rewrites them like any other site.

- [x] B708: **an extraction missed what a format string captures.**
  `println!("{total} file(s)")` reads `total`. No reference records it,
  because the identifier is string content. The extracted function did not
  compile while the command reported success. A capture is a read now, and
  travels as a parameter.

- [x] B709: **a move left a written path naming the module the symbol left.**
  `crate::refactor::delete::deletion_span(…)` kept its old spelling. A fresh
  `use` landed beside it unused, and the crate did not compile. A
  written path is repointed in its own bytes, and such a file gets no `use`
  it does not need.

- [x] B710: **a move carried a `use` the destination already had.** The moved
  code needed `full_line_span`; the destination imported it in a brace group.
  The carried single-name `use` was E0252, twice defined. What the
  destination already binds is not carried, at either of the two writers that
  carry imports.

- [x] B705: **`fr refs` could not predict what `fr rename` rewrites.** The
  tiers alone under-answer. A field-based `s.pending` whose receiver is
  declared `*BatchSink` rewrites too, and only the rename logic knew it. So
  the playground's fidelity sweep and any agent reading `--json` guessed low.
  Every reference now carries `rewritable`, computed from the rename's own
  plan, and the sweep checks the writes against the tool's own claim.

- [x] B704: **removing a guarded import would have stranded its attribute.**
  B702 let liveness reach imports the old caution always held. The first one
  it removed was `#[cfg(feature = "cli")] use crate::scan::S;`, leaving the
  attribute above whatever came next. The deeper truth: a guarded
  import's liveness depends on the configuration, and this index reads one
  tree. A guarded import is held back, with that reason.

- [x] B701: **`fr delete` left the dead function's docs and attributes.**
  Each of this pass's four tool-made deletions left a `///` block behind.
  clippy refuses an orphaned doc comment outright. Worse, the
  `#[allow(dead_code)]` above one of them moved onto the next survivor, which
  changes what the compiler checks. What is attached above a deletion goes
  with it: doc comments, attributes, and a closing `/** ... */` block. Plain
  `//` and `#` comments stay, as the tests have always pinned.

- [x] B702: **the trait caution held imports the workspace can rule on.**
  `use crate::model::Confidence` stayed behind a deletion of its last user.
  The reason given: any capitalised Rust name may be a trait. An enum this workspace
  declares is on record as not one. The caution now asks the index and stays
  only for the names it cannot see. The orphan pass consults the whole
  workspace; its liveness answer stays with the reindexed file.

- [x] B703: **two meanings on one span doubled ordinary references.** The
  shorthand fix first kept a field and a value reading of *every* span. A
  Python keyword argument counted twice, and `fr usages` disagreed with the
  index's own reference count. The second meaning arrives marked as a twin
  now, and only a genuine shorthand carries one.

- [x] B690: **`fr unused` buried the dead code under Markdown headings.** On
  this repository, 202 of the report's 445 lines were headings. Most headings
  are never linked to, so "nothing links here" is true of nearly all of them
  and says nothing. A heading is spared as prose structure, with the reason
  written down.

- [x] B691: **a comma-separated `data-*` value was one symbol.** `data-quiz=
  "a,b,c"` names three hooks a script reads one at a time. The index stored it
  as one name containing commas. No part of it could ever match a use, and
  all three read as dead. Values fan out on commas as on spaces, in
  definitions as in references.

- [x] B692: **an enum variant used from another file read as dead.** Rust
  variants were captured as unqualified fields. So a cross-file
  `Shape::Square(side)` resolved to nothing. This repository's own `Stmt::Let`,
  matched seventeen times in one writer, was listed for deletion. A variant is
  a constant qualified by its enum now, the way Java's constants already were,
  and a variant rename reaches every match arm.

- [x] B693: **`fr symbols <file>` was a usage error.** Every sibling listing
  command takes positional paths. The one that answers "what is in this file"
  did not. It does now, and the whole workspace is still indexed, keeping the
  cross-file answers right.

- [x] B694: **a destructuring pattern read no fields.** `Stmt::ForEach {
  iterable, .. }` is how a writer consumes that field, and it was no reference
  at all. Struct-literal writes were missing too: `Facts { named: 1, shorthand
  }` kept neither field alive. Both are references now, and weak evidence
  spreads across same-named twins and definition groups so the resolver's
  guess cannot make the unchosen twin dead.

- [x] B695: **a serde-renamed variant read as dead.** `ThreatModel::Remote` is
  constructed from a catalog writing `remote`. The string-literal spare
  compared spellings verbatim, so every data-constructed variant was offered
  for deletion. Case and separators drop on both sides now. YAML's quoted
  scalars joined the comparison: `"remote"` is a `double_quote_scalar`, which
  the string gate did not recognise, so quoting a value hid it.

- [x] B696: **renaming through a shorthand corrupted the file.** Renaming the
  `count` local of `Facts { count }` produced `Facts { total }`. That
  initialises a field the struct does not have. The shorthand expands instead,
  in the direction the reference's kind dictates: renaming the local writes
  `count: total`, renaming the field writes `size: count`. TypeScript's
  `{ count }` had the same defect and takes the same fix.

- [x] B697: **a field rename left `f.count` behind, blaming its own type.**
  A struct was no container, so its fields had no owner. The declared-receiver
  rule refused `f.count` on an `&Facts` receiver, with a reason naming the very
  type being renamed. Structs qualify their fields now,
  and a declared type sheds its sigils: `&Facts`, `*Buffer` and `?Handle`
  reach what the bare names do.

- [x] B698: **a local answered calls outside its scope.** Scopes were the
  function's *body*. So parameters, declared before the block opens, spilled
  into the enclosing module. A call to `fn stmt` resolved to a sibling
  function's `stmt` parameter whenever the parameter sat nearer. `fr delete`
  then offered the live function for deletion. Rust, Python, TypeScript
  and Java all scoped only the body. All four scope the whole definition now,
  and a local below file scope is no candidate outside its own chain.

- [x] B699: **`fr delete` kept an orphaned import in silence.** Deleting a
  `use`'s only user leaves an import that trait caution rightly keeps. The
  command said nothing, so the surprise arrived from `-D warnings`. The report
  names the kept import up front, with the reason.

- [x] B700: **a foreign trait's impl read as dead.** `impl Deserialize for
  AppliesTo` is called by serde, `impl Display` by every `format!`. The
  callers live in another crate, so reachability can never see them. A method
  implementing a trait this workspace does not declare is spared, with the
  reason written down.

- [x] B666: **a hoisted Python definition landed with a method's spacing.**
  Fixing B660 moved an extracted definition to module scope. It had been
  written inside a class. `black` wants two blank lines in front of it and
  got one. Verified against `black --check`, which now leaves the output
  unchanged. A definition that stays a class member still takes one.

- [x] B636: **nothing completed anything.** Thirty-three subcommands and six
  global flags, and no shell knew any of it. `fr completions bash|zsh|fish`
  writes a script from the command tree itself. It offers what this binary has,
  and nothing else. A command added tomorrow
  is completed tomorrow.

- [x] B635: **a translated class could not be constructed.** `__init__` is
  how Python spells a public constructor. Its underscores were read as Python's
  mark for "internal", so Java produced a `private Account(...)` on a public
  class, and Rust a private `fn new`. Neither type could be built from
  outside the file that declared it, and nothing said so. A name wrapped in two
  underscores on each side is a protocol method the language itself calls. It
  is part of the surface. One leading underscore still means keep out.

- [x] B633: **Python's `/` crossed as C's `/`.** They are two operations
  sharing a spelling. Python's yields a float whatever it divides, and C's
  truncates two integers. Read as one, `self.cents / 100` became an
  integer division everywhere. Rust and Go refused the file. Java took it and
  answered 5 where the source answered 5.34, which is the worse of the two.
  True division is its own operator in the IR now. The targets whose `/` is
  C's coerce an operand before the operator sees it. The two whose `/` already
  divides in floats are left alone.

- [x] B634: **a translated method that wrote a field would not compile.** Rust
  took `&self` for every method. A body assigning to `self.cents` was refused
  with E0594: cannot assign behind a `&` reference. A body that writes a field
  takes `&mut self`. A body that only reads keeps the shared borrow.

- [x] B630: **an empty list crossed as `[]any` under a signature promising
  something else.** `out = []` says nothing about its elements. Go therefore
  wrote `out := []any{}` inside a function returning `[]Point`, and the compiler
  refused the return. What the body appends says what the list holds, and a
  declared return type says it where nothing is appended. The element type
  settles in the shared binding table, so every target that must name one
  benefits.

- [x] B631: **a function whose body did not translate would not compile.** The
  untranslated statements were carried as comments. That left a Go function
  promising a value and returning nothing: `missing return`. It now panics with
  the marker, so a draft says it is unfinished where a zero value would have
  said it was done.

- [x] B632: **every translated Go file was `package main`.** A file with no
  `func main` is a program with no entry point. `go build` refuses it, so every
  translated library was unbuildable. A module carrying an entry point is
  still `main`. Everything else takes its package name from the file.

- [x] B628: **an unindexed file was refused as if the cursor were wrong.**
  A `def` sat plainly on the line named. The answer was "no symbol or resolved
  reference". The file was excluded by .gitignore and never
  indexed, so nothing in it resolves at any position. The refusal now names the
  file as the reason. It also offers the likeliest cause it can check: a
  language this does not read, a size over the limit, or an ignore rule.

- [x] B629: **ignored files could not be reached at all.** The scan honoured
  .gitignore unconditionally and no flag turned that off. A generated tree, a
  vendored copy or build output was therefore outside every command, and B628's
  advice had no flag to name. `--no-ignore` reads ignored and hidden files.
  Every command takes it, because every command scans.

- [x] B626: **asked from a subdirectory, every command answered about that
  subdirectory.** The root defaulted to `.`. So `fr usages` run in `pkg/deep`
  reported "0 use(s)" of a function `main.py` calls. `fr delete` offered to
  remove it. `fr rename` renamed the definition and left the caller reading
  a name nothing declares. All three exited zero and reported success. That is
  the worst shape a wrong answer takes. A shell sits in a subdirectory far more
  often than at a repository root, and an agent's shell almost always does.
  Where `-C` is not stated, the root is now the nearest enclosing project. It is
  found by `.git`, `Cargo.toml`, `go.mod`, `package.json`, `pyproject.toml`,
  `setup.py`, `build.zig`, `pom.xml` or a Gradle build. Widening it is said out
  loud, and `-C .` still means this directory alone.

- [x] B627: **a path typed from a subdirectory was read against the root.**
  Fixing B626 exposed the other half: `-C` states which workspace to operate
  on, so relative paths resolved against it, and a caller standing in
  `pkg/deep` typing `h.py` was answered "does not exist". Where the root was
  found rather than stated, a typed path is now read from where the caller
  stands first, and against the root second. A stated `-C` keeps its meaning.

- [x] B625: **`fr scan` passed over files without saying so.** A directory of
  `.sql`, `.json` and a `Makefile` beside one Python file answered "1 file(s)"
  and nothing else, so a reader could not tell an unsupported tree from an
  empty one, and neither could an agent deciding whether `fr` was the right
  tool. Both the listing and the JSON now account for every file skipped for
  want of a language. Each is counted by extension, so a lock file does not bury
  the report. A `--languages` filter is not a gap in support, and is not
  counted.

- [x] B624: **`fr duplicates` measured a stylesheet against a function's
  floor.** One threshold of 60 tokens covered all sixteen languages. A
  stylesheet rule written twice comes to 47, so the command answered "no
  duplication" over eleven copied declarations. Where nothing is stated, the
  floor is the one each language class earns. Code gets 60; markup and
  configuration get 30, their lines being far fewer tokens each. `--min-tokens`
  still wins where it is given, and the report names the floor it used.
- [x] B660: **`fr extract --function` spliced a class in half.** The new definition
  went straight after the function it came from, at column zero. Extracting from a
  Python method that was not the last one put a `def` in the middle of the class
  body. Python parses that, because a `def` nests anywhere, so the reparse guard
  passed. Every method below became a closure of the new function, and the class
  lost them. The file still imported, and the tests that called those methods
  failed with `AttributeError`. A nested function got the same treatment, at the
  outer body's expense. Placement is one choke point now. The definition is hoisted
  out of every class it sits in, and stops at the first enclosing function. It takes
  the indentation of whatever it lands beside. Pinned in
  `tests/extract_function.rs`.

- [x] B661: **`fr extract --function` lost a method's receiver.** TypeScript reached
  it first. `this` is named nowhere in a signature, so the data-flow analysis could
  not see it. The body still read `this.values` while the parameter list was empty,
  and the definition landed inside the class body. The syntax guard caught the
  second half, so nothing was written, and the capability matrix went on claiming
  extract-function for typescript and tsx. Go had it right all along: its receiver
  is a named parameter, so `func (c *Cart) Subtotal()` yields `accumulate(c *Cart)`
  by ordinary means. TypeScript now carries the receiver the same way, as a
  parameter typed with the class it came out of, with the call passing `this`. A
  private or protected member, a generic class, an anonymous one and `super` are
  each refused by name. Rust and Java say the receiver cannot be handed over.
  Pinned in `tests/extract_function.rs`.

- [x] B662: **`fr move` did not carry a Go symbol's imports.** Moving a function
  that used `math` broke both files. The destination said `undefined: math`. The
  source said `"math" imported and not used`. Two compile errors from one move, in
  the commonest Go refactoring there is, and the report said `0 file(s) gained an
  import`. Both directions had been reported and neither done. A Go import path is
  absolute, so the statement travels verbatim. A qualified use is a reference under
  the package binding, so which import fed which name is a fact. References inside
  the moved span decide what goes and the ones outside decide what stays. Pinned in
  `tests/move_languages.rs`.

- [x] B663: **a relative import that crossed a directory resolved to nothing.** One
  path join short of normalised. `Index::resolve_import_path` joined the specifier
  onto the importer's directory and looked the result up as written.
  `"../src/pricing"` from `test/run.ts` became `test/../src/pricing`. That compared
  unequal to the file it names, because the index is keyed by paths with no `..` in
  them. Every caller asking which file an import points at got `None`. `fr move`
  then wrote its new import beside the old one it had failed to recognise: `TS2300:
  Duplicate identifier` and `TS2459`. Duplicate imports parse, so no guard caught
  it. The join is normalised at that one point. Pinned in `tests/move_imports.rs`.

- [x] B664: **a move left a blank-line scar.** Two declarations have a blank line on
  each side of the one between them. Erasing that one's lines alone left both, so
  the file kept a two-line gap where a one-line gap belongs. `gofmt` rewrites it. A
  symbol moved out and back came home to that scar. What "back" means is decided and
  pinned now. A move appends to the file it lands in and leaves no trace in the one
  it left. So out and back returns the package to the declarations it started with,
  in a different order. The emptied file returns to what it held. Holes whose blank
  runs meet are worked out as one, because two edits claiming the same blank line is
  a conflict the applier refuses. A declaration with nothing blank above it keeps
  the run below, which is the only separator left. Pinned in
  `tests/move_languages.rs` and the unit tests beside `erase_spans`.

- [x] B665: **half of a documented pairing was empty.** `fr inline` was called the
  reverse of `fr extract`. `extract --function` writes a body of several statements
  by construction. `inline --call` refuses a body of several statements by
  construction. So no output of the first is an input to the second, and nothing
  said so. Teaching `--call` the multi-statement case is a different piece of work
  to the one that would make the sentence true today. So the limit is stated
  instead, in EXAMPLES.md, TUTORIAL.md, the README synopsis and `fr inline --help`.
  The refusal states it too, and names the command whose output cannot come back.
  Pinned in `tests/inline_call.rs`.
- [x] B685: **a recipe run and its `--explain` disagreed on the recipe's length.**
  `--explain` read the file and said "3 step(s)". The run of the same file said
  "2 step(s)", because a refusal stopped it at the second and the header counted the
  steps reached. A recipe is a reviewable artifact, so its length is a fact about the
  file. The header reports that now, from `steps_in_recipe`, and how far the run got
  is its own line and its own JSON field. Pinned in `tests/recipe.rs` and
  `tests/cli.rs`, which compares the two commands.

- [x] B684: **`fr imports` kept an import and never said why.** The planner works out a
  reason for each one it holds back: a package `__init__.py` re-export, a `__future__`
  import, a submodule imported for its registration side effects, a Rust trait used
  through its methods. Each reason was built as a warning and then thrown away by the
  command, in text and in `--json` alike. So the user read "removed 0 import(s)" and
  had nowhere to go. The single-file report lists them now, and carries them as
  `kept_imports`. The workspace sweep prints the count and names the command that
  gives the reason. Pinned in `tests/cli.rs`.

- [x] B683: **`fr impact` left out what `fr rename` reports.** No resolver sees a
  name written as text. An `__all__` entry, a line of documentation, a CI script
  all hide one. `fr rename` sweeps for those and lists each one.
  The tool suggests `fr impact` before that rename, and it ran no such
  sweep. So it answered one site where the rename showed three. A reader met
  the rest after committing to the change. It runs the same sweep now, from
  `crate::mentions`, which is where `fr usages` and `fr delete` already ask.
  `tests/impact_completeness.rs` compares the two commands site by site.

- [x] B682: **`fr restructure` reported no matches as success.** `fr rename` exits 3
  for a target it cannot find. Restructure printed one line and exited 0.
  It is the operation where a typo is likeliest and least visible. A caller
  looping over rewrites read "your pattern was wrong" as "there is nothing left to
  do". It is a not-found now, in the exit code and in the `--json` error object. The
  matches a template could not be written over were prose on stdout in `--json` mode
  as well. They arrived in front of the report, so the output was not JSON, and they
  are `skipped_occurrences` in it now. Pinned in `tests/cli.rs`.

- [x] B681: **a read through a Python module object resolved to nothing.** `from app
  import flags` binds the submodule `app/flags.py`, and `flags.USE_NEW_TAX` reads it.
  The index took the import path for the whole answer, so the receiver named
  `app/__init__.py`, which declares nothing. `fr remove-flag` then refused with
  "nothing reads it" and sent the reader to `fr delete`, over a live declaration
  and a line the tool prints. `import app.flags` and `from . import flags` failed the
  same way. A receiver bound by an import can name the submodule as well as the
  package now, and relative module paths resolve. A refusal that finds no firm use
  lists the occurrences `fr rename` would show, instead of claiming there are none.
  Pinned in `tests/cascade.rs` and `tests/python_modules.rs`.

- [x] B680: **`fr remove-flag` rewrote an import statement.** Python puts a flag in
  its own module and imports it where it is read.
  `from app.flags import USE_NEW_TAX` became `from app.flags import True`, and
  the final parse gate threw the whole cascade away. So the command was unusable on
  that shape. TypeScript wrote `import { true }` in the same place and only survived
  because a later round deleted the mangled statement. An import binds a name and
  reads nothing, so no literal stands there. `use_site` now answers `Binds` for an
  occurrence under an import, in every language. The declaration still goes, because
  binding a name is not reading it, and the round that drops unused imports takes the
  statement away. Pinned in `tests/cascade.rs`.

- [x] B622: **a folded flag left code that cannot run.** `if FLAG { return a }
  return b` became `return a; return b`. It answers the same, and every
  compiler says so: `go vet` reports unreachable code, `rustc` warns, and Zig
  refuses the file outright. A branch that always leaves now takes the rest of
  its block with it, and a branch that falls through keeps what follows.
  Pinned in `tests/remove_flag_sweep.rs`.

- [x] B623: **`fr entrypoints` called a module's workings its inputs.** A
  `locals` block was reported as `infra-input` beside a real `variable`. An
  `output` was not an entry point at all. Since
  `fr unused` treats an entry point as reached, nothing in HCL or Helm was ever
  unreferenced, however plainly `fr usages` said otherwise. A variable is an
  input, an output is the surface, and a local is neither. Pinned in
  `tests/entrypoints_conventions.rs`.

- [x] B640: **a file that did not parse was invisible where it mattered.**
  `fr parse` and `fr duplicates` named it. So did `fr rename` and `fr extract`.
  `fr symbols`, `fr usages`, `fr unused` and `fr graph` said nothing. `fr unused`
  was the dangerous one. It listed deletion candidates read out of a file the
  grammar only half understood. A user acting on that deletes live code. The
  index already knew which files carried `FactGap::SyntaxErrors`.
  `Index::unparsed` hands them to the choke point that warns about a file skipped
  for its size. So every command that indexes says what it could not read. It
  says it on stderr, and as `unparsed_files` in its JSON. Pinned in
  `tests/json_surface.rs`.

- [x] B578: **a stalled install hung the gate.** The step installing Zig,
  Terraform and Helm reaches hosts nobody here controls. `apt` reaches a
  mirror. None of it set a deadline, so both check jobs sat in that step for
  an hour, runners idle and the log silent. Each fetch has a deadline and
  three retries now, and so does `apt`. The step and the jobs are bounded.
  A gate that cannot finish has to say so rather than wait until GitHub's own
  six-hour limit. One slow host left both
  check jobs in that step for forty-five minutes, runners idle and the log
  silent. Every fetch has a connect timeout, a deadline and three retries now.
  One that cannot finish says which host it waited for.

- [x] B581: **`fr delete` removed a Terraform module output that a caller still
  read.** `terraform validate` failed on the result. The refusal was already there
  for Helm values and CSS classes, and it reads the index. Nothing in the index
  said `module.net.subnet_id` reached that output, so there was nothing to refuse
  over. B580 put the edge in, and this refusal came with it. Pinned in
  `tests/namespaces.rs`.

- [x] B580: **`fr usages`, `fr refs` and `fr impact` omitted an edge `fr flow`
  reported.** A caller reads `module.net.subnet_id`. That counted as zero uses of
  the module's `output "subnet_id"`. `fr flow back` named the same read, from its
  own traversal code. Two things were missing from the facts. The
  third segment of the traversal recorded `module` as its receiver, which lost
  the module call the second segment names. And no symbol said which block-type
  keyword declared it, so an `output "x"` and a `provider "x"` beside it were the
  same thing to resolution. The keyword is now the symbol's qualifier, written
  once at extraction, and `Index::resolve_module_surface` resolves both halves of
  a module's call surface. `fr rename` rewrites the caller, and `hcl_role` reads
  the recorded keyword instead of re-reading the file. Pinned in
  `tests/namespaces.rs`.

- [x] B610: **`fr stitch` truncated a chain silently.** A chart with no
  `Chart.yaml` had its `templates/deployment.yaml` read as plain YAML. So the
  chain began at the manifest and looked whole. The values file feeding it went
  missing and unmentioned. A `templates/` file writing
  template actions is a chart template now, and the directory holding
  `templates/` is the chart boundary. Pinned in `tests/stitch_languages.rs`.

- [x] B609: **Markdown was invisible to the mention sweep.** The sweep looks for
  a string node or a comment node, and Markdown has neither. So a style guide
  writing `` `.btn-primary` `` in prose, and `class="btn-primary"` in an html
  fence, was walked past twice. `fr rename` rewrote the CSS, the HTML and the
  TSX, and listed neither Markdown site. A paragraph and a fence body count as
  text now. So `fr rename`, `fr delete`, `fr usages` and the flag cascade all
  report them. Pinned in `tests/cross_language.rs`.

- [x] B608: **a name nothing declares was reported as a typo.** `<a
  href="#section-two">` with no element carrying that id got "no symbol named
  'section-two'". The sites that write the name now ride with the message, so a
  link into nothing is visible from any command that takes a name.

- [x] B607: **HTML modelling stopped at element ids.** A hook like
  `data-testid="submit-btn"` is written twice. Once in the markup, once in the
  TSX that renders the same element. `fr usages submit-btn` answered "no symbol
  named" to it. A `data-*` value is a
  symbol now, of its own kind, with every site equal, as a CSS class is. So a
  rename of a test hook rewrites both files. Pinned in `tests/facts_html.rs`
  and `tests/cross_language.rs`.

- [x] B606: **a resolved call at file scope counted as unresolved.** A shell
  script's `deploy_app "prod"` sits outside any function. So the graph has no
  node for the caller. The callee resolved all the same. `fr graph`
  counted it under "unresolved calls", which said the tool could not resolve
  what `fr usages` resolved. These have their own count now, `file-scope
  calls`, in the summary and in the JSON. Pinned in `tests/graph_export.rs`.

- [x] B605: **`fr callers` answered nothing where it knows nothing.** SCSS has
  no call graph in the matrix. The command printed the name and exited
  0. A reader takes that for "nothing calls this" while `fr usages` lists two
  `@include` sites. It refuses now, with a reason that fits SCSS: the old one
  said the language has no functions, and `fr symbols` prints one. `fr graph`
  keeps its filter behaviour, since a whole-workspace answer covers many
  languages. Pinned in `tests/graph_export.rs`.

- [x] B604: **`fr signature` refusals exited 1.** `fr --help` promises 5 for a
  refactoring that refused to proceed. Three sites raised a considered refusal
  as a plain error. An HCL variable the module still reads, an SCSS mixin
  parameter its body still reads, and a shell positional. The exit code is
  chosen from the error's type, so each now raises the `Refusal::StillUsed`
  that `fr delete` raises. Pinned in `tests/cli.rs`.

- [x] B603: **`fr remove-flag` wrote `Flags.true`.** A use written as a member,
  `Flags.SHINY`, had its name replaced and its qualifier left standing. Java,
  Go, Python and TypeScript all read a constant that way. The reparse gate let
  it through, and `--write` put it on disk. The literal now stands for the
  whole qualified name. An import is left alone, since a later round drops it.
  Pinned in `tests/remove_flag_sweep.rs`.

- [x] B602: **`fr remove-flag` refused the name `fr symbols` prints.**
  `featureFlags::newCheckout` got "no symbol named" and exit 3. The bare leaf
  reached the right refusal and exit 5. `fr usages` and `fr delete` took the
  qualified spelling all along. One lookup, `Index::symbols_written`, now
  serves every command that takes a name. Pinned in
  `tests/remove_flag_sweep.rs`.

- [x] B601: **`fr extract` on YAML reported a replacement it never made.**
  Without `--all` it wrote the anchor `&g` and counted one replacement. Every
  occurrence stayed spelled out and nothing named the anchor. An anchor binds a
  name and an alias spends it, so the pair is the whole edit. A single-site
  extraction now refuses, and says how many other occurrences `--all` would
  alias. Pinned in `tests/config_extract_inline.rs`.

- [x] B600: **`fr move` took link definitions away.** They serve a whole
  Markdown document. A definition like `[api]: ./a.md` sits at the end of a
  file, under the last section. Moving that section carried the definition off,
  so reference links left behind resolved to nothing. The report said nothing
  about it. A definition now stays where it is, and the section is taken around
  it. Where the moved text uses one, a copy goes with it and a warning names
  the copy. Pinned in `tests/move_languages.rs`.

- [x] B576: **a repeated `signature add:` named one thing twice.** The grammar
  parses `def scale(v, factor, factor)`, so the syntax gate passed it. Python
  refused the file. Go answered `rate redeclared in
  this block`. Every other operation declines a repeat, so a retried command or
  a re-run recipe broke what it had just changed. A declaration that already
  has the name refuses. Pinned in `tests/signature_hierarchy.rs`.

- [x] B577: **a parameter's name was read from the wrong end.** Go writes
  `name type` and Java writes `type name`. One rule read both, so Go's `price
  float64` came back as a parameter called `float64`. Each
  language is read the way it writes. Pinned in
  `tests/signature_hierarchy.rs`.

- [x] B574: **every `--write` reset the file's mode to 0600.** A commit stages
  beside the target and renames over it. The target inherited the private mode
  a temporary file is given. An executable script stopped being executable, a
  git hook stopped running, and a file the group could read became one only
  its owner can. A repository-wide rename re-permissioned the
  repository. The staged file takes the mode of the file it replaces. Pinned
  in `src/edit.rs` tests.

- [x] B575: **a first import landed above the docstring and the shebang.** With
  no imports to sit after, the insertion point was byte zero. That is above
  everything. A `#!` line moved to line two, so the script stopped running, and
  a module docstring became an expression nobody reads, so `__doc__` was
  `None`. The insertion point is after the file's prologue now. Pinned in
  `tests/move_imports.rs`.

- [x] B572: **every element id was reported as an HTTP route.** A rule matches
  a symbol, and HTML declares only element ids. So a page-level rule fired once
  per id. One page with two `<div id>` reported
  two routes, and a page with no ids reported nothing. An id is not a route.
  What an element genuinely offers the outside, a mount point or a form target,
  is still reported as what it is. Pinned in `tests/entrypoints_conventions.rs`.

- [x] B573: **a typed path parameter reached the contract as a string.**
  `def h(i: int)` under `@app.get("/x/{i}")` produced `{"type": "string"}`,
  disagreeing with the document FastAPI generates for itself, under a
  description claiming the schemas are as good as what the source declared.
  The annotation decides now. Pinned in `tests/nextjs.rs`.

- [x] B570: **the capability matrix denied a capability the binary ships.**
  `fr openapi` reads a FastAPI router and writes a document. The row called
  Python not applicable, "because this derives an OpenAPI document from a
  Next.js route tree". Two route shapes reach that command, not one. The row
  names both now, and the claims test proves the cell by running it.

- [x] B571: **the `flow` row answered a dataflow question with a call-graph
  reason.** CSS, SCSS, HCL, YAML and Helm were marked not applicable "because
  this language has no functions, so there is nothing to call", on a command
  that traces values through all five. The verdict was right and the reason was
  not. `fr flow` follows provenance for a language evaluated by substitution.
  That is its own row, and the reason says so and points there.

- [x] B549: **removing a parameter took the wrong argument.** A call passing
  arguments by name resolves them to the parameter. So a keyword three files
  away was reported as "the body of `greet` still reads `punct`", with the
  call site's line. The check looks
  inside the declaration now, which is the only place a removal cannot repair.
  At a call site the name decides which argument goes, so `greet("b",
  loud=True)` keeps what it passed. A call that names arguments and not the
  one going relied on the default, and is left alone. Pinned in
  `tests/signature_hierarchy.rs`.

- [x] B547: **a body that returns a value got no return type.** A Python
  function annotates nothing and still hands something back. Rust, Go, Java
  and Zig must name what, and named nothing, so the draft did not compile.
  Each now names the type the returns agree on. Where they do not agree, the
  target's word for an unknown type carries a note. The canonical builtins
  carry their own types, so `return len(items)` is an integer rather than a
  shrug.

- [x] B548: **a field divided as a float where a local divided as an
  integer.** `this.total / 2` in TypeScript kept its remainder. The same
  division over a local truncated. A bare name in a method body is a local, a
  parameter, or a field of its record. The type question looks in all three
  places now.

- [x] B565: **`fr move` refused a class that names itself.** The cycle it
  named does not exist. `Counter.STEP` written in `Counter`'s own method
  counted as a use left behind in the source file. So the source was given an import of a
  name it no longer mentions. Where the moved code also needed something the
  source keeps, the two phantom imports read as a cycle and the move was
  refused. A reference inside the moved span travels with it and is no longer
  counted. Pinned in `tests/move_dependencies.rs`.

- [x] B564: **`fr restructure` skipped a commented occurrence in silence.** A
  comment is an extra. It sits between two children of the node it interrupts.
  `foo(1, /* why */ 2)` was a three-argument call to the matcher,
  and `foo($A, $B)` passed over it while the run reported itself complete.
  Comments are out of the shape now, so the pattern matches across them. A
  comment inside what a metavariable binds travels with that binding. One
  between the pattern's own tokens has nowhere to go. That occurrence is left
  alone and reported by file and line. Pinned in
  `tests/restructure_languages.rs`.

- [x] B563: **`fr signature` was blind to a macro-hidden method call.**
  `println!("{}", s.draw(4))` gives the grammar tokens and not a call. The
  dispatch pass passed over it without a word. The trait and the impl both grew
  a parameter, the report said "0 call sites", and the crate stopped compiling.
  A dispatch site the pass cannot reach now refuses and names the site. Out of
  reach means a macro body, a call the grammar hides, an unparseable call, or a
  call with no argument list. Rename was checked for the same hole and has
  none. It rewrites the name where it stands and reports the site as a dispatch
  candidate. Pinned in `tests/rust_receivers.rs`.

- [x] B562: **a Terraform rename left the module call behind.** Renaming a
  module's `variable "region"` rewrote the module's own `var.region` reads and
  reported success. The caller's `module "net" { region = ... }` kept the old
  name, and `terraform validate` then rejected the configuration. An argument
  of a `module` block names an input variable of the called configuration. The
  index records it as a reference to that variable now. A source outside the
  workspace resolves to nothing, and the rename reports the argument instead of
  rewriting it. Pinned in `tests/namespaces.rs`.

- [x] B561: **a binding borrowed the enclosing function's type.** `fr type`
  read a Zig `const width = 3;` as `void`. That is the return type of the `fn`
  around it. The walk outwards from a declaration looked for a `type` field on
  four ancestors and never stopped at the block. It stops at the construct that
  holds statements now. Pinned in `tests/types.rs`.

- [x] B560: **`fr extract` wrote uncompilable Go.** Two live-out values came
  back as `return a, b` from a function declared `int`. The report said
  success. Go spells several results as a parenthesised list, and the signature
  says `(int, int)` now. The same selection written idiomatically, with
  `total := 0`, was refused for a type "never written down". Go and Java both
  fix a binding's type at its declaration, so inference supplies it. Only a
  type neither written nor derivable is refused. Pinned in
  `tests/extract_function.rs`.

- [x] B546: **a field's starting value was dropped in both directions.**
  Python's `retries: int = 3` became `retries: number;`. That is undefined at
  run time, and Java's `= new ArrayList<>()` went the same way. Neither took the
  value, so no writer had one to write. Python, TypeScript, Java and Zig each
  declare it in the field now. TypeScript writes a class where a field starts
  somewhere, because an interface holds no initializer. Rust and Go declare no
  value in a field at all, and say so beside it rather than let it go quietly.
  `field(default_factory=list)` reads as the `[]` it means; pydantic's
  `Field(min_length=8)` states a constraint and gives no value, so that field
  starts at nothing. Pinned in `tests/translate_field_defaults.rs`, which runs
  the Java and the Python.

- [x] B545: **only the first number in a concatenation was coerced.**
  Java's `"x" + 1 + 2` raised a TypeError in Python. It came out as
  `"x" + str(1) + 2`, where the source printed `x12`. The chain is
  left-associative, so the outer `+` holds the inner one, and the inner one had
  no type. A `+` with a string on either side is a string, whatever the other
  side is. The whole chain follows from that one line, associativity included.
  Zig's own concatenation check reads the same answer. Pinned in
  `tests/translate_concatenation.rs`, which runs the Java and the Python.

- [x] B544: **a header that bound names was dropped under its branch.** Go's
  `if` may run a statement in its header. `if m, ok := tree.Min(); ok { }`
  lost the header with no marker at all. The branch then tested `ok` and
  printed `m` while the output bound neither. The header is written before
  the branch now. That widens the scope of what it binds, and every target
  here already scopes it that way. Two sibling branches that bind the same
  names shared one scope after the move, so the second settles them again
  instead of declaring them twice. Pinned in
  `tests/translate_orphaned_bindings.rs`, which runs both.

- [x] B543: **`a, b = b, a` and `x, err := f()` carried, even into Python.**
  Python has that syntax to the character. Go returns the pairs the first
  line takes apart. Both were unknown constructs, so the swap never
  happened and the pair left both its names undeclared. The IR settles several
  names at once now. Python, Go, Rust and TypeScript each write their own
  form. Java and Zig have no tuple, and carry the line whole rather than drop
  the names. Pinned in `tests/translate_multiple_assignment.rs`. That gate
  runs the Go, the Python and the Rust, and compares what they print.

- [x] B542: **a Java entry from an unexported source would not start.** Go's
  `main` is lower-case. So the Java draft came out `private static void
  main`. The runtime answered "Main method not found
  in class". Whether the source exported its entry is a fact about the source.
  The entry is written public whatever it was. Pinned by the run in
  `tests/translate_counted_for.rs`.

- [x] B541: **Go's `for` carried in three of its four spellings.** `for { }`,
  `for cond { }` and `for i := 0; i < n; i++ { }` all became comments, and the
  comment took the body with it. Every name the header bound was then
  undeclared. Java's counted `for` went the same way, and so did `i++` as a
  statement of its own. The IR has a counted loop now. Go, Java and TypeScript
  write the whole header. Zig writes the step as a continue expression. Rust
  and Python walk a range where the header walks one and say the rest longhand.
  A `continue` under the longhand would skip the step, so those loops carry
  whole and say so. Fixed alongside: `for i, x := range xs` dropped the index
  and left `i` undeclared. It is an indexed loop now. Pinned in
  `tests/translate_counted_for.rs`. That gate runs the Go, the Python, the Rust
  and the Java, and compares what they print.

- [x] B540: **a field named with no receiver crossed as a free variable.** Java
  lets a body write `accounts` for a field it declares. Every writer here
  needs a receiver written. `tsc` answered "Cannot find name
  'accounts'. Did you mean the instance member 'this.accounts'?" twenty-eight
  times over one translated class. Python was worse: the field was declared
  `balance_cents` while the body still said `balanceCents`, a disagreement the
  translation introduced by itself. The writers now enter a method body through
  one call. It binds the receiver and the fields the body may name bare. A bare
  name in that set is written through the receiver in the field table's
  spelling. A parameter or a local of the same name is the nearer declaration
  and wins. Pinned in `tests/translate_implicit_receiver.rs`. That gate runs
  the Java, the Python and the TypeScript, and compares what they print.

- [x] B536: **a shell function reached through `source` was reported dead, and
  deleting it broke the script.** Sourcing a file is not a binding. It runs
  the file, and every function it defines becomes callable by its bare name.
  The
  call resolved to nothing. So `fr usages` said none, `fr unused` listed the
  function, and `fr delete` removed it while `bash` still called it. A call
  that names a top-level definition of a sourced file resolves to it. So does
  the path, where the source line ends in a plain file name, which is what
  `source "$(dirname "$0")/lib.sh"` does. Pinned in `tests/facts_bash.rs`.

- [x] B537: **a textual match was called a comment.** The sweep matched the
  declaration and every resolved use, then listed them again. The heading read "mention(s) in a comment or a string.
  No command edits these". A YAML key is neither.
  A reader told a broken reference was a comment has been told it is safe. The
  listing drops what the search already accounts for, and says what the rest
  are: matched as text, with nothing linking them to the declaration. Pinned
  by the navigate and rename suites.

- [x] B531: **a directory sweep wrote a package that could not build.** Two
  Python files each declaring `Thing` became one Go package. `Thing redeclared
  in this block` was the first anyone heard of it, and the report said both
  files translated. Where the target keeps a directory in one namespace, the
  file earliest by path keeps the plain name. The others take their own file's
  name in front, and each says so in its header. Pinned in
  `tests/translate_projects.rs`.

- [x] B532: **an import inside a function stayed a comment while its code
  crossed.** `def helper(): from a import Thing` breaks an import cycle. The
  body's `Thing()` became live TypeScript beside a commented-out import, so
  the file named a class nothing brought in. Every target here
  hoists its imports, so a sibling named inside a body is lifted to the file's
  own imports. Pinned in `tests/translate_projects.rs`.

- [x] B533: **an aliased base class left the family.** `from base import Base
  as Foundation` recorded an edge pointing at a name nothing declares.
  So `self.count` in the subclass was left behind, and applying the rename
  raised. Supertype names resolve through the file's imports now, at one point
  that answers for every language. Pinned in
  `tests/rename_property_family.rs`.

- [x] B534: **a leading underscore inverted its own meaning.** The case
  converter read it as a word boundary, so Python's `_helper` became Go's
  `Helper`. The mark for "not outside this module" turned into the mark for
  exported, and `go -> python -> go` published a package's internals.
  Visibility travels in the IR now, Python spells it with the underscore at
  every mention, and the round trip comes home unchanged. The entry point is
  the exception, since `main` is what a runner looks for. Pinned in
  `tests/translate_zig_forms.rs`.

- [x] B535: **a tab reached a Zig comment.** Carried Go source brings Go's
  indentation, and Zig's lexer refuses a tab inside a comment. The
  file could not be read by its own compiler. The comment writer replaces
  them.
  Pinned in `tests/translate_zig_forms.rs`.

- [x] B530: **a translated Java program only ran on the newest JDKs.** The
  entry came out as `public static void main()`. The runtime accepts that only
  where niladic main methods are final. Everywhere else it answered "Main
  method not found in class", so the draft compiled and would not start. The
  JDK on this machine allows it and CI's does not, which is where it surfaced.
  The entry takes `String[] args` now, whatever the source's entry took.
  Reading Java back, that parameter is convention and not data. A `main` whose
  body never touches it comes home with no parameter, and one that reads it
  keeps it. Pinned in `tests/translate_entrypoints.rs` and
  the round trip.

- [x] B527: **`//` crossed to Rust as arithmetic that disagrees with it.**
  `div_euclid` rounds so the remainder is never negative. Python's `//` rounds
  toward negative infinity. They agree only when the divisor is positive, so
  `7 // -2` was -4 in Python and -3 in the draft, running and unmarked. The
  Rust writer emits a floor-division helper whose answers match Python for
  every sign. Pinned in `tests/translate_floor_div.rs`.

- [x] B528: **`fr extract` wrote unparseable code across a loop boundary and
  called it success.** A selection with one end inside a loop's body and the
  other outside it kept its bytes, so the loop's outdent landed in the middle
  of the new function. Such a selection is refused with the boundary named, by
  a guard shared across languages. Both this refusal and the escaping-`return`
  one are considered refusals now, and exit 5 as the help promises. Pinned in
  `tests/extract_function.rs`.

- [x] B529: **a rename trusted an initializer the code had overwritten.** With
  `b = B()` and then `b = A()` on a live path, `b.size(2)` renamed with `B`'s
  method under the claim that `b` is declared `B`, and the result raised
  AttributeError. A type derived from an initializer is evidence only where
  nothing reassigns the binding. Otherwise the site stays for review, and the
  reason says the binding is assigned more than once. Pinned in
  `tests/rename_property_family.rs`.

- [x] B524: **a Python class with two bases lost both of them.** `class
  Import(Taxed, Levied)` crossed as a class extending nothing. The body kept
  `super().cost()`, TypeScript answered TS2335, and the report claimed every
  signature carried. The first base is the one `super()`
  dispatches to, so it rides in the single slot the targets offer. The rest are
  named beside the type. The translated class compiles under `tsc --strict`
  and prints what Python prints. Pinned in
  `tests/translate_inheritance.rs`.

- [x] B525: **a default that read another parameter died at import.**
  `function pad(text, width = text.length + 2)` reached Python verbatim. The
  module raised NameError before anything ran. Python evaluates a default
  once, at `def` time, where the parameters do not exist yet. Such a default
  becomes the sentinel idiom, computed in the body, and the annotation widens
  to admit it. Pinned in `tests/translate_defaults.rs`.

- [x] B526: **a computed assert message was dropped into a comment.** Rust's
  macro takes a format string and arguments, evaluated only on failure. The
  other targets already did that. The message rode above the
  check as prose instead, so the failure said nothing, and any effect
  computing it had was lost. Pinned in
  `tests/translate_asserts.rs`.

- [x] B514: **a file skipped for its size falsified every answer, silently.** A
  workspace holding one file over the scan's limit reported clean success: the
  rename said applied with no warnings, `usages` counted none, `unused` listed
  the symbol, and delete removed a function the skipped file still called. Every
  command that indexes now says what it could not read, on stderr and as
  `skipped_files` in its JSON. One choke point answers for all of them, and
  `--max-file-size` raises the limit. Pinned in
  `tests/json_surface.rs`.

- [x] B515: **`fr imports` stripped a Python package's public API.** A
  `from .mod import api_func` in `__init__.py` is the package's export. The
  tidy step deleted it as unused, and importers raised. An import binding in a
  package `__init__.py` declares what the package offers, and stays.
  Pinned in `tests/imports_liveness.rs`.

- [x] B516: **a recipe whose expectation failed left its edits on disk.** The
  documented promise is one transaction. The refusal path honoured it and the
  expectation path did not. A failed expectation restores the bytes the run
  started from, and the report says whether anything was written. Pinned in
  `tests/json_surface.rs`.

- [x] B517: **a refusal's blocking positions lived only in prose.** Ambiguity
  had structured candidates. A refusal made an agent regex file, line and
  column out of an English sentence. Refusals carry `references` as data now,
  and a recipe's refusals carry the same. Pinned in `tests/json_surface.rs`.

- [x] B518: **the exit-code taxonomy leaked into the generic 1.** A recipe
  stopped by a refusal exited 1 rather than 5. A position naming a file that
  does not exist exited 1 rather than 3. A malformed position was reinterpreted
  as a symbol name. Each failure now exits as the help promises.
  Pinned in `tests/cli.rs`.

- [x] B519: **`fr translate` answered prose to `--json`.** Listing what a file
  could be written as ignored the flag outright. A single-file translation
  omitted the fidelity block its own directory sweep emits. Both speak the
  sweep's schema now. Pinned in `tests/json_surface.rs`.

- [x] B520: **`fr symbols` emitted spans no command could take back.** The
  extract range wants 1-based line and column. Symbols offered byte offsets
  beside them, unlabelled, so an agent converted by reading the file itself. A
  symbol's span carries line and column now, and round-trips into `extract`.
  Pinned in `tests/json_surface.rs`.

- [x] B521: **one warning had three shapes.** A rename's warnings were
  structured on their own and flat prose through a recipe. Location keys
  drifted between `file` and `path`. One shape now, whichever command emits
  it.
  Pinned in `tests/json_surface.rs`.

- [x] B522: **`recipe --explain` re-serialised its plan as surface syntax.**
  Selectors and expectations came back as the strings a reader types. Checking
  a plan meant re-implementing the recipe parser. They are structures now,
  beside the text. Pinned in `tests/json_surface.rs`.

- [x] B523: **deleting a definition left the blank lines that framed it.** The
  runs above and below merged into one. A Python file kept three blank lines
  where its style puts two. As many trailing blanks go as there were leading
  ones. Pinned in `tests/refactor_delete.rs`.

- [x] B513: **a Rust `match` on the module's own sum carried whole.** Rust's
  one spelling for sums took entire function bodies into comments. Unit and struct patterns read into the variant match, bindings
  and renames included. A match naming a foreign choice, an imported enum this
  module never declares, still carries, re-rendered from the IR so the carry
  keeps its body. Pinned in `tests/translate_narrowing.rs`.

- [x] B505: **consuming a sum value never crossed.** Construction landed a pass
  ago; the question "which variant is this?" did not. `s.kind == "circle"` and
  `s.radius` went to Rust verbatim, against an enum that declares neither,
  under a header claiming every signature carried. The IR holds the match now,
  payload fields bound to plain locals. TypeScript's kind chains and switches
  read into it, and each writer spells it natively. Rust matches, Python asks
  `isinstance`, Go switches on type, TypeScript switches on the discriminator,
  Java tests `instanceof`, Zig switches on the union. Pinned in
  `tests/translate_narrowing.rs`.

- [x] B506: **two sums sharing a tag degraded to a map with a clean header.**
  Two state unions holding an `"idle"` is ordinary TypeScript. The value
  became `HashMap::from([("kind", "idle")])` in a position that wants `Fetch`.
  A return under a declared signature and a binding under an annotation now
  settle against the type the position names. Pinned in
  `tests/translate_narrowing.rs`.

- [x] B507: **the discriminator literal was derived, not read.** An interface
  named `FIdle` writing `kind: "idle"` got the derived tag `f_idle`. No
  consumer matched, and the writers respelled the wire format.
  A variant carries the literal its source declared, and every reader and
  writer prefers it. Pinned through `tests/translate_narrowing.rs`.

- [x] B508: **a variant dodging a name collision was built under the name it
  dodged.** The declaration renamer wrote `class StatusOk`. The construction
  site wrote `Ok()`, the very record the dodge avoided, and running it
  raised. The spellings live in one table now, computed before anything is
  written, and both sides consult it. Pinned in
  `tests/translate_narrowing.rs`.

- [x] B509: **a struct used concretely was consumed into its sum anyway.** Go's
  `func Standalone() Point` kept the type. Inside it, `Point{}` became
  `Shape::Point`, and rustc refused both lines. A member named in a concrete
  position keeps its struct beside the variant and sheds the marker method.
  A construction settles by the position it stands in. Pinned in
  `tests/translate_narrowing.rs`.

- [x] B510: **a shadowed union member still settled as the variant.** A nested
  `def Card(...)` means `Card(number)` calls the local. The call became `Payment::Card` and changed the value's type. A
  name bound by a carried construct in the same function holds its calls back,
  and they carry visibly. Pinned in `tests/translate_narrowing.rs`.

- [x] B511: **Java's sealed interface never formed a sum.** The most explicit
  closed-choice declaration of the five crossed as an empty struct. The
  returns came out wrong-typed under a clean header, while Go's marker idiom
  had settled for a pass already. An empty interface with method-less implementing
  records is the sum it declares. `new Point()` builds the variant,
  `instanceof` plus the cast collapse into the match, and the accessor reads
  become payload bindings. Pinned in `tests/translate_narrowing.rs`.

- [x] B512: **an integer literal under a float signature stopped Rust.**
  `return 0` where `f64` was promised, and `n <= 0` against a float parameter:
  Go and Zig coerce the untyped literal, Rust refuses it, and the draft died
  in rustc. Returns take the declared type and comparisons take the binding's,
  so the literal gains its point where the target needs one. Pinned in
  `tests/translate_narrowing.rs` and `tests/transpile.rs`.

- [x] B443: **the wasm slice could not see a cli-only import.** With default
  features on, an import only the CLI uses looked used. The unused-import refusal surfaced in the deploy's wasm build
  instead. The slice now also runs clippy without default features, on
  the host target, which catches the class without needing a wasm clang.
  Caught by CI on this very pass: `run::distance` was cli-only and ungated.

- [x] B442: **a Java record crossed wrong twice.** `implements X` was dropped
  in silence. A compact `name()` body crossed beside the `name` field, and
  the pair collided in every flat target. A single interface rides in the
  base slot, and more of them ride in prose. The field wins the collision,
  and an overriding body is said beside the field. Pinned in
  `tests/translate_java_records.rs`.

- [x] B441: **a lost initializer took its binding's name with it.**
  `var wg sync.WaitGroup` and `ch := make(chan int, 4)` carried whole. Every
  later statement then read a name the output never declared. The
  binding stays, the original rides in a marker, and TypeScript types it `any`
  so strict compilation accepts the declaration. Pinned in
  `tests/translate_carried_bindings.rs`.

- [x] B440: **an annotated instance field vanished.** `self.entries: list[str]
  = []` read as a binding whose dotted "name" was no name at all. The whole
  assignment carried as a comment and the field was deleted. It reads as the
  plain assignment now, and the derived field takes the annotation's type.
  Pinned in `tests/translate_classes.rs`.

- [x] B439: **`super` and the exception bases spoke the source.**
  `super().__init__(m)` crossed as a call to `super_`, which nothing declares.
  A class extending `Exception` extended a name TypeScript lacks. Coming home,
  `super(m)` carried and the constructor gained a `raise NotImplementedError`.
  The reach is canonical now and each writer spells it, the bases map both
  ways, and `ABC` drops with a note. Pinned in `tests/translate_super.rs`.

- [x] B438: **an optional parameter required its argument.** TypeScript's
  `punct?: string` crossed to Python as an optional type with no default.
  Every valid call site then raised TypeError. The absence carries as
  `= None`.
  Pinned in `tests/translate_optional_params.rs`.

- [x] B437: **floor division was a runnable `null`.** Python's `cents // 100`
  read as an unknown operator, and a formatter printed "$null.00". Every
  writer says it with its own flooring call now, `Math.floor`, `div_euclid`,
  `Math.floorDiv`, `math.Floor`, `@divFloor`, and Python keeps the operator.
  Pinned in `tests/translate_floor_div.rs`.

- [x] B436: **a lambda crossed as a runnable `null`.** `lambda x: e`,
  `(x) => e`, `|x| e` and `x -> e` are one nameless function, and each carried
  as a marker where a callback belonged. The one-expression shape crosses
  between the four languages that have it. Go and Zig carry it visibly, since
  neither writes a closure without types. On the way this surfaced a bracket
  loss: `(a == b).then(x)` rendered as `a == b.then(x)` in every writer. A
  field or index receiver now takes brackets from structure. Pinned in
  `tests/translate_lambdas.rs`.

- [x] B435: **a translated test file checked nothing and said it passed.**
  Python's `assert c, "m"` carried as a comment. The crossing then ran,
  checked nothing, and printed "all tests passed". Asserts are a statement of their
  own now. Python, Rust and Zig read theirs; every target writes its own, and
  the ones without an assert test the condition and throw or panic. Pinned in
  `tests/translate_asserts.rs`.

- [x] B430: **an inverted extract range died on the span constructor's
  assertion.** `fr extract --range file:8:20-8:5` printed byte offsets and a
  panic. It is refused where both ends are known, with both ends named.
  Invalid input exits 2, the code clap uses for a command line that does not
  parse. A column of 0 is refused with "columns start at 1." instead of
  quietly reading as column 1. Pinned in `src/span.rs` tests and
  `tests/cli.rs`.

- [x] B431: **two refusals broke the exit-code promise.** Delete's "refusing
  to delete" exited 1 while the help promised 5. It is a typed refusal now.
  `fr remove-flag` on a name nothing declares also exited 1. It goes through
  the not-found path rename uses, exits 3, and suggests the nearest declared
  names. Pinned in `tests/cli.rs`.

- [x] B432: **human listings printed absolute paths.** Every site in a rename
  report carried the workspace prefix, noise a reader skips over. Human
  output is workspace-relative through one
  helper, error prose is relativised at one choke point, and JSON keeps
  absolute paths. Pinned in `tests/cli.rs`.

- [x] B433: **RECIPES.md promised `fr recipe --explain` and `fr recipe fmt`,
  and neither existed.** `--explain` exists now. It prints each step's
  selector and expectation without running it. `fmt` stayed unbuilt, and the
  document says so and why. The `.fr` example extension became `.recipe`, the
  one the tool reads. Pinned in `tests/cli.rs`.

- [x] B434: **`fr` sat silent while indexing a large workspace.** A first run
  over a big repository gave no sign anything was happening. Indexing paints
  progress on stderr when stderr is a terminal, in coarse steps, erased on
  completion; piped output stays byte-identical. Pinned by the cli suite's
  piped-output assertions.

- [x] B429: **`fr flow back` claimed a `-f` nobody passed.** Without inputs the
  strongest source printed as `user-supplied -f values-prod.yaml`. The label now
  reads `would win under -f values-prod.yaml`, and a loser says when the
  override would apply. The hedging around the undecided answer stays. Pinned
  in `tests/helm_inputs.rs`.

- [x] B428: **`fr impact` promoted callers past an unproven edge to certain.**
  Each caller carried only its last hop's confidence. So everything above a
  field-based dispatch edge read as "would definitely change". A route is now as
  trustworthy as its weakest edge, and a node keeps the best route's confidence.
  Pinned in `src/analysis/impact.rs` tests.

- [x] B427: **a call through an import alias resolved to nothing.** `from lib
  import helper as h2` then `h2()` left `helper` with no callers. A bare name an
  import binds resolves through the import under the imported original, at
  import-qualified confidence. An aliased re-export chain carries each hop's own
  original name. Pinned in `src/index.rs` tests.

- [x] B426: **declared Python console scripts read as dead code.** `fr
  entrypoints --unreachable` flagged a function that `[project.scripts]`
  installs as a command. setup.py `console_scripts`, pyproject
  `[project.scripts]` and a package's `__main__.py` are entry points now. The
  packaging files are read line by line, and each detection's rule says so.
  Pinned in `src/analysis/entrypoints.rs` tests.

- [x] B425: **docker-compose `environment` entries were invisible to `fr
  stitch`.** Both spellings count now, `APP_MODE: x` and `- APP_MODE=x`.
  Compose files are recognised by shape, a
  top-level `services:` mapping with an `environment` key under it. Their
  variables join chains and orphan detection beside the Kubernetes `env:`
  shapes. Pinned in `src/analysis/stitch.rs` tests.

- [x] B424: **a chart value declared in two values files was two symbols.**
  Usages found nothing. A rename moved one file, and delete removed a value
  the template still read. A Helm values key now groups
  with its same-path keys across one chart's `values*.yaml` files, the way CSS
  classes group. Usages, rename and delete act on the whole entity, and the
  template read blocks delete. Pinned in `tests/helm_values_refs.rs` and
  `src/index.rs` tests.
- [x] B418: **a value of a sum type never crossed.** The types crossed for
  eleven passes while every value of one carried: Rust's `Shape::Point` reached
  Python as a comment, Zig's `.{ .one = n }` took its whole `if` with it, and
  a Python class consumed into a union kept constructing as a class the target
  never declared. The IR holds the variant now, validated against the module's
  own sums, and each writer builds it the way its language does: Python calls
  the constructor, TypeScript writes the declared discriminator, Go
  parenthesises the composite literal out of the `if x == Go{}` trap, Java
  orders the record's fields, Zig infers the union from the position. Every
  reader produces it too. Go composite literals settle as variants or record
  constructions. TypeScript kind-literal objects settle against the module's
  sums, and the inline union form becomes the named one's sum. A
  path naming anything else, `Vec::new`, an enum from another crate, goes back
  to being carried whole. Pinned in `tests/translate_variants.rs`.

- [x] B419: **`fr inline --call` pasted a callee that read its own file's
  imports.** B412 held module globals back and stopped there. `os.environ`
  crossed into a file that never imports `os` and raised NameError the same
  way. A name bound by the callee file's imports counts as carried now, both
  ways. Visible at the call site, through any import form, the inline goes
  through. Pinned in `tests/inline_call.rs`.

- [x] B420: **a Python property renamed one door of two.** `@property def size`
  and `@size.setter def size` are one attribute; renaming the getter left the
  setter answering the old name and left `@size.setter` reading a binding the
  class no longer had. Both defs are one definition group now. The
  decorator's bare `size` resolves lexically, since a `def` in a class body
  binds the name in the namespace the decorator reads. Pinned in
  `tests/rename_property_family.rs`.

- [x] B421: **a use site inside the owner counted the property's two doors as
  two candidates.** `b: Box` made `b.size` ambiguous. The very class that
  declares the property could not reach it, because ambiguity was counted in
  symbols. It is counted in entities now: candidates that form one definition
  group are one answer wherever the count decides. Pinned in
  `tests/rename_property_family.rs`.

- [x] B422: **a receiver the source typed did not carry its member sites.**
  Three forms of the same silence: `b.size` with `b: Box` stayed behind at
  field-based confidence though `Box` declares the property; `s.area()` with
  `s: Sub2` stayed though `Sub2` extends the owner; and `var b = new B()`
  claimed the type unknown though the construction writes it on the right of
  the `=`. The family's owners now include every declared subtype, and the
  derivation feeds the receiver's type where no annotation exists. A weak
  member site renames when its receiver's known type owns the renamed entity
  and nothing else answers that name on that type. Pinned in
  `tests/rename_property_family.rs`.

- [x] B423: **`self.count` in a subclass one import away stayed behind.** B407
  crossed the class chain inside a file; an attribute family whose base class
  lives in another module still skipped the subclass sites, because the
  enclosing instance is the one receiver `receiver_declared_type` refused to
  answer for. The enclosing class is the answer, and the subclass sits among
  the family's owners, so the site renames. Pinned in
  `tests/rename_property_family.rs`.

- [x] B412: **`fr inline --call` pasted a callee's module globals across files.**
  `clamp` read `LIMIT` from beside itself. Pasted into another file, the name
  meant nothing there. The paste compiled, ran, and raised NameError without a
  warning. A body name defined beside the callee and invisible at the call
  site refuses, named. Pinned in `tests/inline_call.rs`.

- [x] B411: **`fr move` broke Python importers twice over.** Code moved into the
  file it imported from carried the import along, a module importing itself
  half-initialised; and an importer holding the whole module (`import mod;
  mod.foo()`) gained a dead named import while every call kept dereferencing
  the module that no longer held the name. The self-import is dropped, and the
  module-attribute receivers rewrite to the new module, which the importer now
  imports. Pinned in `tests/move_languages.rs`.

- [x] B410: **a receiver's declared type did not hold its call still.** Renaming
  `A`'s overloads took `b.size(2)` with them as a dispatch candidate. `b` is
  declared `B`, and `B` answers `size` itself, so javac refused the result.
  A dispatch-candidate site whose receiver's declared type sits outside the
  family stays, and the warning names the type instead of claiming it unknown.
  The same evidence holds `fr signature` still. Pinned in
  `tests/rename_hierarchy.rs`.

- [x] B409: **TypeScript overload signatures renamed apart from their
  implementation.** Two `function pick` declarations over one body are one
  function. Renaming any alone left `error TS2389`. Same name, same file, same
  container is the entity. Pinned in `tests/rename_hierarchy.rs`.

- [x] B408: **deleting the only statement of a Python suite wrote a file that
  does not parse.** A `pass` fills the hole. The judgment covers every span of
  the plan, so a multi-site delete still empties cleanly.
  Pinned in `tests/python_attributes.rs`.

- [x] B407: **instance attributes and locals fed each other's renames.** A bare
  `count` never names a member in the languages that spell members through a
  receiver. `self.count` in a sibling method is a member of the enclosing
  class wherever its definition sites sit. Both resolutions said otherwise, so
  a local's rename took one line of three, and an attribute's skipped the
  sibling method and the subclass. Bare names now exclude members, the
  enclosing instance resolves by the class the code sits in, and the attribute
  family crosses the declared class chain. Pinned in
  `tests/python_attributes.rs`.

- [x] B399: **two racing `fr rename --write` runs both reported applied and one
  rename vanished.** Whole-file writes let the last writer win in silence. The
  commit now re-reads every file and refuses whenever the text differs from what
  the plan read, and nothing partial is written. OS locks held in the system
  temporary directory serialise the read-verify-write window. Pinned by the
  commit tests in `src/edit.rs`.

- [x] B400: **`fr symbols --json | head` panicked once `head` closed the pipe.**
  Exit 101 and a broken-pipe abort, for a reader that had taken what it wanted.
  Every stdout write now treats a closed pipe as the end of the run and exits 0.
  Pinned in `tests/cli.rs`.

- [x] B401: **diff headers named absolute paths, which `git apply -p1` refuses.**
  Headers are now workspace-root-relative, `a/src/x.rs`, while the JSON `path`
  fields stay absolute. Pinned in `tests/cli.rs` and `tests/json_surface.rs`.

- [x] B402: **`fr usages` and `fr rename` disagreed about the same entity.**
  Usages excluded definition sites; rename counted them. So `files_changed`
  said 2 where usages saw 1 file. `fr usages` now lists the definitions apart
  from the uses, and rename's JSON carries `definition_edits`. Pinned in
  `tests/json_surface.rs`.

- [x] B403: **every domain failure exited 1.** Not found exits 3, ambiguous 4,
  a refusal 5. Clap keeps 2 and everything else stays 1. `fr --help`
  documents the codes. Pinned in `tests/cli.rs`.

- [x] B404: **`fr scan --json` spelled paths its own way and dropped symlinks
  in silence.** Each item now carries an absolute `file` beside `path`.
  Skipped symlinks are listed with their targets named. Pinned in
  `tests/json_surface.rs` and the `src/scan.rs` tests.

- [x] B405: **a `restructure` step that matched nothing reported "matched 1,
  applied 0" and ok.** The pattern is the step's selector. So an empty match
  now stops the run unless `allow-empty` says it may. The matched count is the
  occurrence count. Pinned in `tests/recipe.rs`.

- [x] B406: **`fr remove-flag` left the flag's name behind in strings, comments
  and config.** The rename sweep now runs over the finished workspace. Every
  remaining mention lands under "Left undone" with its file and line. Pinned
  in `tests/remove_flag_sweep.rs`.

- [x] B417: **three markers stopped the build they were drafted into.** Go's
  inline stand-in was a bare `nil`, untypable at `:=`. It binds as `any(nil)`
  now, and only a call stands alone as a statement. Rust's `todo!`
  interpolated braces the carried source brought along; they double. A
  constant whose value held anything untranslated became a `todo!` a `const`
  evaluates at compile time; it carries whole as a comment, name and all.
  Pinned in `tests/translate_markers.rs`.

- [x] B416: **the implicit entry never crossed.** Rust, Go, Java and Zig run
  `main` without writing a call. Their programs translated to Python and
  TypeScript did nothing. The readers synthesize the call, and the
  self-running targets drop it again with a note. Python guards it, passing
  `sys.argv[1:]` to a `main(String[] args)` and starting an async main under
  `asyncio.run`. Go keeps a niladic `main` lowercase, so `package main`
  still starts. Pinned in `tests/translate_entrypoints.rs`.

- [x] B415: **a thrown class was one the target never declared.** `throw new
  Error(m)` reached Python as `raise Error(m)`. `raise ValueError(m)`
  reached TypeScript as a call to nothing. The readers fold the everyday
  names into the canonical ones, and TypeScript declares one-line classes
  for the builtins it lacks. A caught error read as text is its message
  everywhere: `str(e)`, `(e as Error).message`, `e.getMessage()`. The probe
  fixtures run byte-identical to their sources in both directions.
  Pinned in `tests/translate_exceptions.rs`.

- [x] B414: **a Result crossed as a type nothing could write.** Rust's
  `Result<T, E>` and Zig's `E!T` read as one shared name now. Go writes the
  `(T, error)` pair: `Ok` returns beside `nil`, `Err` returns the zero and an
  error, a propagated call binds beside a checked `err`. The exception
  languages return the ok value bare and raise the `Err`. Zig spells the
  union back, error sets cross as sums, and `format!` is a template.
  Pinned in `tests/translate_results.rs` and `tests/translate_propagation.rs`.

- [x] B413: **a value-position Zig switch carried, and one-statement branches
  vanished.** `const x = switch (...) {...};` lowers to declare-then-assign.
  Every writer already says that shape, and the Rust writer folds the pair
  back into a `match` expression. Found beside it: `if (x) return e;` dropped
  its return without a word, and a `while` with a step clause lost the step.
  Both cross or carry visibly now, and the corpus ledger is re-pinned.
  Pinned in `tests/translate_results.rs` and `tests/translate_corpus_sweep.rs`.

- [x] B398: **a Python instance attribute was not a symbol at all.** `fr rename`
  answered "no symbol or resolved reference at". Python programs rename that
  target more often than any other. Each `self.x = ...` site now defines a
  field. The class, carried as the qualifier, groups the sites into one entity,
  and the reads follow the rename. Pinned in `tests/python_attributes.rs`.

- [x] B397: **`@property` crossed as a method while its accessors stayed reads.**
  In the target, `it.total` was the function object. Every comparison against it
  was quietly false. The flag crosses on the method. TypeScript writes
  `get total()`, and Python writes the decorator back. The targets without the
  idiom write the accessors as the calls they become.
  Pinned in `tests/translate_classes.rs`.

- [x] B396: **the everyday library calls crossed as compile errors.**
  `console.log` reached Python, `.push` reached Rust, `print` reached
  TypeScript, all unmarked. The readers rewrite their spellings into one
  canonical set, and the writers rewrite them out: `print`, `len`, `str`,
  `.append`, `.upper`, `.lower`, `.strip` and `sep.join(xs)`. Go gains the
  imports its mapped calls need. Pinned in `tests/translate_builtins.rs`.

- [x] B395: **the program's own entry was dropped as unsupported.** Two entry
  forms became comments. `main();` sits at the bottom of a TypeScript file, and
  the other call sits under Python's `__main__` guard. The translated program ran
  and printed nothing. A top-level statement is an item now; Python writes it back under
  its own guard. Two shapes around the same story went with it. A field's
  initializer crosses as a default the dataclass accepts. A returned object
  literal builds the record its signature promised.
  Pinned in `tests/translate_entrypoints.rs`.

- [x] B394: **a class crossed as an empty struct.** Python declares fields in
  `__init__`, and `record Order(...)` declares them in its header. Both were
  read as nothing, while the methods went on using them. Both derive now. A
  constructor of plain assignments becomes each target's own constructor, and
  `Item(...)` becomes a construction. A Java static loses the receiver its
  call sites never passed, and a record's accessor calls become the field
  reads they are. Pinned in `tests/translate_classes.rs` and
  `tests/translate_java_records.rs`.

- [x] B393: **`return a, b` translated to a bare `return`.** The reader mapped
  Go's multiple return to nothing. A two-value return lost its payload with
  nothing said, in every target at once. Several values travelling as one are
  a tuple in the IR now, expression and type both. A writer with no spelling
  for one says so instead. Pinned in `tests/translate_tuples.rs`.

- [x] B392: **a field and a method under one name shared one use list.** The
  Rust facts recorded a method call's callee as a field read, so `order.name()`
  and `order.name` were indistinguishable: the field's uses counted zero and
  the method collected the field's accesses. The callee records as a call now,
  and the resolver keeps only the member the syntax allows.
  Pinned in `tests/member_kinds.rs`.

- [x] B391: **`fr move` broke both of an importer's imports.** An aliased
  `import { foo as increment } from "./a"` named a gone export. A fresh
  unaliased import landed beside it. The existing statement repoints,
  keeping the alias and splitting stayers from movers. The Go half of the same
  probe: a moved body's bare calls back into its old package now qualify with
  the package name. The destination gains the import, and an unexported
  dependency refuses with the visibility problem named.
  Pinned in `tests/move_imports.rs` and `tests/move_languages.rs`.

- [x] B390: **`fr signature` skipped a function held as a value.** `let f:
  fn(i32, i32) -> i32 = add;` has no argument list to rewrite, so the site was
  silently passed over, the declaration changed under the binding, and the
  command reported clean call sites. A value-shaped mention outside an import
  refuses, naming the binding. Pinned in `tests/signature_hierarchy.rs`.

- [x] B389: **renaming Java overloads wrote calls to nothing.** Both `size`
  declarations renamed as one entity while every call stayed behind at
  name-only confidence. So javac refused the result. When the group holds
  every declaration the name answers to, a name-only call can only reach a
  renamed one. So it renames too, reported under the dispatch-candidate heading.
  A stranger answering the same name still holds the calls in place.
  Pinned in `tests/rename_hierarchy.rs`.

- [x] B388: **`fr inline` ran a side effect twice.** `let v = effect(); v + v`
  inlined to `effect() + effect()`. The call inliner already refused this for
  arguments. The variable path now applies the same rule, and only to values
  that can run something, so `a + b` twice still inlines.
  Pinned in `tests/inline_scope.rs`.

- [x] B387: **a rename could move a use under a shadow and change what runs.**
  An inner `let temp` shadowed outer `value`. Renaming `value` to `temp`
  rebound the use, the file compiled, and it returned a different number.
  Both directions refuse
  now, naming the capturing declaration, the line, and the fact that the
  compiler would not have noticed. Pinned in `tests/rename_capture.rs`.

- [x] B386: **the OpenAPI status note spoke FastAPI at a Next.js tree.** The
  note was copied from the translation. That note tells the reader to add
  `status_code=` to a `@router` decorator that exists nowhere in their tree.
  The statuses now travel as data on the route plan. The baseline writes its
  own note in the source's terms: `NextResponse.json(..., { status })` or
  `new Response(..., { status })` settles the status.
  Pinned in `tests/json_surface.rs`.

- [x] B385: **a malformed position was looked up as a symbol name.** `fr def
  py/app.py:abc:1` answered "no symbol named 'py/app.py:abc:1'". That sent
  the reader after a naming problem when the fault was a typo in the position.
  A target shaped like a position, an existing file followed by
  colon-separated parts, is now refused with the part that is wrong. Pinned
  in `tests/json_surface.rs`.

- [x] B384: **every failed command printed nothing to stdout under `--json`.**
  An agent asking for JSON had nothing to parse. The CLI now prints one
  `{"error": {...}}` object on stdout when `--json` was passed. The `kind`
  field names what went wrong: `not-found`, `ambiguous`, `refused`,
  `invalid-input`, `io`, or `error` for a plain failure. An ambiguous name
  carries a `candidates` array: name, kind, path, line and column for each
  rival. The data is threaded from the site that knew it, never parsed back
  out of the prose. The stderr prose and the exit codes are unchanged. Pinned
  in `tests/json_surface.rs`.

- [x] B383: **a signature change and a delete ignored the dispatch family.
  `remove:0` took `&self` off a trait method.** Three holes in one probe. The
  receiver sat in the declaration's parameter list. Position 0 addressed it,
  while every call site counted from the first real argument. `fr signature`
  now takes the receiver off the addressable list for Rust, Python and Zig.
  The change and the delete now follow the same family `fr rename` learned in
  B382: every member's declaration changes or goes, each member's body guards
  the change, and the dispatch sites that resolve to no single implementation
  are updated with the declared default and named in the notes. The family
  expands only through declared relationships; the name-only tier that fans a
  Java call out for reachability is deliberately too weak to merge a change,
  which the first version of this fix learned from two unrelated `width`
  methods in the compile gate.

- [x] B382: **renaming a trait method left its implementations behind.** Silent
  broken code. `fr rename` on `Shape::area` renamed the declaration alone.
  `impl Shape for Circle` kept `area`, the dyn-dispatch call kept `area`, and
  the plan reported one clean site. The reverse direction was as wrong from the
  other end. A method in declared dispatch now renames as one family, through
  the same `Hierarchy` the call graph and `fr unused` already trust. The family
  is the declaration, every implementation, and the dispatch sites that resolve
  to no single one of them. The dispatch sites are reported under their own
  heading at field-based confidence, for a person to review. A same-named
  method on an unrelated type stays untouched.

- [x] B381: **Java was the one language refused both kinds of extraction.** The
  machinery already fit it. `requires_explicit_types` copies declared types the
  way it does for Rust, Go and Zig, and `var` infers a binding the way `let`
  does. What was missing was the arms. Java now extracts an expression into a
  `var` binding, and statements into a `static` method at the class's member
  indent. A mutated outside binding travels back the way it does everywhere
  else. A local declared with `var` refuses by name, because the type it would
  need was never written down. The compile gate drives both through `javac` now
  instead of pinning the refusal, and the capability matrix moved to 272 of
  384.

- [x] B379: **a call to a declared record wrote a call.** Silent wrong answer.
  `Point(0, 0)` from Python crossed into Rust as `Point(0, 0)`, which does not
  compile against named fields. In Go it crossed as a conversion, which means
  something else entirely. The record is declared in the same module, so its
  field names are known, and a positional construction now maps onto them:
  `Point { x: 0, y: 0 }`, `Point{X: 0, Y: 0}`, `Point{ .x = 0, .y = 0 }`. An
  arity mismatch stays a call, because mapping it would invent a default.

- [x] B380: **TypeScript parameter properties never became fields.** Dropped.
  `constructor(public x: number)` declares the field and assigns it, in the
  parameter list. The reader saw only a parameter, so the class crossed with
  no fields at all. The modifier now declares the field it names.

- [x] B378: **the Go and Java readers read `total += item` as `total = item`.** Silent
  wrong answer. One grammar node covers `=` and `+=` in both languages, and both
  readers took the sides and dropped the operator. A translated accumulator
  assigned its last element instead of its sum. Python, TypeScript, Zig and Rust
  carried the statement instead, a gap rather than a lie. All six now desugar
  `target op= value` into `target = target op value`, and an operator with no
  counterpart, `>>=`, carries whole. Covered in `tests/translate_while_present.rs`.


- [x] B373: **`fr extract --function` lost a mutation to an outside binding.** Silent
  wrong answer. A binding declared before the region, assigned inside it and read
  after became a parameter. A parameter is a copy in every one of these
  languages. `invoice_total` extracted its loop and started returning zero. The
  changed value travels back as a return now. The call assigns instead of
  declaring, and the Rust parameter says `mut`. Zig refuses by name, because its
  parameters cannot be assigned at all.

- [x] B374: **a TypeScript assignment target was no use of its binding.** Query gap.
  The reference catch-all is restricted to `primary_expression`, and the left side
  of `total += item` is not one, so the index recorded no use. Extraction moved
  such regions without passing `total` in, and the draft named a binding that no
  longer existed. Rename and usages missed the same sites. Explicit patterns for
  assignment targets close it.

- [x] B375: **a statement range without `--function` built a garbage edit.** Refusal
  late. `fr extract` on a `for` loop spliced `name = for …`, and only the reparse
  gate stopped it. Its message spoke about parsing instead of the flag that does
  what was wanted. The binding path now refuses a statement by name and points at
  `--function`.

- [x] B376: **every translated Zig call lost its arguments.** Silent. The grammar
  hangs arguments off the call with no argument-list node; the reader looked for
  one, found nothing, and read every call as nullary. `twice(x)` crossed as
  `twice()` with a clean fidelity report. The arguments are the children after the
  callee, which is what `fr inline` had always known. Four ledger counts rose to
  the honest number when the arguments started carrying.

- [x] B377: **a Go `:=` with any right side carried whole.** Wrapper. Both sides of
  `:=` and `=` arrive inside an `expression_list` even when they hold one
  expression, and the wrapper reached `expr` as an unknown construct. The single
  element is unwrapped now; a genuine pair, `a, b := f()`, still carries, because
  the IR cannot bind two names at once.

- [x] B364: **a Zig file whose top level is fields loses them in translation.** The
  file-as-struct idiom. zls writes `const Self = @This();` and fields at file scope.
  The reader had no record to put them in, so each carried as a comment. Fixed. The
  reader builds a record from the file's fields, named by the `@This()` binding. When
  the binding is the conventional `Self`, the file itself names the record.
  Signatures saying `Self` are renamed to the record, so the output never mentions a
  type it does not declare. Receiver-taking functions join the record as methods the
  way they do everywhere else. Covered in `tests/translate_file_struct.rs`.

- [x] B365: **a Zig tagged union has no crossing.** Missing feature. `union(enum)` is
  a Rust enum with payloads, a TypeScript discriminated union, in their spellings. The
  reader carried it whole. Fixed: the IR has a sum, a closed choice of variants each
  carrying named fields. Every language that can spell one has a reader. Rust enums
  with payloads and Zig `union(enum)` and plain `enum` cross. So do TypeScript
  discriminated unions over the file's own object types, and Python unions of local
  dataclasses. So does Go's marker-interface convention, which this tool's own Go
  writer emits, so a trip through Go comes home. Writers exist for all six; Java spells it
  `sealed interface` over records. An untagged Zig `union` stays carried, because it
  overlays members and knows nothing about which is live. In a flat-namespace target,
  a variant whose name collides with another of the file's types is prefixed with its
  sum's name. The rename lands in the notes. Explicit discriminants and unnamed tuple
  payloads cross with their loss said in the variant's doc. Covered in
  `tests/translate_sums.rs`, and the corpus ledger dropped: every `container_field`
  the zls corpus used to carry now translates.

- [x] B286: `fr inline` parenthesised by what the value was, not by where it went.
  So `let scaled = base` inlined to a needlessly bracketed
  `let scaled = (w * 2 + h * 3)`. Fixed per use site without a precedence table. A
  declaration, an assignment, an argument list, a return and a collection element
  each hold their whole value between delimiters. No operator outside can reach in,
  so the value goes in bare. Any parent the list does not
  recognise keeps the wrap, which errs toward noise and never toward changed
  arithmetic. `(w + h) * 2` still gets its parentheses. The same rule spares
  `fr inline --call` from wrapping an expansion that lands in a delimited spot.
  Extract-then-inline round-trips bytes in the common cases now.

- [x] B372: **the prose meter read char literals as string delimiters.** Parity flip.
  The extractor lexes string literals with a regex, and `'"'` in the code read as an
  opening quote. Spans of plain Rust between two of them counted as prose. 160 of
  the "long sentences" in the ledger were code. Char literals are blanked before the
  scan now. Every budget in `tools/PROSE-DEBT` moved to the honest number, which is
  lower.

- [x] B366: **a Python keyword argument carried the whole statement with it.** The IR
  had `Expr::Keyword`; no reader produced one. So `encode(a, algorithm=c)` carried as
  a comment. The reader produces it now. A target without keyword arguments degrades
  one argument and says so inline, where it lost the line before.

- [x] B367: **a Java cast carried the whole statement with it.** No IR node. So
  `((JsonArray) o).elements` took its `return` out as a comment. `Expr::Cast`
  exists now, and every writer spells it: `as` twice, a conversion, `@as`, and the
  parenthesised original. Python drops it, because a cast is not a thing there.

- [x] B368: **a TypeScript destructuring declaration carried whole.** Needless.
  `const { params } = parse(context)` is a binding and a field read, sayable
  everywhere. It lowers to that now, one binding for several names; renames,
  defaults and nesting still carry, and say so.

- [x] B369: **a Zig field default was dropped without a word.** Silent. `mutex: Mutex
  = .init` became a field with no default and no note. No language here puts a default
  on a plain struct field, so it is still dropped. The field's doc now says what the
  source gave it.

- [x] B370: **a translation to `--out` with an unfamiliar extension failed in the edit
  engine.** Re-detection. The engine took the language from the destination's name,
  and `api.gen` names nothing. The plan knows the language it wrote; the edit set carries
  that declaration now, and detection by name is the fallback.

- [x] B371: **the translate sweep read only what the CLI printed.** Truncated. Ten
  notes, then "and N more"; the ledger pinned a tenth of the truth. The corpus sweep
  counts in process now, ratcheted both ways. The remove-flag sweep drives the one
  writing command no sweep had reached, in seven languages.

- [x] B363: **the prose meter never decoded a string's escape sequences.** Undercounted.
  `.\n` ended no sentence, gluing each message to the next literal in the file, and the
  two words around any `\n` counted as one. Real over-long strings hid under the miscount.
  The extractor decodes now, and the long-sentence budget was re-baselined upward to the
  honest number, with the note in `tools/PROSE-DEBT` saying why.

- [x] B358: **`fr translate` wrote Python's `NewType` incantation into every target as a
  value.** `Pence = NewType("Pence", int)` was read as a constant. So Rust got
  `pub const pence: &str = NewType("Pence", int);`, which parses and refers to nothing.
  The IR has a `Newtype` item now. Python reads the call, and TypeScript reads the brand
  idiom. Each writer spells the real thing: a tuple struct, a defined type, a brand plus
  constructor, a one-component record, a non-exhaustive integer enum. Construction
  follows, with `new` in Java and `@enumFromInt` in Zig. Found by translating the
  tutorial's own examples.

- [x] B359: **the translate listing hid a target whose destination existed.**
  `options_for` swallowed every failed plan. With `money.ts` on disk the listing offered
  four languages, teaching the reader the fifth pair did not exist. A blocked target is
  listed with the reason now. `--out` and `--force` are the two ways past it, on all three
  translation paths. The imperative-pair refusal text also still denied the transpiler
  exists; it names the missing reader or writer now.

- [x] B360: **a rejected edit said only that the result would not parse.** Guesswork.
  The rejected text was gone before anyone could look at it. The refusal now names the
  line and column where the result stops parsing, and prints the lines around it.

- [x] B361: **`fr imports` took one file where every other sweep takes the workspace.**
  Odd one out. `unused`, `duplicates` and `parse` walk the tree; `imports` demanded a
  path. With no file it now organizes every file the index holds, in one atomic apply.
  Every skipped file is counted and the reason printed, because a silent skip reads as
  coverage.

- [x] B362: **applying an edit into a directory that does not exist failed at the
  staging step.** Late. `--out drafts/m.ts` planned fine and then could not stage. A
  relative `--out` also resolved against the process directory. The writer creates the
  destination directory now, and the flag resolves like every other path, against
  `-C`.

- [x] B352: **clicking a node in the call graph landed on the indentation.** The drawing
  carried each function's line and no column. So the click put the cursor at column 1. The
  status bar then read "nothing the index knows at this position", and every action
  refused. `graph_around` carries the column of the name now. Found by clicking one.

- [x] B351: **the fixed-defect archive was 31,000 words.** 333 entries, median 85 words,
  for defects closed and gone. An entry needs its symptom and its fix, and git holds the
  rest. Entries below B300 keep the symptom line alone, and the file is 9,000 words now.
  This entry was written once and lost before the commit, which is its own small lesson.

- [x] B350: **the call graph tab never drew anything.** `graph_around` has two checks
  there now. One asks the shape of an answer, the other the shape of a refusal.

- [x] B349: **the graph pane was on screen at all times.** `.graph-view[hidden] { display:
  none }` settles it.

- [x] B348: **PLAN.md kept a copy of the capability matrix by hand. It had drifted.** `fr
  capabilities` computes the table from the code, and `README.md` carries the generated
  copy, so the hand-written one is gone.

- [x] B347: **`fr usages` left out the places a name appears in prose.** One scan now lives
  in `src/mentions.rs`, all three call it. `Usages` carries the mentions apart from the
  references.

- [x] B346: **the browser could not draw the call graph.** The walk itself is
  `CallGraph::neighbourhood`, which a test can reach without a browser.

- [x] B345: **the dispatch wording explained nothing.** The message now names the
  implementations a call could reach. It also says the program chooses one while it runs.

- [x] B344: **a doc comment for one function sat on top of another.** Found while rewriting
  the comments in that file.

- [x] B343: **the prose in this repository was written in a machine's voice.**
  `docs/terminology.md` holds the terms.

- [x] B342: **the site could not be published for two days. Every run said `cancelled`.**
  `cancel-in-progress: true` now, so the newest deploy takes the slot.

- [x] B341: **`fr flow` sent three languages to an analysis that has no arm for them.** The
  test asserts the rule now. No language is offered both.

- [x] B340: **the browser never routed to provenance. So it answered questions the CLI
  could.** Both bindings route the same way the CLI does now.

- [x] B339: **`fr remove-flag` told the reader to do something the command could not do.**
  The resolved name is fixed before the cascade starts. Everything downstream looks the
  flag up by name: which uses are left, which imports were orphaned, what each round is
  called.

- [x] B338: **"move it somewhere under `src/`" led to a second refusal.** The first now demands
  a destination the module tree already declares. The second names the line to add.

- [x] B337: **provenance's refusal named a library module.** No reader of a CLI or
  browser message can run "Use analysis::flow (backward/forward) instead". It names
  `fr flow`.

- [x] B336: **the compile gate passed whether the tool worked or refused.** Each site now
  says which outcome it expects. `must_plan` covers a site that compiles, `must_refuse` a
  site that declines, with the reason.

- [x] B335: **licence and provenance checks passed when they checked nothing.** All three
  count what they examined. A count of zero fails.

- [x] B334: **nine refusals wrote their own article. Four kinds start with a vowel.** A test
  asks every kind at once and not the four that happen to be wrong today.

- [x] B333: **`fr type` and `fr flow` answered for nine languages the matrix disclaims.**
  Both now refuse by name. The language list lives in one place each.

- [x] B332: **the matrix disclaimed two capabilities the tool has.** One predicate now lists
  the arms the command has. 272 of 384 pairs are supported.

- [x] B331: **`fr remove-flag` wrote XML that no parser accepts.** The command asks
  `supports_cascade` now, the same predicate the matrix publishes. It refuses by name.

- [x] B330: **the scale sweep measured whatever `web/src/wasm` happened to hold.** The sweep
  now compares the artifact's timestamp against the newest `.rs` file. It refuses to run
  when the artifact is behind.

- [x] B329: **the scale sweep decided what counted as a refusal by reading the sentence.**
  The browser API reports `refused` from the type now. The prose no longer decides.

- [x] B328: **four more refusals blamed a language for a path.** `because` is `&'static str`
  now.

- [x] B327: **`fr move` told a Rust user that Rust was unsupported.** The matrix audit found
  it. A claim and a refusal cannot both be right.

- [x] B326: **`fr delete` left the import its deleted code was the only user of.** The
  result parses either way. So the parse sweeps never saw it.

- [x] B325: **`fr imports` never narrowed a statement that lost one of its names.** It
  narrows now. It cuts the dead names' clauses out of the statement and leaves the rest
  as written. Each language spells the list differently, so the separator is the only
  part worth understanding.

- [x] B324: **`fr remove-flag` left the imports its collapsed branch had been using.** That
  command knows about uses no query can see. A Rust trait reached through its methods,
  a JSX pragma in a comment.

- [x] B258: **closed unreproducible, with the evidence.**

- [x] B323: **a Java statement declaring several names gave each of them all three.** The
  query captured the statement. It captures the declarator now, which the symbol is.

- [x] B322: **`fr type` could not read a Java call or construction.** The omission
  repeats. It once made `fr signature` refuse at every Java call site there has
  ever been. A second place had not heard.

- [x] B321: **`fr type` answered `var`.** It falls through to inference now, which is what
  the keyword asks for.

- [x] B320: **`fr inline` refused every Java local.** Java puts the name and the value
  in one declarator, since a statement may declare several. So the value hangs off the
  declarator rather than the declaration.

- [x] B319: **three readers answered "what does this declaration bind". Disagreed.** There
  is one reader now, `parse::declaration_value`, and `tests/declaration_values.rs` names
  every shape it has to know.

- [x] B318: **`fr move` left the destination reaching the symbol through its old file.**
  A Zig call's qualifier says the same thing differently. The move drops it now.

- [x] B317: **`fr signature` reported call sites it had not touched.** The two are told
  apart now: no argument list still means no parentheses. A grammar that wraps nothing
  is read on its own terms.

- [x] B316: **a Zig `@import` path resolved to nothing.** So `fr rename` rewrote the
  declaration. That left every caller naming something that is not there.

- [x] B315: **a Java static call resolved to the wrong method, at exact confidence.** The rule now
  covers every language. A receiver naming a type this workspace declares is a path.

- [x] B314: **the confidence cap and the rule that resolves a qualified call disagreed.**
  Both places now ask one question in one place.

- [x] B313: **`Language` ignored a width in a format string.** Its `Display` wrote the
  name straight out instead of going through `Formatter::pad`. So `{target:<10}` padded
  nothing, and the column of targets `fr translate` prints came out ragged wherever a
  `Language` sat in it.

- [x] B312: **`fr translate` offered a list that was not true.** `tests/translate_sweep.rs`
  holds the list to its word from both sides.

- [x] B311: **`fr move` left a star re-export naming a file the symbol had left.** Removed
  outright where the move takes the last export. TypeScript calls a file with no exports
  "not a module", and rejects the star for that instead.

- [x] B310: **`fr move` dropped the names beside the one it repointed.** One name survived.
  `export { width, Holder } from "./holder"` came back as
  `export { width } from './util'` with `Holder` gone.

- [x] B300: **`fr move` declined at a re-export barrel.** Now says why: readers write
  `ns.width`. Splitting the module in two cannot be followed by repointing one
  statement.

- [x] B309: **`fr usages` and the reference index disagreed. The check could not see it.**
  Those are what it checks now.

- [x] B308: **a Go call into another package resolved to nothing.** The import statement
  names the package. So which declaration a qualified call means is written down; resolution
  now reads it.

- [x] B307: **`fr move` wrote import cycles that neither Go nor Python accepts.** Both are
  refused now. The refusal names the two files and what each would import.

- [x] B306: **`fr move` wrote a relative import into a file in no package.** It writes
  relative inside a package, absolute outside.

- [x] B305: **`fr remove-flag` deleted a declaration whose readers it had refused.** A
  cascade that changes nothing now refuses, carrying the reasons. It no longer returns a
  plan of zero edits.

- [x] B304: **`fr remove-flag` replaced the callee of a call instead of the call.** A documented
  rule that never runs repeats B296's defect shape.

- [x] B303: **`fr remove-flag` wrote a boolean into a type position.** One use in a
  position only a type can occupy settles the name. The whole operation is refused,
  naming that use.

- [x] B302: **`fr remove-flag` treated every constant as a possible flag.** A sweep found
  234 asks that produced a plan. It ran over every name in the vendored corpus, both
  values. One was `const DocumentScope = @import("DocumentScope.zig")`, rewritten to
  `*const true`.

- [x] B301: **`fr restructure` rewrote files when asked for no change.** Eight identity
  rewrites over `src/`: eight changed files before, none now.

- [x] B300: **a use reached through a re-export barrel resolved by name alone.** Repointing
  an export is a different operation from repointing an import. The move says so instead
  of writing both faults.

- [x] B263: **a Terraform input variable and a local sharing a name were one symbol.**

- [x] B299: **a CSS class and an element id sharing a name were one symbol.**
  `Reference::expects` holds it.

- [x] B298: **four reports stopped early and said nothing.** Each one now states how many it
  left out.

- [x] B296: **`fr rewrite guard-clause` wrote `return;` in a function that returns a
  value.**

- [x] B297: **`fr extract` placed a binding where the names in it do not exist.**

- [x] B292: **`fr move` imported the symbol it had just moved.** `Symbol::is_top_level` now
  asks about both.

- [x] B293: **`fr move` wrote a workspace it knew would not build.**

- [x] B294: **an import path resolved to a method of the same name.**

- [x] B295: **a call inside a macro was ambiguous with a method of the same name.**

- [x] B290: **a bare Rust call resolved to a method or a field.**

- [x] B291: **a dotted name inside a macro resolved to a free function.**

- [x] B288: **`fr move` refused every move in this workspace, over a doc comment.**

- [x] B289: **`fr move` wrote `use crate::…` into files that are not in the crate.**

- [x] B287: **`fr imports` moved an import out from under its `#[cfg]`.**

- [x] B284: **`fr inline` refused on any name reused elsewhere in the file.**

- [x] B285: **`fr inline` panicked on a declaration longer than one line.**

- [x] B282: **JavaScript files were not source files.**

- [x] B281: **a link to a heading resolved to nothing. Renaming the heading broke it.**

- [x] B280: **one SCSS interpolation cost every fact below it in the file.**

- [x] B279: **a Helm action in key position left the entry out of the index, silently.**

- [x] B278: **Helm masking produced YAML that does not parse, four ways.**

- [x] B277: **the language filter had two names.** The docs used both and now use one.

- [x] B276: **`fr duplicates` crashed on a multi-byte character.**

- [x] B275: **`fr duplicates --json` reported a language spelling no other command uses.**

- [x] B274: **`fr duplicates` gave lines and no columns.**

- [x] B273: **`fr unused` named a symbol and would not say where it was.**

- [x] B271: **the published site shipped HTML the tool cannot parse.**

- [x] B272: **the same gap one node deeper: a type named by its path.** Rust was alone in
  both.

- [x] B270: **a method of a generic type was not a method of anything.**

- [x] B269: **`Refusal::Unsupported`'s `language` field held a language in five of fifteen
  cases.**

- [x] B268: **five more refusals reported a resolution that had not happened.**

- [x] B267: **a remedy was offered where it would not work.**

- [x] B266: **an argument the shell decides at run time was reported as weak resolution.**

- [x] B265: **a signature change refused by talking about renaming.**

- [x] B264: **`zig-test` matched a test's description, not the construct.**

- [x] B262: **`fr unused` reported HCL blocks Terraform gives no address to.**

- [x] B261: **two capability predicates returned `true` for every language, behind branches
  that could not run.**

- [x] B260: **three commands took two or three bare booleans in a row.**

- [x] B259: **line and column travelled as a bare `(usize, usize)` in six places.** All six
  return `LineCol` now.

- [x] B256: **`fr unused` did not treat an HTML attribute value as a string.**

- [x] B255: **`fr unused` reported containers of entry points.** A dead method beside a live
  test still reports.

- [x] B254: **`fr unused` reported JavaBean accessors reached by their property.**

- [x] B253: **three Spring conventions missing from the catalogue.**

- [x] B252: **`fr unused` reported every package clause, one per file.**

- [x] B257: **`fr unused` printed a count without a breakdown.**

- [x] B251: **a recipe's misspelled predicate value blamed the repository.**

- [x] B250: **three matcher conditions did not count as conditions. An empty matcher matched
  nothing without saying so.**

- [x] B249: **`fr type --json` answered with numbers nobody can use.** Both now say the same
  thing.

- [x] B248: **`as_str` named two different things.** The display three are now `label()` and
  `describe()`.

- [x] B247: **the tool's JSON could not be read back into the tool's own types.**

- [x] B246: **a misspelled value in a catalogue loaded and matched nothing.**

- [x] B245: **the same overclaim, one branch up. B243's fix did not reach it.**

- [x] B243: **a member access claimed to know a receiver it had never seen.**

- [x] B244: **the list of commands named 28 of 32.**

- [x] B242: **`fr type --help` said the command does not do what the command does.**

- [x] B241: **passing locally and passing in CI meant different things.**

- [x] B240: **entry-point detection read every file once per rule.** The cache cannot change
  an answer: a miss re-reads.

- [x] B239: **a decorator's name is not unique across libraries.**

- [x] B238: **a modifier between the annotation and the declaration ended the run.**

- [x] B237: **a dot in an annotation's arguments hid the annotation.**

- [x] B236: **route handlers, queue consumers and scheduled jobs were dead code.**

- [x] B235: **a Next.js server action was dead code.**

- [x] B230: **a parse failure said how many and never where.**

- [x] B229: **a Go type implemented an interface it does not implement.**

- [x] B228: **the tool printed names it would not accept.**

- [x] B227: **three tests counted what came back and never looked at it.**

- [x] B226: **a test named for path order never checked the order.** The order is asserted
  now.

- [x] B225: **the entry-point coverage report was checked for having names in it.**

- [x] B224: **the cache's own claim was tested for stability and not for meaning.**

- [x] B223: **`fr duplicates` named its threshold only when it found nothing.**

- [x] B222: **the published site was three commits behind and said nothing.**

- [x] B221: **`fr type` answered half the question.**

- [x] B220: **the published site was checked by hand and by nothing else.**

- [x] B219: **`fr impact` reported a bounded search as a complete answer.**

- [x] B218: **every hop of a forward flow was printed twice.**

- [x] B217: **forward flow stopped at the first hop in Rust.** It keeps walking now.

- [x] B216: **a value was reported as flowing into the function around it.**

- [x] B215: **a constructor body that builds and returns was thrown away for three
  targets.**

- [x] B214: **an enum variant was read as a record.**

- [x] B213: **a Rust struct literal was not read at all.**

- [x] B212: **a Zig method that changed its own object did not compile.**

- [x] B211: **the Java output named types the file had never imported.**

- [x] B210: **joining two strings produced Zig that does not compile.**

- [x] B209: **the Zig output named a standard library it had never bound.**

- [x] B208: **comparing two strings meant something else in Java. Nothing at all in Zig.**

- [x] B207: **`a %% b` on two integers meant something else in Python, silently.**

- [x] B206: **`a / b` on two integers became float division in Python.**

- [x] B205: **a Rust function's tail expression translated to nothing.**

- [x] B204: **every translation dropped the brackets.**

- [x] B203: **the grouping nearly went into CSS.**

- [x] B202: **`fr restructure` changed what the code computes.** Both halves are the defect
  already fixed twice in `inline`.

- [x] B201: **a move left behind what the moved code needed, in every language with its own
  move path.**

- [x] B200: **deleting a lone Java field deleted the class around it.**

- [x] B199: **`fr remove-flag` never worked on a TypeScript file.**

- [x] B198: **extracting a region that awaited produced code that does not compile.**

- [x] B197: **extracting a region containing `yield` silently did nothing.**

- [x] B196: **an expansion of two bracketed halves was left unbracketed.** The check
  balances the brackets now.

- [x] B195: **`fr inline --call` changed what the program computes, in all seven
  languages.**

- [x] B194: **a pytest fixture and a `unittest` fixture matched no rule.**

- [x] B193: **a Python script with a `__main__` guard reported no entry point.**

- [x] B192: **a type argument was read as a supertype.**

- [x] B191: **class hierarchy analysis skipped Java.**

- [x] B190: **the number describing the matrix was not checked, and had drifted.**

- [x] B189: **a shell function was told it needs a return type and modifiers.**

- [x] B188: **the capability row for `fr stitch` was a transcription of the accessor table.
  It had drifted from it.**

- [x] B187: **`fr stitch` could not see a Java or Zig program read its configuration.**

- [x] B186: **a reference in an argument position could be mistaken for the call.**

- [x] B185: **a constructor's parameters were reordered and every `new` left as it was.**

- [x] B184: **`fr signature` refused at every Java call site there has ever been.**

- [x] B183: **the imports a moved symbol needs were written above the code and not where
  imports go.**

- [x] B182: **moving a symbol into a file that imported it left the import behind.**

- [x] B181: **an `if` that binds what it tested could be inverted.**

- [x] B180: **Zig fell into the C arm of the boolean spelling table.**

- [x] B179: **`invert-if` negated half a condition and swapped the branches anyway.**

- [x] B178: **a shorthand object property refused the whole object.**

- [x] B177: **`??` had no counterpart in the IR**

- [x] B176: **a query parameter did not survive the crossing.**

- [x] B175: **the contract could only be derived from one side of the crossing.**

- [x] B174: **a handler's inline `context.params.petId` was left naming an object Python
  does not have.**

- [x] B173: **the contract had no query parameters at all.** Read from
  `searchParams.get("…")` now.

- [x] B172: **the contract listed schemas that nothing referred to.**

- [x] B171: **a zod schema declared in another module was invisible.**

- [x] B170: **removing a parameter the body still reads.**

- [x] B169: **extracting an expression that *is* its statement left a statement that only
  names the binding.**

- [x] B168: **`inline` refused every Zig binding there has ever been**

- [x] B167: **`inline` changed what the code does.**

- [x] B166: **an overload set was resolved by proximity, at `Exact`.**

- [x] B165: **a rename that left a call behind said nothing at all.**

- [x] B164: **a member access was resolved through the lexical scope chain.**

- [x] B163: **the rename collision guard was file-scoped. It was not scope-scoped.**

- [x] B162: **a Go type's recursion was not its entry point.**

- [x] B161: **a Rust reference was stripped after the containers were checked**

- [x] B160: **`readonly string[]` was read as an array of `readonly string`.**

- [x] B159: **every Zig type read from text was read wrong.**

- [x] B158: **the Zig reader required a named node after `=`**

- [x] B157: **the Python reader would not read back what the Python writer writes.**

- [x] B156: **the round trip checked functions and not data.**

- [x] B150: **the methods of every generic Rust type became free functions.**

- [x] B149: **a constructor had no counterpart in the IR at all.**

- [x] B148: **a constructor's own name claimed a spelling in the naming map.**

- [x] B147: **a Rust raw identifier grew an `r` every time it crossed.**

- [x] B146: **Python's `self` was stripped from free functions too.**

- [x] B145: **a `@staticmethod` disappeared from its class.**

- [x] B144: **every reader's record member loop ended with `_ => {}`.**

- [x] B143: **there was no round-trip check at all.**

- [x] B142: **a note was reported only when something else had gone wrong.**

- [x] B141: **a base class was dropped without a word.**

- [x] B140: **there was no conditional expression in the IR**

- [x] B139: **`_` was put through the naming convention.** A rename that produces nothing is
  not a rename.

- [x] B138: **a Zig `comptime` parameter was read as an ordinary one.**

- [x] B137: **a Zig destructuring kept the first name and dropped the rest.**

- [x] B136: **Zig optionals and pointers were never read.**

- [x] B135: **a Rust raw or byte string was not read as a string.**

- [x] B134: **a parse error with no position reported none at all.**

- [x] B132: **a comment inside a parameter list was read as a parameter.**

- [x] B131: **every string escape was doubled on every crossing.**

- [x] B130: **a method was written as a free function whose body reached through a receiver
  nothing bound.**

- [x] B129: **a method with no receiver was written as one with a receiver.**

- [x] B128: **a multi-line comment got its marker on the first line only.**

- [x] B127: **a doc comment could end itself early.**

- [x] B126: **`0usize` was carried into every target.**

- [x] B125: **a Rust tuple struct silently lost its payload.**

- [x] B124: **`let _ = f();` declared something with no name.**

- [x] B123: **a TypeScript class member is public unless it says otherwise. Every one of
  them was read as private.**

- [x] B122: **Python's `x = 1` is a declaration the first time and an assignment every time
  after. All of them were read as declarations.**

- [x] B121: **the receiver had six names and the IR recorded none of them.**

- [x] B120: **`self` is the one keyword Rust refuses to raw-escape.**

- [x] B119: **Go's `error` is Zig's keyword for an error set**

- [x] B118: **the Zig reader read named children only. In that grammar the `:` before a
  type, the `=` before a value and every operator are anonymous.**

- [x] B117: **a `for` over two sequences. An `if`/`while` that unwraps an optional, were
  read as if they were the one-binding form.**

- [x] B116: **Zig rejects a `var` nothing writes to.**

- [x] B115: **Zig has no block comment**

- [x] B114: **the ordered-pair translation test covered sixteen of twenty pairs and asserted
  twelve.**

- [x] B113: **`generic()`'s path separator reads as an argument separator.** Renamed to
  `path_separator`.

- [x] B112: **Java was missing from the transpiler's reserved-word table**

- [x] B111: **a Java catch clause lost both its exception type and its binding.**

- [x] B110: **`d[k] = v` translated into Java as `d.get(k) = v`,**

- [x] B109: **the entry-points reason called YAML a stylesheet.**

- [x] B108: **`fr remove-flag` refused every Java flag, and then refused to fold it.**

- [x] B107: **`fr imports` told a reader that Bash "has no import statements to organize"**

- [x] B106: **`fr translate` and `fr openapi` were missing from the capability matrix.**

- [x] B105: **`fr translate` denied a capability the tool has.**

- [x] B104: **the browser scale sweep covered fourteen of sixteen languages while claiming
  all of them.**

- [x] B103: **the playground's own UI said "fifteen languages"**

- [x] B102: **two pages disagreed about the same measurement.**

- [x] B101: **the README's status section stated a fixed bug as current, twice over.**

- [x] B100: **the bundled playground sample had no Java file,** A private method that is
  genuinely dead.

- [x] B99: **`annotated_with` only looked *above* a definition.**

- [x] B98: **the capability table claimed `inline --call` for every imperative language.**
  Both fixed; `inline::supports_call` is now the authority.

- [x] B97: **the capability table and `move` disagreed about Java.**

- [x] B96: **six capability reasons were false about Java.**

- [x] B95: **a recipe computed both workspace analyses whether or not an expectation asked
  for either.**

- [x] B94: **a recipe step rebuilt the whole index after every subject.** Same result, 48
  seconds.

- [x] B93: **`rewrite` treated a file it had nothing to do in as a refusal.**

- [x] B92: **applying a micro-rewrite across a file asked at every byte offset.**

- [x] B91: **`fr signature move` could produce Python the interpreter rejects.**

- [x] B90: **every Go function body was carried into a translation as a single comment.**

- [x] B89: **the recipe runner planned each step against the file on disk.**

- [x] B88: **the recipe runner planned every selected symbol against one snapshot.**

- [x] B87: **the recipe report dropped the warnings its steps produced.**

- [x] B86: **a Java method call resolved to nothing.**

- [x] B85: **`fr signature` and the CLI each had their own copy of the change parser,**

- [x] B84: **a bare `xs.filter(p)` did not translate. A comprehension that kept every
  element it selected wrote out an identity `map`.**

- [x] B83: **inlining a variable was refused whenever any name in its value appeared
  anywhere else in the file.**

- [x] B82: **`fr signature X 'add:1:flag: bool:false'`, the example in the tool's own error
  message, did not work.**

- [x] B81: **the catalogue page's report pane dropped three quarters of what the tool
  said.** Split by position instead.

- [x] B80: **`commit` chose how to write by feature flag and not by where the writes go.**

- [x] B79: **`src/wasm.rs` could not be compiled without a wasm toolchain. So every edit to
  the browser API was checked only by CI.**

- [x] B78: **a foreign name that is a keyword in the target made the whole file
  unwritable.**

- [x] B77: **Python's `*`, `/`, `*args` and `**kwargs` were read as ordinary
  parameters.** `def create_user(*, session, user_create)` produced
  `export function createUser(*: unknown, …)`, which TypeScript will not parse, caught
  by the translator's own parse check, on real code, in a file 1,300 fixture tests had
  never seen. A `*` is a rule about the parameters around it. Dropping it silently
  would be worse: the signature would look carried when the way callers must invoke it
  had changed. `ParamKind` now models all four and `signatures_with_changed_calls`
  counts the difference.

- [x] B76: **an optional chain was written away.**

- [x] B75: **a TypeScript type assertion became `None`.**

- [x] B74: **comments were reported as untranslatable constructs.**

- [x] B73: **`try`/`catch` had no counterpart in the IR. So whole handler bodies came out as
  one comment.**

- [x] B72: **the Python writer decided "did I write anything" in each match arm.**

- [x] B71: **the naming convention was applied at declarations and not at uses.**

- [x] B70: **the Next.js route matcher required a leading slash. So no relative path was
  ever a route.**

- [x] B69: **`await` was not in the IR. So every line containing one was carried verbatim.**

- [x] B68: **the Next.js translation counted handler signatures as failures and overwrote
  the helper count.**

- [x] B67: **the Next.js translation printed a Rust `Debug` dump where the source should
  have been.**

- [x] B65: **a template metavariable the pattern never bound was caught by the wrong
  check.**

- [x] B66: **`?repo=` picked the workspace for the JSON renderings and was ignored by the
  page.**

- [x] B64: **a Rust method call resolved to a Zig method. A rename rewrote it.**

- [x] B63: **`fr refs` under-reported for anything declared more than once.**

- [x] B62: **a rename buried its success under twelve thousand warnings.**

- [x] B61: **an edit re-parsed every file in the workspace.** zod 3144ms → 624ms, ripgrep
  860ms → 149ms.

- [x] B60: **a file a refactoring created was never indexed.** The move reported success.

- [x] B59: **a message rendered the workspace root as nothing.**

- [x] B58: **the coordinate button claimed a copy that had not happened.**

- [x] B57: **the status bar reparsed the open file on every keystroke.** 3ms.

- [x] B56: **`DefinitionRole` serialised as a Rust variant name.**

- [x] B55: **`fr unused` named symbols `fr delete` could not remove.**

- [x] B54: **a CSS class used by the markup was reported as dead.**

- [x] B53: **the browser reported symbols dead that the terminal reported live.**

- [x] B52: **two workspaces in one page shared one set of bytes.**

- [x] B51: **`Path::exists` bypassed the virtual filesystem.** `tests/vfs_choke_point.rs`
  now fails the build on a new one.

- [x] B50: **a call that resolved to an interface method reached no implementation.**

- [x] B49: **`fr implementations` answered nothing for an interface.**

- [x] B47: **a new import was written inside a multi-line import statement.**

- [x] B48: a moved Python symbol left its module imports behind. `import os` binds
  `os` without naming it in the statement. So the name-based check that carries named
  imports never matched it, and the moved code lost `os.path`. Also carried now:
  `from __future__ import annotations`. It binds nothing at all and decides how
  every annotation in the file is read. Without it, `str | None` stops parsing below
  Python 3.10. It is placed first, where the language requires it.

- [x] B46: **a guard clause exited the wrong construct.**

- [x] B45: a statement pattern was impossible in Python, shell and YAML. Those
  languages wrap a `fr restructure` fragment in nothing, so the statement the pattern
  writes is the outermost node. The descent that strips wrapper-introduced
  statement containers stripped that one too. The fragment then started six bytes
  inside itself, and every such pattern was rejected as "not a valid fragment". Descending
  is only correct when the child begins where the container does; `raise` does not.
  `fr restructure 'raise InvalidURL($X)' 'raise InvalidURL($X) from None'` now works
  on psf/requests.

- [x] B44: **a Terraform traversal ignored its namespace.**

- [x] B41: **the cache reused facts produced by a different extractor.**

- [x] B42: Rust reached nothing through a path. `super::render_custom_markup(…)` and
  `Patterns::from_low_args(…)` both resolved to nothing, because the prefix of a
  `scoped_identifier` was never recorded. References now carry it, flagged as a path
  instead of a value. A path names a type or a module. The resolver matches it against
  a symbol's own qualifier with no type inference, since the source wrote the type down.
  This rule runs before every other: ripgrep declares four `from_low_args` methods in
  one file. So the nearest-in-file rule would otherwise pick whichever sat closest and
  leave the other three looking dead.

- [x] B43: a Rust test looked like dead code. Tests declare themselves with `#[test]`,
  and the entry-point catalog could only match names and paths, ripgrep's are called
  `backslash`, `tab` and `carriage`. Catalogs gained `annotated_with`, which reads the
  annotations immediately above a definition, and Rust gained rules for `#[test]` and
  `#[bench]`. Detected test entry points in ripgrep went from 141 to 516, and its
  internal dead-code report from 643 findings to 317.

- [x] B40: **`fr extract` put a Go binding above the declaration it read.** All three copies
  are now one.

- [x] B39: **a Helm values key could not be renamed.**

- [x] B35: **`--path` filters matched nothing, and reported that as nothing found.**

- [x] B36: a relative path in a target was read from the shell's working directory
  instead of the workspace `-C` names. So `fr -C ../helm refs pkg/x.go:3:6` failed
  with "reading pkg/x.go: No such file". Four sites had their own
  `canonicalize().unwrap_or(…)`, which kept the unusable path and let the failure
  surface two frames later. They now share one resolver that says where it looked.

- [x] B37: a field access resolved to a local variable. `i.provData` bound to a
  `provData, err := …` two lines up. The nearest-definition rule ran before
  anything checked that a member access can only name a member. The field then had no
  references at all and was reported as dead.

- [x] B38: nothing tested the command line. Every test called the library directly.
  B35 and B36 both lived entirely in the layer between an argument and that
  library, so nothing saw them. `tests/cli.rs` runs the binary: argument parsing, path
  resolution, exit codes, and the text a person reads.

- [x] B26: **Go resolved nothing across files in a package.**

- [x] B27: a method call resolved to a package-level function of the same name.
  `w.contextWithTimeout(…)` and `time.Now()` are one syntax in Go, and the grammars
  capture only the callee, so nothing separated a member from a package-qualified
  call. References now record the receiver they were written against, and an import
  binding before the dot is what tells the two apart. Without it the method read as
  dead while the function absorbed its call sites.

- [x] B28: **file proximity decided which method a call meant.**

- [x] B29: a binding resolved inside its own initialiser. helm's
  `templatesDirExists := run(…, templatesDirExists(path))` calls the package function
  and *then* shadows it. Resolving the call to the variable being declared made the
  function look dead. The rule holds in Rust (`let x = x + 1`) and Python (`x = f(x)`)
  as well. All three apply it now.

- [x] B30: a use bound to the nearest declaration in either direction. Go re-declares
  with `:=` mid-function. In helm, `var ret …` / `return ret` / `ret, err := …`
  bound the early return to the *later* binding. It sat 15 bytes closer. Value
  bindings now prefer a declaration above the use; a function may still be called
  above where it is written.

- [x] B31: a package may declare one name twice under opposite build tags,
  `//go:build windows` and `//go:build !windows`. Resolution picked the first and
  reported the other as dead; picking one would rewrite half a pair and break the
  other build. Both are now reported as ambiguous and spared.

- [x] B32: **the public API of a library rooted nothing.**

- [x] B33: two types sharing a private method name made both look dead, because the
  call resolved to neither. They are now spared with that stated. Where the hierarchy
  analysis has already ruled on the name, its answer is the more precise one and
  stands.

- [x] B34: `fr unused` had no way to narrow its report. On a polyglot repository every
  Markdown heading drowned the code findings. `-C` could not be used to narrow
  because a smaller index invents dead symbols instead of hiding them. Added
  `--lang`, `--path` and `--internal`, which filter the report and not the index,
  with an unknown language name refused against the known list.

- [x] B16: micro-rewrites were published for seven languages and tested on three.
  `invert-if` and `guard-clause` negated the whole condition node, which in the C
  family and Zig *includes the brackets*. So both emitted `if !(a)`, valid Rust,
  a syntax error in TypeScript, TSX and Zig. Zig failed earlier still: its grammar
  calls the consequence `body`, not `consequence`, so no part of the `if` was found.
  The fix negates the expression inside the condition and splices within it, so
  whatever the grammar writes around it survives. `guard-clause` now builds its
  header from the source's own bytes instead of reinventing one per language.
  Found by running the tool on grafana/grafana, where 63 of 65 real if/else sites
  now invert cleanly and both refusals are genuine `else if` chains.

- [x] B17: **`guard-clause` silently changed what Go programs do.**

- [x] B18: `invert-if` accepted an `else if` chain and produced unparseable output.
  The second condition is only tested when the first is false. So swapping the
  branches changes which tests run; it is now refused with that reason. Also fixed:
  `else_body_of` returned the whole `else` clause when it did not recognise the body
  shape, splicing the `else` keyword into the consequence position.

- [x] B19: **de Morgan dropped the grouping its own result needs.**

- [x] B20: `extract --function` emitted `function helper(x: : number)` for TypeScript.
  The C-family grammars fold the `:` into the annotation node, and the renderer added
  another. The type is now read bare and each language spells its own punctuation, so
  Go gets `x int` and not `x: int`. Its call site loses the C semicolon, and
  `gofmt -d` reports no diff on the result.

- [x] B21: **a move produced files that parse and do not compile.**

- [x] B22: a move wrote imports like `'../../../../../../../var/folders/…'`. It
  happened when the destination was spelled differently from the indexed path: a
  relative path, or `/var` where the index holds `/private/var`. In the relative case
  it silently added no import at all. `canonicalize()` had failed on a file that does
  not exist yet and the result was passed through unchanged. The destination is now
  resolved through its parent directory, and a missing directory is an error. The move
  itself had a matching silent skip: a file needed an import, did not get one, and the
  run reported success. That is a failure now.

- [x] B23: the index records one `Import` per imported name, each carrying the whole
  statement's span, so a four-name import read as four statements. Anything rewriting
  import statements has to regroup them first, and a move did not: it emitted the
  same statement once per used name.

- [x] B24: generated code was indented four spaces regardless of the file. Two-space
  TypeScript and tab-indented Go both received four, on every guard clause and every
  extracted function. One level is now read from the source.

- [x] B25: `fr unused` listed every `_`-prefixed parameter. Rust, TypeScript, Python
  and Zig all use that prefix for a binding a signature forces and the body ignores.
  One real file contributed eight. They are now spared with that stated reason rather
  than dropped quietly.

- [x] B10: Helm values precedence stopped at the command line. Whether a
  `values-*.yaml` is passed with `-f`, the order of several `-f` files, every
  `--set`: all were invisible and reported undecided. A missing input caused it,
  not a limit. `fr flow back <target> -f values-prod.yaml --set a.b=c` supplies the
  invocation. Helm's order then decides it: chart `values.yaml` < each enclosing
  parent chart < each `-f` in the order given < `--set`. The winner is marked, and
  every loser stays listed, including a values file the caller says is *not* passed.
  With nothing supplied the answer is what it was.

- [x] B12: Terraform lost the third and later step past an index traversal. A query
  cannot say "every sibling after this one". So each step needs its own pattern; six
  are now written, which is far past anything Terraform expresses. A test asserts
  the bound so it stays a decision instead of an accident.

- [x] B0a: `LineIndex` invented a phantom trailing line for files ending in a newline,
  `src/span.rs`. So `"a\nb\n"` counted 3 lines, and an EOF offset reported a column
  past the last character. Fixed: a trailing newline terminates the final line;
  columns clamp to the line end.

- [x] B0b: `.gitignore` was ignored outside a git repository, so scans of worktrees
  and exported trees walked `target/`, `node_modules/` etc, `src/scan.rs`. Fixed with
  `WalkBuilder::require_git(false)`.

- [x] B1: SCSS was parsed with the plain CSS grammar, so `$variables`, `@mixin`,
  `@include` and `@use` were all parse errors. Fixed at the root by adding the
  `tree-sitter-scss` grammar. A test asserts the CSS grammar still rejects SCSS
  syntax, so the split is real and not cosmetic.

- [x] B2: a Helm template action in a structural position yielded a YAML tree
  reflecting no single rendering. Fixed for the analyses that reason about values: a
  key wrapped in `{{- if }}` now produces a stop naming the exact condition. The
  condition's own `.Values` key resolves. Masking itself is unchanged by design. It
  keeps byte offsets valid, so the symbol index still shows guarded keys
  unconditionally. Only provenance and stitch consult the guards.

- [x] B3: deleting a CSS selector left its `{ ... }` block orphaned. The delete widens
  the selector's span to the whole rule when the selector is alone on it. Where the
  rule has others, it widens to that selector and its comma.

- [x] B4: import liveness was name-based, so anything a language brings into scope
  invisibly looked unused. Per-language guards now hold back and report: Python
  `__future__` imports, `__all__` re-exports and dotted registration imports;
  TypeScript type-only imports, JSDoc `{Foo}` mentions, JSX pragmas and `typeof X`;
  Go blank imports and packages whose clause name cannot be derived from the path.
  Zig was verified to need none. Two real false positives fell out of it: Python
  `import a.b` binds `a`, not `b`, and `gopkg.in/yaml.v2` binds `yaml`, not `v2`.

- [x] B6: consecutive standalone Go `import "x"` lines were not sorted. The `import`
  keyword sits outside the `import_spec` span, so it looked like unrelated code
  ending the block.

- [x] B7: Helm `.Values` references lived inside masked actions and were invisible.
  Fixed by parsing the actions: paths resolve through pipelines, function arguments,
  `with` scopes, `$.` and into `define` bodies reached by `include`. Fields of a dot
  bound by `range`, values reached via `index .Values "a-b"`, and computed template
  names are named as unresolved and not resolved.

- [x] B8: Terraform splat traversals lost their trailing segments. `[*].id` and
  `.*.id` now capture every following attribute; B12 records what an index traversal
  still loses.

- [x] B9: `.tfvars` top-level attributes now produce `Key` symbols. So values files
  are in the index instead of needing provenance to walk the tree itself.
