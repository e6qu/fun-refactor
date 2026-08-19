# BUGS

Known defects and limitations, and their status. Updated alongside PLAN.md at every
stage.

Format: `- [ ] B<N>: <symptom> — <where> — <status/notes>`

Open entries are characterised limitations: the behaviour is reported and no operation
silently does the wrong thing.

Every open entry is pinned by a test, so a claim that stops being true fails a build
instead of sitting here. B11 said `@content` was a gap after it had stopped being one, and
nothing noticed. The eight grammar limits are pinned by `tests/known_grammar_gaps.rs`,
from both sides, the failing form and the neighbouring forms that work. The three that are
this tool's own behaviour are pinned by `tests/open_defects.rs`. Each asserts the
whole entry: what the tool does not do, and what it reports instead. Every one of these
stands on the second half. A test that checked only the first would pass just as well
if the report went away.

## Open

Re-triaged against this branch. Every entry below still reproduces; none was found to be
stale. Eight are limits of a published grammar. Each names the construct the grammar has no
rule for, at the version this build pins: `tree-sitter` 0.26.11, with
`tree-sitter-go` 0.25.0, `tree-sitter-python` 0.25.0, `tree-sitter-typescript` 0.23.2,
`tree-sitter-zig` 1.1.2 and `tree-sitter-scss` 1.0.0. The version is part of the claim: an
upgrade is what retires one of these, and `tests/known_grammar_gaps.rs` fails when it does.

- [ ] B283: `.sass` maps to `Language::Scss`, and the indented syntax is not SCSS. Sass
  has two syntaxes, the braced one in `.scss` files and the older whitespace-significant
  one in `.sass` files, and `tree-sitter-scss` implements the first. So a `.sass` file
  is scanned and then fails to parse. It is visible and not silent, so the
  mapping stays: removing it would make those files vanish the way `.js` files did.
  Pinned in `tests/known_grammar_gaps.rs`. `tree-sitter-scss` 1.0.0 implements the braced
  syntax only and has no rule set for the indented one. So this needs a second grammar
  and not a change to that one. What changed meanwhile: `fr parse` now prints the
  cause beside the positions, in text and in `--json` as `known_cause`. The error
  nodes no longer read as a syntax error to hunt for. The gap that remains is the
  grammar.

- [ ] B5: `find_unused` and the call graph follow class-hierarchy dispatch as well as
  resolved calls: a Rust `impl Trait for Type` (supertraits included), a Go interface
  whose method set a type covers by name and arity, a TypeScript `implements`/`extends`
  clause, and a Python base class each fan an unresolved method call out to every
  implementation, tagged `field-based`, counted apart from resolved edges by
  `fr graph`, and named as the reason a symbol was spared. TypeScript additionally
  falls back to matching the method name alone where no `implements` is written, which
  is unsound by design and labelled `method-name` and not `declared-supertype`.
  What remains is undecidable from the source, not unimplemented: a function held in a
  map, a struct field or a variable and called through it. Nothing declares it a
  method of any type, so there is no method set to look it up in. A name assembled
  at runtime from pieces no string literal spells. A symbol used only from a file that
  failed to parse is invisible for a third reason. `delete::plan` reports that file
  as possibly hiding uses. Zig (comptime duck typing) and Bash declare no
  implements-relationship at all, so neither has a hierarchy to read.

- [ ] B13: an answer from supplied values inputs is only as complete as the
  description of them. Given `--set` but no `-f`, or the reverse, the competition is decided *given the
inputs supplied* and says so. It names the channel it was never told about; nothing infers an invocation. Three narrower edges: `--set ports[0].name`
  and `--set ports[1].name` address the same key path, because the symbol index
  records mapping paths without list indices. `--set x=null`, which deletes a key in
  Helm, is ranked as a source that supplies it. And `{a,b}` list literals, `--set-file`
  and `--set-json` are refused by name and not half-applied.

