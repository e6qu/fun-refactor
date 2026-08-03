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

- [x] B53: **the browser reported symbols dead that the terminal reported live.**
  `find_unused` takes the roots reachability starts from, and the browser API passed
  `&[]` where the terminal passed a detected catalog. Both type-checked; the empty
  slice means "nothing runs", so every `#[test]`, every HTTP handler and everything
  they reach read as dead — twenty extra findings in a twenty-four-file workspace.
  The roots are now a type, `Entrypoints`, with no way to end up empty by omission:
  `detect` runs the catalogs, `exactly` takes a list on purpose, `none` says so.

- [x] B52: **two workspaces in one page shared one set of bytes.** The browser's
  virtual filesystem was a single thread-local map that each new `Workspace`
  overwrote. An older handle then answered from the newer one's text: spans measured
  against one file applied to another. It surfaced as a rewrite that `rewrites_at`
  said was unavailable at a position where `rewrite` applied it. Each `Workspace` now
  owns its files and installs them before every call, which `tests/wasm_api.rs`
  checks, because the compiler cannot.

- [x] B51: **`Path::exists` bypassed the virtual filesystem.** Every *read* went
  through `crate::vfs`; the six `exists()` calls did not, and on wasm there is no
  filesystem, so each quietly answered false. `fr move` refused every Rust file in the
  playground with "src has neither lib.rs nor main.rs" while `src/main.rs` sat in the
  loaded workspace. Helm chart detection had the same hole. `tests/vfs_choke_point.rs`
  now fails the build on a new one.

- [x] B50: **a call that resolved to an interface method reached no implementation.**
  The hierarchy layer fans out call sites that resolved to *nothing*. A Go
  `sink.Store(r)` where `sink` is typed as the interface resolves exactly — to a
  declaration with no body — so the fan-out never ran and every implementation of
  every interface was unreached, and reported as dead code. A resolved call to an
  abstraction now also yields one `field-based` edge per implementation.

- [x] B49: **`fr implementations` answered nothing for an interface.** It required a
  method, so pointing at the type that declares them returned an empty list and the
  message "nothing declares it as an abstraction" — about a Go interface with three
  implementors. Asking of the type now returns the implementing types: declared
  subtypes for Rust, TypeScript and Python, method-set coverage for Go. An empty Go
  interface still names nothing, because every type satisfies it.

- [x] B47: **a new import was written inside a multi-line import statement.** The
  insertion point was found by scanning lines for an `import` prefix and stopping at
  the first line that is not one — so given `from typing import (` / `    Any,` /
  `)`, it stopped at `Any,` and inserted the new statement between the parentheses.
  psf/requests writes its typing imports exactly that way, so every `fr move` out of
  `utils.py` produced a file that would not parse. The index knows where each
  statement ends, and is now asked.

- [x] B48: a moved Python symbol left its module imports behind. `import os` binds
  `os` without naming it in the statement, so the name-based check that carries named
  imports never matched it, and the moved code lost `os.path`. Also carried now:
  `from __future__ import annotations`, which binds nothing at all and decides how
  every annotation in the file is read — `str | None` stops parsing without it below
  Python 3.10 — and which is placed first, where the language requires it.


- [x] B46: **a guard clause exited the wrong construct.** The rewrite emitted
  `return` for any `if` last in its block, including one last in a *loop* body —
  ripgrep's `find_program` ends a `for` body that way, and the rewrite left the loop
  entirely instead of continuing it. It also emitted a bare `return` regardless of
  the enclosing function's return type, which in that same function
  (`-> Result<PathBuf>`) does not compile. The exit now fits the block: `continue`
  in a loop, `return` in a function that returns nothing, and a refusal where the
  function owes a value, since what to return early is the author's decision.


- [x] B45: a statement pattern was impossible in Python, shell and YAML. Those
  languages wrap a `fr restructure` fragment in nothing, so the statement the pattern
  writes is the outermost node — and the descent that strips wrapper-introduced
  statement containers stripped that one too, leaving the fragment starting six bytes
  inside itself and every such pattern rejected as "not a valid fragment". Descending
  is only correct when the child begins where the container does; `raise` does not.
  `fr restructure 'raise InvalidURL($X)' 'raise InvalidURL($X) from None'` now works
  on psf/requests.


- [x] B44: **a Terraform traversal ignored its namespace.** `var.azs`, `local.azs`
  and `module.azs` name three different declarations, and an `output "azs"` beside
  them names a fourth that no traversal reaches — but all four are just `azs` in one
  directory, and the directory-scoped rule picked whichever came first. In
  terraform-aws-vpc, `var.azs` resolved to the module's own `output "azs"`: a rename
  would have rewritten the output and all 41 uses of the variable. The namespace is
  written down in the source, so it is now recorded as the reference's receiver and
  the kind it implies is required of the target.


