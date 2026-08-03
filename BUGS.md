# BUGS

Known defects and limitations, and their status. Updated alongside PLAN.md at every
stage.

Format: `- [ ] B<N>: <symptom> — <where> — <status/notes>`

Every open entry below is a *characterised limitation* rather than breakage: the
behaviour is reported to the user, and no operation silently does the wrong thing.

## Open

- [ ] B5: `find_unused` and the call graph follow class-hierarchy dispatch as well as
  resolved calls: a Rust `impl Trait for Type` (supertraits included), a Go interface
  whose method set a type covers by name and arity, a TypeScript `implements`/`extends`
  clause, and a Python base class each fan an unresolved method call out to every
  implementation, tagged `field-based`, counted apart from resolved edges by
  `fr graph`, and named as the reason a symbol was spared. TypeScript additionally
  falls back to matching the method name alone where no `implements` is written, which
  is unsound by design and labelled `method-name` rather than `declared-supertype`.
  What remains is undecidable from the source, not unimplemented: a function held in a
  map, a struct field or a variable and called through it — nothing declares it a
  method of any type, so there is no method set to look it up in — and a name assembled
  at runtime from pieces no string literal spells. A symbol used only from a file that
  failed to parse is invisible for a third reason, and `delete::plan` reports that file
  as possibly hiding uses. Zig (comptime duck typing) and Bash declare no
  implements-relationship at all, so neither has a hierarchy to read.
- [ ] B13: an answer from supplied values inputs is only as complete as the
  description of them. Given `--set` but no `-f` (or the reverse) the competition is
  decided *given the inputs supplied* and says so, naming the channel it was never
  told about; nothing infers an invocation. Three narrower edges: `--set ports[0].name`
  and `--set ports[1].name` address the same key path, because the symbol index
  records mapping paths without list indices; `--set x=null`, which deletes a key in
  Helm, is ranked as a source that supplies it; and `{a,b}` list literals, `--set-file`
  and `--set-json` are refused by name rather than half-applied.
- [ ] B14: a CSS class named inside a TSX helper call or template literal —
  `className={cx("btn", active && "on")}`, `` className={`btn ${size}`} `` — is not
  resolved, because only a plain string attribute value is captured. A rename of that
  class rewrites the plain `className="btn"` uses and leaves the helper ones; the
  textual sweep does report each missed site as needing review, so the result is
  incomplete rather than silently wrong. Resolving them means teaching the TSX queries
  which call arguments are class lists, which is a per-library convention (`clsx`,
  `cx`, `classnames`, `cva`, `tailwind-merge`) rather than a language rule.

  Measured over grafana/grafana's 4,400 TSX files, `className` is written as:
  `styles.x` from CSS-in-JS 3,233 times, `cx(…)` 381, a plain string literal 224, a
  template literal 28. So the helper form outnumbers the resolvable one, and closing
  this would roughly triple the reach of a stylesheet-class rename in a modern React
  codebase. The CSS-in-JS majority is a different matter and out of scope: there is no
  stylesheet selector to link `styles.x` to.

- [ ] B11: SCSS forms `tree-sitter-scss` 1.0 cannot parse, each surfaced as a parse
  error rather than mis-handled. Found by hand: empty parentheses on a declaration
  (`@mixin m()`), empty parentheses on a call (`@include m();`), and a namespaced
  include after `@use 'x' as t` (`@include t.m(…)`). Found by running over
  grafana/grafana, where they cost 5 of 8 stylesheets: **`@content`** inside a mixin,
  and **map literals** (`$m: (a: 1, b: 2)`). The last two are ordinary Sass, so SCSS
  coverage is materially worse than the three hand-found cases suggested. Fixing any
  of them is upstream grammar work.

- [ ] B15: `tree-sitter-go` parses `new(…)` as the builtin, which takes a *type*, so
  a call to a user-defined function named `new` fails — `new("-10s")` and
  `new(err.Error())` both produce error nodes. In Go `new` is a predeclared
  identifier, not a keyword, and may be shadowed, so this is an upstream grammar bug
  rather than invalid source. It accounts for **177 of the 178 Go files** that fail to
  parse in grafana/grafana (2.9% of 6,214); the remaining one is unexplained. Files
  still index, since an error node is local to its subtree — what is lost are the
  facts inside that expression.

## Fixed