- [ ] B14: a CSS class named inside a TSX helper call or template literal,
  `className={cx("btn", active && "on")}`, `` className={`btn ${size}`} ``, is not
  resolved, because only a plain string attribute value is captured. A rename of that
  class rewrites the plain `className="btn"` uses and leaves the helper ones. The
  textual sweep does report each missed site as needing review. So the result is
  incomplete and not silently wrong. Resolving them means teaching the TSX queries
  which call arguments are class lists, which is a per-library convention (`clsx`,
  `cx`, `classnames`, `cva`, `tailwind-merge`) instead of a language rule.

  Measured over grafana/grafana's 4,400 TSX files, `className` is written as:
  `styles.x` from CSS-in-JS 3,233 times, `cx(…)` 381, a plain string literal 224, a
  template literal 28. So the helper form outnumbers the resolvable one, and closing
  this would roughly triple the reach of a stylesheet-class rename in a modern React
  codebase. The CSS-in-JS majority is a different matter and out of scope: there is no
  stylesheet selector to link `styles.x` to.

- [ ] B11: SCSS forms `tree-sitter-scss` 1.0 cannot parse. Measured over
  `twbs/bootstrap`'s stylesheets, the canonical SCSS codebase: 73 of 99 files failed,
  and 59 do after B280 masked the first form below.

  The counts this entry used to carry were of files that *use* each form and fail.
That is co-occurrence, not cost: most of those files hit several forms at once. Masking one
  form at a time and re-measuring gives what each costs:

  * **Interpolation in a declaration value**, `color: #{$v}`, `--x: #{$v}`. Handled by
    masking (B280); the grammar still cannot read it. Worth masking because its error
    node runs to the end of the file instead of staying in the declaration. 14 files
    parse once it is masked. The facts recovered are far larger than that, symbols
    1916 → 2826, references 3839 → 6277. Interpolation in a selector (`.a-#{$x}`) and in
    a property *name* (`--#{$p}x`) both parse unaided.
  * **Empty parentheses**, on a declaration (`@mixin m()`) or a call (`@include m();`),
    13 files.
  * **A nested rule opening with a combinator**, `.a { > .b { … } }`, and the selector
    list `> .b, > .c`. 10 files. Not in this entry until a sweep of the corpus found it,
    which the counts above were hiding.
  * **Map literals**, `$m: (a: 1, b: 2)`, nested or not.
  * **`@if` with `and` or `or`**, `@if $a == 1 and $b == 2`. A bare comparison parses.
  * **`!default`**, `$x: 1rem !default`, on every configurable variable in a Sass
    library.
  * **`@use 'x' as t`**, and so the namespaced `@include t.m(…)` that follows it. None in
    bootstrap; found by hand.

  Masking the rest was measured and rejected: they fix 23 more files' error counts and
  recover **no** facts, because their error nodes stay inside the construct. Blanking
  valid source for nothing is a worse trade than the parse error. Fixing them properly is
  upstream grammar work: `tree-sitter-scss` 1.0.0 has no rule accepting an interpolation
  inside a declaration value, an empty parameter list on `@mixin` or `@include`, a nested
  selector opening with a combinator, a map literal, `and` or `or` in an `@if` condition,
  `!default`, or a namespace on `@use`.

  Corrected: this entry previously said `@content` inside a mixin was among them, from
  the grafana run. It parses, bare, nested, and with arguments, so the claim was either
  wrong when written or fixed upstream since, and nothing had re-checked it.

- [ ] B15: `tree-sitter-go` parses `new(…)` as the builtin, which takes a *type*.
  A call to a user-defined function named `new` therefore fails: `new("-10s")` and
  `new(err.Error())` both produce error nodes. In Go `new` is a predeclared
  identifier, not a keyword, and may be shadowed, so this is an upstream grammar bug
  and not invalid source. It accounts for **177 of the 178 Go files** that fail to
  parse in grafana/grafana (2.9% of 6,214); the remaining one is unexplained. Files
  still index, since an error node is local to its subtree, what is lost are the
  facts inside that expression. In `tree-sitter-go` 0.25.0 `new` is consumed by the
  builtin-call rule, so no ordinary call expression is produced for it.