- [x] B41: **the cache reused facts produced by a different extractor.** Entries are
  keyed by file content and by the query set, which is correct only while "the
  extractor" is a constant — and it is not. Adding a field to `Reference` changes
  what a cached fact means, while `#[serde(default)]` lets yesterday's entry
  deserialize cleanly into today's struct. The result is a cache that looks healthy
  and answers wrongly: it turned seven unrelated TypeScript import tests red and cost
  an afternoon of bisecting code that was not at fault. `build.rs` now hashes the
  sources that define extraction into the cache namespace, so editing any of them
  makes every stale entry unreachable rather than wrong.

- [x] B42: Rust reached nothing through a path. `super::render_custom_markup(…)` and
  `Patterns::from_low_args(…)` both resolved to nothing, because the prefix of a
  `scoped_identifier` was never recorded. References now carry it, flagged as a path
  rather than a value — a path names a type or a module and can be matched against a
  symbol's own qualifier with no type inference, since the type was written down.
  This rule runs before every other: ripgrep declares four `from_low_args` methods in
  one file, so the nearest-in-file rule would otherwise pick whichever sat closest and
  leave the other three looking dead.

- [x] B43: a Rust test looked like dead code. Tests declare themselves with `#[test]`,
  and the entry-point catalog could only match names and paths — ripgrep's are called
  `backslash`, `tab` and `carriage`. Catalogs gained `annotated_with`, which reads the
  annotations immediately above a definition, and Rust gained rules for `#[test]` and
  `#[bench]`. Detected test entry points in ripgrep went from 141 to 516, and its
  internal dead-code report from 643 findings to 317.


- [x] B40: **`fr extract` put a Go binding above the declaration it read.** Extracting
  `len(totalItems)` from an `if` inserted `itemCount := len(totalItems)` at the top of
  the function, before `totalItems` existed. The result parses, so the reparse check
  saw nothing wrong; it simply does not compile. The cause was a *third* private copy
  of the "is this a statement container" predicate, this one still not knowing about
  Go's `statement_list`, so the enclosing statement of an expression resolved to the
  whole function body. All three copies are now one.


- [x] B39: **a Helm values key could not be renamed.** A template action is masked
  before parsing — which is what keeps the surrounding YAML parseable and the byte
  offsets honest — so everything inside `{{ … }}` was invisible to the index.
  Provenance parsed the actions separately and could say which templates read a key,
  but `fr refs` on that key answered zero and a rename rewrote `values.yaml` and
  nothing else, listing every template use as a textual occurrence to check by hand.
  The `.Values` paths are now extracted as references spanning the final segment
  only, so renaming `image.tag` rewrites `tag` and leaves `image` alone.

  Resolution is scoped to the chart, because two charts in one workspace routinely
  declare `image` or `name` and a global match would point a template at a
  neighbour's values file. The segment before the key is carried as the reference's
  receiver, which is what distinguishes `image.tag` from an unrelated top-level
  `tag`, and only files named `values*.yaml` are candidates — every template in the
  chart is YAML with keys of its own. `{{ .Release.Name }}` is still reported as a
  textual occurrence rather than rewritten, because it is not a values key.


- [x] B35: **`--path` filters matched nothing, and reported that as nothing found.**
  They were built by joining the default root `.`, giving `./pkg/action`, which
  starts-with-matches no absolute path in the index. Every filtered report came back
  empty and read as a clean bill of health. Filters are now resolved against the
  workspace root and canonicalised, and a path that does not exist is an error.

- [x] B36: a relative path in a target was read from the shell's working directory
  rather than the workspace `-C` names, so `fr -C ../helm refs pkg/x.go:3:6` failed
  with "reading pkg/x.go: No such file". Four sites had their own
  `canonicalize().unwrap_or(…)`, which kept the unusable path and let the failure
  surface two frames later; they now share one resolver that says where it looked.

- [x] B37: a field access resolved to a local variable. `i.provData` bound to a
  `provData, err := …` two lines up, because the nearest-definition rule ran before
  anything checked that a member access can only name a member. The field then had no
  references at all and was reported as dead.

- [x] B38: nothing tested the command line. Every test called the library directly,
  which is why B35 and B36 — both entirely in the layer between an argument and that
  library — were invisible. `tests/cli.rs` runs the binary: argument parsing, path
  resolution, exit codes, and the text a person reads.


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