- [x] B26: **Go resolved nothing across files in a package.** A package in Go is a
  directory — a function in `a.go` is called from `b.go` with no import and no
  qualifier — and only Terraform was treated that way. `fr refs` returned *zero*
  references for symbols helm/helm calls from the file next door, `fr unused`
  reported 238 internal Go symbols as dead where 50 are, and a rename rewrote the
  definition while listing the call sites it could not see as "unresolved". Package
  scope is now the directory, restricted to top-level declarations — `qualifier`,
  not `container`, since a Go method's receiver type is declared elsewhere and so the
  method links to no containing symbol.

- [x] B27: a method call resolved to a package-level function of the same name.
  `w.contextWithTimeout(…)` and `time.Now()` are one syntax in Go, and the grammars
  capture only the callee, so nothing separated a member from a package-qualified
  call. References now record the receiver they were written against, and an import
  binding before the dot is what tells the two apart. Without it the method read as
  dead while the function absorbed its call sites.

- [x] B28: **file proximity decided which method a call meant.** Two types declaring
  the same method in one file are equally plausible targets; resolution picked the one
  written nearer the call and reported it as a resolved edge, which made the other
  look dead. Proximity is no longer evidence for a member access — the answer is
  "either", and both stay live.

- [x] B29: a binding resolved inside its own initialiser. helm's
  `templatesDirExists := run(…, templatesDirExists(path))` calls the package function
  and *then* shadows it; resolving the call to the variable being declared made the
  function look dead. The rule holds in Rust (`let x = x + 1`) and Python (`x = f(x)`)
  as well, and is now applied in all of them.

- [x] B30: a use bound to the nearest declaration in either direction. Go re-declares
  with `:=` mid-function, and helm's `var ret …` / `return ret` / `ret, err := …`
  bound the early return to the *later* binding because it sat 15 bytes closer. Value
  bindings now prefer a declaration above the use; a function may still be called
  above where it is written.

- [x] B31: a package may declare one name twice under opposite build tags —
  `//go:build windows` and `//go:build !windows`. Resolution picked the first and
  reported the other as dead; picking one would rewrite half a pair and break the
  other build. Both are now reported as ambiguous and spared.

- [x] B32: **the public API of a library rooted nothing.** With no `main`, everything
  beneath an exported symbol read as dead — in helm that was most of `pkg/action`,
  where `performInstall` is reached only through the exported `RunWithContext`.
  Exported symbols now seed reachability, while being judged on their own uses, so an
  export nothing calls is still listed and tagged rather than hidden.

- [x] B33: two types sharing a private method name made both look dead, because the
  call resolved to neither. They are now spared with that stated — except where the
  hierarchy analysis has already ruled on the name, whose answer is the more precise
  one and stands.

- [x] B34: `fr unused` had no way to narrow its report. On a polyglot repository every
  Markdown heading drowned the code findings, and `-C` could not be used to narrow
  because a smaller index invents dead symbols rather than hiding them. Added
  `--language`, `--path` and `--internal`, which filter the report and not the index,
  with an unknown language name refused against the known list.


- [x] B16: micro-rewrites were published for seven languages and tested on three.
  `invert-if` and `guard-clause` negated the whole condition node, which in the C
  family and Zig *includes the brackets*, so both emitted `if !(a)` — valid Rust,
  a syntax error in TypeScript, TSX and Zig. Zig failed earlier still: its grammar
  calls the consequence `body`, not `consequence`, so no part of the `if` was found.
  Fixed by negating the expression inside the condition and splicing within it, so
  whatever the grammar writes around it survives; `guard-clause` now builds its
  header from the source's own bytes rather than reinventing one per language.
  Found by running the tool on grafana/grafana, where 63 of 65 real if/else sites
  now invert cleanly and both refusals are genuine `else if` chains.

- [x] B17: **`guard-clause` silently changed what Go programs do.** Go's grammar puts
  a `statement_list` between a block and its statements; counted as a statement, it
  made every block look like a block of one, so the "is this `if` last?" check passed
  for an `if` with code after it and the guard hoisted that code out from under the
  condition guarding it. The result parses, so the reparse check never saw it.
  Measured over 250 files of grafana/grafana's `pkg/services`: **1,258 of 1,498
  applications (84%) were wrong**; after the fix those are refused and the remaining
  240 apply. Fixed at the class rather than the site — `is_statement_container` was
  duplicated in two modules and had already drifted, and is now one shared predicate.

- [x] B18: `invert-if` accepted an `else if` chain and produced unparseable output.
  The second condition is only tested when the first is false, so swapping the
  branches changes which tests run; it is now refused with that reason. Also fixed:
  `else_body_of` returned the whole `else` clause when it did not recognise the body
  shape, splicing the `else` keyword into the consequence position.