- [ ] B234: `tree-sitter-python` cannot read a type parameter default,
  `type A[T = int] = float`, PEP 696, Python 3.13. A type alias without one reads
  cleanly. Found in `psf/black`'s test data. `tree-sitter-python` 0.25.0 has no default
  clause on a type parameter.

- [ ] B233: `tree-sitter-python` cannot read a starred *literal* in an unparenthesised
  tuple. `g = 1, *[2]` is ordinary Python, and so are the `*(2,)`, `*{2}` and `*"ab"`
  forms. A starred *name* or *call* in the same position reads fine, and so does the
  whole thing in brackets. Found in `psf/black`'s `expression.py`, where the line is
  `g = 1, *"ten"`. `tree-sitter-python` 0.25.0's unparenthesised tuple accepts a splat of
  a name or a call and not of a literal.

  Both are pinned by `tests/known_grammar_gaps.rs`, from both sides: the failing form
  and the neighbouring forms that work. A grammar upgrade that fixes one should retire
  its entry. One that starts reading it *without* an error node while building the
  wrong tree would be worse than the error it replaced.

- [ ] B232: `tree-sitter-typescript` cannot read a property called `in` when another
  member precedes it. `interface G { in?: string }` is fine and so is
  `interface G { in: string }`, but this is not:

  ```ts
  interface G {
    a?: string
    in?: string      // error node
  }
  ```

  The grammar takes `in` after a preceding member as the `in` operator. Found in
  `vuejs/core`'s SVG attribute types, where the SVG `in` and `in2` filter attributes sit
  in a long list of properties. `tree-sitter-typescript` 0.23.2 takes `in` after a
  preceding member as the `in` operator.

- [ ] B231: `tree-sitter-typescript` cannot read an import type,
  `import("@babel/types").Statement[]` in a type position. Valid TypeScript and common in
  generated declarations; found in `vuejs/core`'s compiler-sfc. `tree-sitter-typescript`
  0.23.2 has no rule for an `import` type.

- [ ] B133: `tree-sitter-zig` requires at least one member in a struct. So it cannot
  parse `const Foo = struct {};`, which is ordinary Zig, and is the only parse failure
  across 29 files of Zig's own standard library (`json/static_test.zig:465`). The tool's own check would
  therefore refuse to write a correct file. So an empty record is written with an empty
  `comptime {}` block in it, under a comment saying why. That block does nothing, both
  Zig and the grammar accept it, and the alternative was refusing to translate a type
  with no fields at all. `tree-sitter-zig` 1.1.2 requires at least one member in a
  container declaration.

## Fixed

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
  `clamp` read `LIMIT` from beside itself; pasted into another file the name
  meant nothing there, and the paste compiled, ran, and raised NameError with
  no warning. A body name defined beside the callee and invisible at the call
  site refuses, named. Pinned in `tests/inline_call.rs`.

- [x] B411: **`fr move` broke Python importers twice over.** Code moved into the
  file it imported from carried the import along, a module importing itself
  half-initialised; and an importer holding the whole module (`import mod;
  mod.foo()`) gained a dead named import while every call kept dereferencing
  the module that no longer held the name. The self-import is dropped, and the
  module-attribute receivers rewrite to the new module, which the importer now
  imports. Pinned in `tests/move_languages.rs`.

- [x] B410: **a receiver's declared type did not hold its call still.** Renaming
  `A`'s overloads took `b.size(2)` with them as a dispatch candidate, though
  `b` is declared `B` and `B` answers `size` itself; javac refused the result.
  A dispatch-candidate site whose receiver's declared type sits outside the
  family stays, and the warning names the type instead of claiming it unknown.
  The same evidence holds `fr signature` still. Pinned in
  `tests/rename_hierarchy.rs`.

- [x] B409: **TypeScript overload signatures renamed apart from their
  implementation.** Two `function pick` declarations over one body are one
  function; renaming any alone left `error TS2389`. Same name, same file, same
  container is the entity. Pinned in `tests/rename_hierarchy.rs`.

- [x] B408: **deleting the only statement of a Python suite wrote a file that
  does not parse.** The hole gets a `pass`, judged against every span of the
  plan so a multi-site delete still empties cleanly.
  Pinned in `tests/python_attributes.rs`.

- [x] B407: **instance attributes and locals fed each other's renames.** A bare
  `count` never names a member in the languages that spell members through a
  receiver, and `self.count` in a sibling method is a member of the enclosing
  class wherever its definition sites sit; both resolutions said otherwise, so
  a local's rename took one line of three and an attribute's skipped the
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
  answered "no symbol or resolved reference at" the most common rename target
  the language has. Each `self.x = ...` site now defines a field; the class,
  carried as the qualifier, groups the sites into one entity, and the reads
  follow the rename. Pinned in `tests/python_attributes.rs`.

- [x] B397: **`@property` crossed as a method while its accessors stayed reads.**
  In the target `it.total` was the function object, and every comparison
  against it was quietly false. The flag crosses on the method: TypeScript
  writes `get total()`, Python writes the decorator back, and the targets
  without the idiom write the accessors as the calls they become.
  Pinned in `tests/translate_classes.rs`.

- [x] B396: **the everyday library calls crossed as compile errors.**
  `console.log` reached Python, `.push` reached Rust, `print` reached
  TypeScript, all unmarked. The readers rewrite their spellings into one
  canonical set and the writers rewrite them out, `print`, `len`, `str`,
  `.append`, `.upper`, `.lower`, `.strip` and `sep.join(xs)`; Go gains the
  imports its mapped calls need. Pinned in `tests/translate_builtins.rs`.

- [x] B395: **the program's own entry was dropped as unsupported.** `main();`
  at the bottom of a TypeScript file, the call under Python's `__main__`
  guard: both became comments, and the translated program ran and printed
  nothing. A top-level statement is an item now; Python writes it back under
  its own guard. With it went two shapes around the same story: a field's
  initializer crosses as a default the dataclass accepts, and a returned
  object literal builds the record its signature promised.
  Pinned in `tests/translate_entrypoints.rs`.

- [x] B394: **a class crossed as an empty struct.** The fields Python declares
  in `__init__` and the ones `record Order(...)` declares in its header were
  read as nothing, while the methods went on using them. Both derive now;
  a constructor of plain assignments becomes each target's own constructor,
  `Item(...)` becomes a construction, a Java static loses the receiver its
  call sites never passed, and a record's accessor calls become the field
  reads they are. Pinned in `tests/translate_classes.rs` and
  `tests/translate_java_records.rs`.

- [x] B393: **`return a, b` translated to a bare `return`.** The reader mapped
  Go's multiple return to nothing, so a two-value return lost its payload with
  nothing said, in every target at once. Several values travelling as one are
  a tuple in the IR now, expression and type both; a writer with no spelling
  for one says so instead. Pinned in `tests/translate_tuples.rs`.

- [x] B392: **a field and a method under one name shared one use list.** The
  Rust facts recorded a method call's callee as a field read, so `order.name()`
  and `order.name` were indistinguishable: the field's uses counted zero and
  the method collected the field's accesses. The callee records as a call now,
  and the resolver keeps only the member the syntax allows.
  Pinned in `tests/member_kinds.rs`.

- [x] B391: **`fr move` broke both of an importer's imports.** An aliased
  `import { foo as increment } from "./a"` was left naming a gone export while
  a fresh unaliased import landed beside it. The existing statement repoints,
  keeping the alias and splitting stayers from movers. The Go half of the same
  probe: a moved body's bare calls back into its old package now qualify with
  the package name, the destination gains the import, and an unexported
  dependency refuses with the visibility problem named.
  Pinned in `tests/move_imports.rs` and `tests/move_languages.rs`.

- [x] B390: **`fr signature` skipped a function held as a value.** `let f:
  fn(i32, i32) -> i32 = add;` has no argument list to rewrite, so the site was
  silently passed over, the declaration changed under the binding, and the
  command reported clean call sites. A value-shaped mention outside an import
  refuses, naming the binding. Pinned in `tests/signature_hierarchy.rs`.