- [x] B19: **de Morgan dropped the grouping its own result needs.** `!(a && b)` is one
  operand; `!a || !b` is two, and the brackets that held it together left with the
  negation. Inside another operator that rebinds silently: `x && !(a && b)` became
  `x && !a || !b`, which parses and means something else. Now bracketed whenever the
  parent binds operands; in shell, where `( … )` opens a subshell, it is refused.

- [x] B20: `extract --function` emitted `function helper(x: : number)` for TypeScript.
  The C-family grammars fold the `:` into the annotation node, and the renderer added
  another. The type is now read bare and each language spells its own punctuation, so
  Go gets `x int` rather than `x: int`, and its call site loses the C semicolon —
  `gofmt -d` reports no diff on the result.

- [x] B21: **a move produced files that parse and do not compile.** The definition was
  relocated and nothing else: it was left unexported while an import was written for
  it, its own imports stayed behind, and whatever it still needed from its old file
  was unreachable. A move now carries the imports the moved code uses — narrowed to
  the names it actually mentions, `type` modifiers intact — imports back what stayed
  behind, exports that too, and exports the moved symbol. A Python back-import makes
  a run-time cycle and says so.

- [x] B22: a move given a destination spelled differently from the indexed path — a
  relative path, or `/var` where the index holds `/private/var` — wrote imports like
  `'../../../../../../../var/folders/…'`, and in the relative case silently added no
  import at all. `canonicalize()` had failed on a file that does not exist yet and the
  result was passed through unchanged. The destination is now resolved through its
  parent directory, and a missing directory is an error. The matching silent skip in
  the move itself — a file that needed an import and did not get one, reported as
  success — is now a failure.

- [x] B23: the index records one `Import` per imported name, each carrying the whole
  statement's span, so a four-name import read as four statements. Anything rewriting
  import statements has to regroup them first, and a move did not: it emitted the
  same statement once per used name.

- [x] B24: generated code was indented four spaces regardless of the file. Two-space
  TypeScript and tab-indented Go both received four, on every guard clause and every
  extracted function. One level is now read from the source.

- [x] B25: `fr unused` listed every `_`-prefixed parameter — the convention in Rust,
  TypeScript, Python and Zig for a binding a signature forces and the body ignores.
  One real file contributed eight. They are now spared with that stated reason rather
  than dropped quietly.


- [x] B10: Helm values precedence stopped at the command line — whether a
  `values-*.yaml` is passed with `-f`, the order of several `-f` files, and every
  `--set` were invisible and reported undecided. That was a missing input, not a
  limit: `fr flow back <target> -f values-prod.yaml --set a.b=c` supplies the
  invocation, and Helm's order (chart `values.yaml` < each enclosing parent chart <
  each `-f` in the order given < `--set`) then decides it, winner marked and every
  loser — including a values file the caller says is *not* passed — still listed.
  With nothing supplied the answer is exactly what it was.

- [x] B12: Terraform lost the third and later step past an index traversal. A query
  cannot say "every sibling after this one", so each step needs its own pattern; six
  are now written, which is far past anything Terraform expresses, and a test asserts
  the bound so it stays a decision rather than an accident.

- [x] B0a: `LineIndex` invented a phantom trailing line for files ending in a newline,
  so `"a\nb\n"` counted 3 lines and an EOF offset reported a column past the last
  character — `src/span.rs`. Fixed: a trailing newline terminates the final line;
  columns clamp to the line end.
- [x] B0b: `.gitignore` was ignored outside a git repository, so scans of worktrees
  and exported trees walked `target/`, `node_modules/` etc — `src/scan.rs`. Fixed with
  `WalkBuilder::require_git(false)`.
- [x] B1: SCSS was parsed with the plain CSS grammar, so `$variables`, `@mixin`,
  `@include` and `@use` were all parse errors. Fixed at the root by adding the
  `tree-sitter-scss` grammar. A test asserts the CSS grammar still rejects SCSS
  syntax, so the split is real rather than cosmetic.
- [x] B2: a Helm template action in a structural position yielded a YAML tree
  reflecting no single rendering. Fixed for the analyses that reason about values: a
  key wrapped in `{{- if }}` now produces a stop naming the exact condition, and the
  condition's own `.Values` key resolves. Masking itself is unchanged by design — it
  is what keeps byte offsets valid — so the symbol index still shows guarded keys
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
  names are named as unresolved rather than resolved.
- [x] B8: Terraform splat traversals lost their trailing segments. `[*].id` and
  `.*.id` now capture every following attribute; B12 records what an index traversal
  still loses.
- [x] B9: `.tfvars` top-level attributes now produce `Key` symbols, so values files
  are in the index rather than needing provenance to walk the tree itself.