- [x] B389: **renaming Java overloads wrote calls to nothing.** Both `size`
  declarations renamed as one entity while every call stayed behind at
  name-only confidence, and javac refused the result. When the group holds
  every declaration the name answers to, a name-only call can only reach a
  renamed one: it renames too, reported under the dispatch-candidate heading.
  A stranger answering the same name still holds the calls in place.
  Pinned in `tests/rename_hierarchy.rs`.

- [x] B388: **`fr inline` ran a side effect twice.** `let v = effect(); v + v`
  inlined to `effect() + effect()`. The call inliner refused exactly this for
  arguments; the variable path now applies the same rule, and only to values
  that can run something, so `a + b` twice still inlines.
  Pinned in `tests/inline_scope.rs`.

- [x] B387: **a rename could move a use under a shadow and change what runs.**
  Renaming outer `value` to `temp` under an inner `let temp` rebound the use;
  the file compiled and returned a different number. Both directions refuse
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
  carried each function's line and no column, so the click put the cursor at column 1. The
  status bar then read "nothing the index knows at this position", and every action
  refused. `graph_around` carries the column of the name now. Found by clicking one.

- [x] B351: **the fixed-defect archive was 31,000 words.** 333 entries, median 85 words,
  for defects closed and gone. An entry needs its symptom and its fix, and git holds the
  rest. Entries below B300 keep the symptom line alone, and the file is 9,000 words now.
  This entry was written once and lost before the commit, which is its own small lesson.

- [x] B350: **the call graph tab never drew anything.** So `graph_around` has two checks
  there now: the shape of an answer, and the shape of a refusal.

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

- [x] B345: **the dispatch wording explained nothing.** The message now says which
  implementations a call could reach, and that the program chooses one while it runs.

- [x] B344: **a doc comment for one function sat on top of another.** Found while rewriting
  the comments in that file.

- [x] B343: **the prose in this repository was written in a machine's voice.**
  `docs/terminology.md` holds the terms.

- [x] B342: **the site could not be published for two days. Every run said `cancelled`.**
  `cancel-in-progress: true` now, so the newest deploy takes the slot.

- [x] B341: **`fr flow` sent three languages to an analysis that has no arm for them.** The
  rule that holds, no language is offered both, is what the test asserts now.

- [x] B340: **the browser never routed to provenance. So it answered questions the CLI
  could.** Both bindings route the same way the CLI does now.

- [x] B339: **`fr remove-flag` told the reader to do something the command could not do.**
  The resolved name is fixed before the cascade starts, because everything downstream, which
  uses are left, which imports were orphaned, what each round is called, looks the flag up
  by name.

- [x] B338: **"move it somewhere under `src/`" led to a second refusal.** The first now says
  the destination has to be one the module tree already declares. The second names the
  line to add.

- [x] B337: **provenance's refusal named a library module.** "Use analysis::flow
  (backward/forward) instead" is not something the reader of a CLI or browser message can
  run. It names `fr flow`.

- [x] B336: **the compile gate passed whether the tool worked or refused.** Each site now
  says which outcome it expects: `must_plan` where it compiles, `must_refuse` with the
  reason where it declines.

- [x] B335: **licence and provenance checks passed when they checked nothing.** All three
  now count what they examined and fail when the count is zero.

- [x] B334: **nine refusals wrote their own article. Four kinds start with a vowel.** A test
  asks every kind at once and not the four that happen to be wrong today.

- [x] B333: **`fr type` and `fr flow` answered for nine languages the matrix disclaims.**
  Both now refuse by name, and the language list lives in one place each.

- [x] B332: **the matrix disclaimed two capabilities the tool has.** One predicate now, and
  it is the list of arms the command has: 272 of 384 pairs supported.

- [x] B331: **`fr remove-flag` wrote XML that no parser accepts.** The command asks
  `supports_cascade` now, the same predicate the matrix publishes, and refuses by name.

- [x] B330: **the scale sweep measured whatever `web/src/wasm` happened to hold.** The sweep
  now compares the artifact's timestamp against the newest `.rs` file and refuses to run
  when it is behind.

- [x] B329: **the scale sweep decided what counted as a refusal by reading the sentence.**
  The browser API reports `refused` now, from the type and not from the prose.

- [x] B328: **four more refusals blamed a language for a path.** `because` is `&'static str`
  now.

- [x] B327: **`fr move` told a Rust user that Rust was unsupported.** It is the one the
  matrix audit found, because a claim and a refusal cannot both be right.

- [x] B326: **`fr delete` left the import its deleted code was the only user of.** The
  result parses either way. So the parse sweeps never saw it.

- [x] B325: **`fr imports` never narrowed a statement that lost one of its names.** It
  narrows now, by taking the dead names' clauses out of the statement and not
  re-spelling it, because each language writes the list differently and the separator is the
  only thing that has to be understood.

- [x] B324: **`fr remove-flag` left the imports its collapsed branch had been using.** That
  command carries a body of knowledge about uses no query can see, a Rust trait reached
  through its methods, a JSX pragma in a comment.

- [x] B258: **closed unreproducible, with the evidence.**

- [x] B323: **a Java statement declaring several names gave each of them all three.** The
  query captured the statement. It captures the declarator now, which the symbol is.

- [x] B322: **`fr type` could not read a Java call or construction.** This is the same
  omission that once made `fr signature` refuse at every Java call site there has ever been,
  in a second place that had not heard.

- [x] B321: **`fr type` answered `var`.** It falls through to inference now, which is what
  the keyword asks for.

- [x] B320: **`fr inline` refused every Java local.** Java puts the name and the value
  together in a declarator, because one statement may declare several, so the value hangs
  off the declarator and not off the declaration.

- [x] B319: **three readers answered "what does this declaration bind". Disagreed.** There
  is one reader now, `parse::declaration_value`, and `tests/declaration_values.rs` names
  every shape it has to know.

- [x] B318: **`fr move` left the destination calling the symbol through the file it came
  from.** The qualifier in front of a Zig call is the same thing said differently, and it is
  dropped now.

- [x] B317: **`fr signature` reported call sites it had not touched.** The two are told
  apart now: no argument list still means no parentheses. A grammar that wraps nothing
  is read on its own terms.

- [x] B316: **a Zig `@import` path resolved to nothing.** So `fr rename` rewrote the
  declaration and left every caller naming something that is not there.

- [x] B315: **a Java static call resolved to the wrong method, at exact confidence.** A
  receiver naming a type declared in this workspace is a path now, in any language.

- [x] B314: **the confidence cap and the rule that resolves a qualified call disagreed.**
  Both places ask one question and now ask it in one place.

- [x] B313: **`Language` ignored a width in a format string.** Its `Display` wrote the
  name straight out instead of going through `Formatter::pad`, so `{target:<10}` padded
  nothing and the column of targets `fr translate` prints came out ragged wherever a
  `Language` sat in it.

- [x] B312: **`fr translate` offered a list that was not true.** `tests/translate_sweep.rs`
  holds the list to its word from both sides.

- [x] B311: **`fr move` left a star re-export naming a file the symbol had left.** Removed
  outright where the move took the last thing the source exported, because TypeScript calls
  a file with no exports "not a module" and rejects the star for that instead.

- [x] B310: **`fr move` dropped the names beside the one it repointed.** `export { width,
  Holder } from "./holder"` came back as `export { width } from './util'` with `Holder`
  gone.

- [x] B300: **`fr move` declined at a re-export barrel.** Now says why: readers write
  `ns.width`. Splitting the module in two cannot be followed by repointing one
  statement.

- [x] B309: **`fr usages` and the reference index disagreed. The check could not see it.**
  Those are what it checks now.

- [x] B308: **a Go call into another package resolved to nothing.** The import statement
  names the package. So which declaration a qualified call means is written down; resolution
  now reads it.

- [x] B307: **`fr move` wrote import cycles that neither Go nor Python accepts.** Both are
  refused now, naming the two files and what each would import.

- [x] B306: **`fr move` wrote a relative import into a file that is in no package.** The
  import is written relative inside a package and absolute outside one.

- [x] B305: **`fr remove-flag` deleted a declaration whose readers it had refused.** A
  cascade that changes nothing is now a refusal carrying the reasons, and not a plan of zero
  edits.

- [x] B304: **`fr remove-flag` replaced the callee of a call instead of the call.** A rule
  that is documented and never runs is the same defect shape as B296.

- [x] B303: **`fr remove-flag` wrote a boolean into a type position.** So one use in a
  position only a type can occupy settles what the name is. The whole operation is refused,
  naming that use.

- [x] B302: **`fr remove-flag` treated every constant as a possible flag.** Sweeping every
  name in the vendored corpus, both values, found 234 asks that produced a plan, among them
  `const DocumentScope = @import("DocumentScope.zig")`, which was rewritten to `*const
  true`.

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

- [x] B118: **the Zig reader read named children only, and in that grammar the `:` before a
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
  `from __future__ import annotations`, which binds nothing at all and decides how
  every annotation in the file is read, `str | None` stops parsing without it below
  Python 3.10. Which is placed first, where the language requires it.

- [x] B46: **a guard clause exited the wrong construct.**

- [x] B45: a statement pattern was impossible in Python, shell and YAML. Those
  languages wrap a `fr restructure` fragment in nothing, so the statement the pattern
  writes is the outermost node. The descent that strips wrapper-introduced
  statement containers stripped that one too, leaving the fragment starting six bytes
  inside itself and every such pattern rejected as "not a valid fragment". Descending
  is only correct when the child begins where the container does; `raise` does not.
  `fr restructure 'raise InvalidURL($X)' 'raise InvalidURL($X) from None'` now works
  on psf/requests.

- [x] B44: **a Terraform traversal ignored its namespace.**

- [x] B41: **the cache reused facts produced by a different extractor.**

- [x] B42: Rust reached nothing through a path. `super::render_custom_markup(…)` and
  `Patterns::from_low_args(…)` both resolved to nothing, because the prefix of a
  `scoped_identifier` was never recorded. References now carry it, flagged as a path
  instead of a value, a path names a type or a module and can be matched against a
  symbol's own qualifier with no type inference, since the type was written down.
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
  instead of the workspace `-C` names, so `fr -C ../helm refs pkg/x.go:3:6` failed
  with "reading pkg/x.go: No such file". Four sites had their own
  `canonicalize().unwrap_or(…)`, which kept the unusable path and let the failure
  surface two frames later. They now share one resolver that says where it looked.

- [x] B37: a field access resolved to a local variable. `i.provData` bound to a
  `provData, err := …` two lines up, because the nearest-definition rule ran before
  anything checked that a member access can only name a member. The field then had no
  references at all and was reported as dead.

- [x] B38: nothing tested the command line. Every test called the library directly,,
so B35 and B36, both entirely in the layer between an argument and that
  library, were invisible. `tests/cli.rs` runs the binary: argument parsing, path
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
  as well, and is now applied in all of them.

- [x] B30: a use bound to the nearest declaration in either direction. Go re-declares
  with `:=` mid-function, and helm's `var ret …` / `return ret` / `ret, err := …`
  bound the early return to the *later* binding because it sat 15 bytes closer. Value
  bindings now prefer a declaration above the use; a function may still be called
  above where it is written.

- [x] B31: a package may declare one name twice under opposite build tags,
  `//go:build windows` and `//go:build !windows`. Resolution picked the first and
  reported the other as dead; picking one would rewrite half a pair and break the
  other build. Both are now reported as ambiguous and spared.

- [x] B32: **the public API of a library rooted nothing.**

- [x] B33: two types sharing a private method name made both look dead, because the
  call resolved to neither. They are now spared with that stated, except where the
  hierarchy analysis has already ruled on the name, whose answer is the more precise
  one and stands.

- [x] B34: `fr unused` had no way to narrow its report. On a polyglot repository every
  Markdown heading drowned the code findings. `-C` could not be used to narrow
  because a smaller index invents dead symbols instead of hiding them. Added
  `--lang`, `--path` and `--internal`, which filter the report and not the index,
  with an unknown language name refused against the known list.

- [x] B16: micro-rewrites were published for seven languages and tested on three.
  `invert-if` and `guard-clause` negated the whole condition node, which in the C
  family and Zig *includes the brackets*, so both emitted `if !(a)`, valid Rust,
  a syntax error in TypeScript, TSX and Zig. Zig failed earlier still: its grammar
  calls the consequence `body`, not `consequence`, so no part of the `if` was found.
  Fixed by negating the expression inside the condition and splicing within it, so
  whatever the grammar writes around it survives; `guard-clause` now builds its
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
  Go gets `x int` and not `x: int`. Its call site loses the C semicolon,
  `gofmt -d` reports no diff on the result.

- [x] B21: **a move produced files that parse and do not compile.**

- [x] B22: a move given a destination spelled differently from the indexed path, a
  relative path, or `/var` where the index holds `/private/var`, wrote imports like
  `'../../../../../../../var/folders/…'`. In the relative case silently added no
  import at all. `canonicalize()` had failed on a file that does not exist yet and the
  result was passed through unchanged. The destination is now resolved through its
  parent directory, and a missing directory is an error. The matching silent skip in
  the move itself, a file that needed an import and did not get one, reported as
  success, is now a failure.

- [x] B23: the index records one `Import` per imported name, each carrying the whole
  statement's span, so a four-name import read as four statements. Anything rewriting
  import statements has to regroup them first, and a move did not: it emitted the
  same statement once per used name.

- [x] B24: generated code was indented four spaces regardless of the file. Two-space
  TypeScript and tab-indented Go both received four, on every guard clause and every
  extracted function. One level is now read from the source.

- [x] B25: `fr unused` listed every `_`-prefixed parameter, the convention in Rust,
  TypeScript, Python and Zig for a binding a signature forces and the body ignores.
  One real file contributed eight. They are now spared with that stated reason rather
  than dropped quietly.

- [x] B10: Helm values precedence stopped at the command line, whether a
  `values-*.yaml` is passed with `-f`, the order of several `-f` files. Every
  `--set` were invisible and reported undecided. That was a missing input, not a
  limit: `fr flow back <target> -f values-prod.yaml --set a.b=c` supplies the
  invocation, and Helm's order (chart `values.yaml` < each enclosing parent chart <
  each `-f` in the order given < `--set`) then decides it, winner marked and every
  loser, including a values file the caller says is *not* passed, still listed.
  With nothing supplied the answer is what it was.

- [x] B12: Terraform lost the third and later step past an index traversal. A query
  cannot say "every sibling after this one". So each step needs its own pattern; six
  are now written, which is far past anything Terraform expresses. A test asserts
  the bound so it stays a decision instead of an accident.

- [x] B0a: `LineIndex` invented a phantom trailing line for files ending in a newline,
  so `"a\nb\n"` counted 3 lines and an EOF offset reported a column past the last
  character, `src/span.rs`. Fixed: a trailing newline terminates the final line;
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
  is what keeps byte offsets valid, so the symbol index still shows guarded keys
  unconditionally; only provenance and stitch consult the guards.

- [x] B3: deleting a CSS selector left its `{ ... }` block orphaned. The delete widens
  the selector's span to the whole rule when it is alone on it, or to that selector
  and its comma when the rule has others.

- [x] B4: import liveness was name-based, so anything a language brings into scope
  invisibly looked unused. Per-language guards now hold back and report: Python
  `__future__` imports, `__all__` re-exports and dotted registration imports;
  TypeScript type-only imports, JSDoc `{Foo}` mentions, JSX pragmas and `typeof X`;
  Go blank imports and packages whose clause name cannot be derived from the path.
  Zig was verified to need none. Two real false positives fell out of it: Python
  `import a.b` binds `a`, not `b`, and `gopkg.in/yaml.v2` binds `yaml`, not `v2`.

- [x] B6: consecutive standalone Go `import "x"` lines were not sorted, because the
  `import` keyword sits outside the `import_spec` span and looked like unrelated code
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
