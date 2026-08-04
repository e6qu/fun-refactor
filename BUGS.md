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

- [x] B104: **the browser scale sweep covered fourteen of sixteen languages while
  claiming all of them.** `.java` was missing from its `PARSEABLE` set, so the two Java
  files in the bundled sample were invisible to it. Adding the extension and asserting
  the claim then showed the gap was older and larger: `html` and `scss` were never
  probed either, because they hold few definitions and the stride falls past them.
  Raising the probe count would have fixed today and broken the next time a language
  was added, so one probe per language goes in first and the stride fills the rest —
  coverage as a property of the sampling rather than of a tuned constant. The claim
  itself is now an assertion that names what it missed.

- [x] B103: **the playground's own UI said "fifteen languages"** in three places,
  including the line printed under the file tree every time the bundled sample loads.
  The documentation sweep had covered `docs/*.html` and not `web/src/`, which is the
  half of the site that is compiled rather than served.

- [x] B102: **two pages disagreed about the same measurement.** `16,525 Grafana files
  across 13 languages` counts what *Grafana* contains, not what this tool supports, and
  a find-and-replace over "13 languages" changed it on both pages. It was caught on
  `index.html` and missed on `why.html`, so the two then disagreed — which is how the
  error announced itself. Also stale there: the test count (1,267, now 1,376) and the
  not-applicable count.

- [x] B101: **the README's status section stated a fixed bug as current, twice over.**
  It carried two "known limitations" paragraphs that contradicted each other, and the
  second said Helm `.Values` references "are not yet resolved" — which B7 fixed. The
  capability counts were stale too (`216 of 300`, when the matrix is `245 of 336`), and
  `fr recipe`, `fr openapi` and `fr translate` were missing from the command list.

- [x] B100: **the bundled playground sample had no Java file,** so the one language
  added most recently was the one language the playground could not demonstrate. Two
  files under `web/sample/agent/`, chosen to exercise what only an annotated language
  reaches: a `main` the JVM is pointed at, a `@RestController` Spring calls, and a
  private method that is genuinely dead.

- [x] B99: **`annotated_with` only looked *above* a definition.** Rust and Python put an
  annotation on its own line above; Java puts it *inside* the declaration, in the
  `modifiers` node, which is within the symbol's own span. Reading only above it made
  every `@Test` method in the language look like dead code — and the same rule answers a
  recipe's `annotated-with=`, so it was wrong in two places at once.

- [x] B98: **the capability table claimed `inline --call` for every imperative
  language.** It was the one cell derived from the language's *class* rather than from
  the operation's own predicate, so adding a language to the enum claimed the capability
  before a line was written. `enclosing_call` also matched only node kinds containing
  "call", and Java's is `method_invocation` — so the table promised something the
  operation could not find a single instance of. Both fixed; `inline::supports_call` is
  now the authority.

- [x] B97: **the capability table and `move` disagreed about Java.** The table said it
  could be moved and the operation refused it — the table lying about the tool in the
  tool's own words, because `supports_move` was a blocklist and the refusal was a match
  arm somewhere else. `why_not_move` is now the single authority and both ask it.

- [x] B96: **six capability reasons were false about Java.** The fallback strings were
  written when every unsupported language was markup or configuration, and they say so:
  `extract variable` told a reader that **Java** "has no binding form: a reusable value
  here is a CSS custom property", and `entry points` told them Java "is a stylesheet". A
  reason untrue about the language it is given for is worse than no reason, because the
  whole point of the table is that the empty cells explain themselves. Reasons are now
  chosen by language class, and four of the six turned out to be capabilities Java
  simply had: micro-rewrites, organize imports, inline call and entry points all work
  and are now claimed.

- [x] B95: **a recipe computed both workspace analyses whether or not an expectation
  asked for either.** `find_unused` and `duplicates` ran over the whole workspace,
  twice — before and after — for every recipe, including ones with no `expect no-new`
  in them. Over helm that was most of a minute answering a question nobody asked.

- [x] B94: **a recipe step rebuilt the whole index after every subject.** Correct and
  unusable: five files of helm took two minutes forty, because each subject re-indexed
  all five hundred and thirty-nine. It is needed exactly when a previous edit could
  have moved the text this subject is about — when its own file has already been
  edited, or once an operation has edited a file other than its subject's, as a rename
  does. Otherwise one index does for all of them. Same result, 48 seconds.

- [x] B93: **`rewrite` treated a file it had nothing to do in as a refusal.** The
  selector chooses *files*, so a file with no wrapping `if` is one that needed no work
  — but it was reported as refused, and `on-refusal stop` is the default, so a run
  abandoned itself on the first ordinary file. Over one package of helm that was three
  of five. It also means `applied` now counts *sites*, which is the unit `limit` is
  about for this step.

- [x] B92: **applying a micro-rewrite across a file asked at every byte offset.** Each
  ask reparses, so it was O(bytes × parse) where it wanted to be O(anchors × parse).
  All three transformations anchor on a conditional or a negation, and those offsets
  come from one parse of the file.

- [x] B91: **`fr signature move` could produce Python the interpreter rejects.**
  Reordering `def circ(r, units="m")` to `def circ(units="m", r)` puts a defaulted
  parameter before a required one, which Python refuses with *"parameter without a
  default follows parameter with a default"* — and tree-sitter parses it without
  complaint, so the engine's reparse check could not see it. The refactoring has to
  know the rule itself; it now refuses for Python and TypeScript, naming what would
  break, and leaves the languages with no defaults alone.

- [x] B90: **every Go function body was carried into a translation as a single
  comment.** tree-sitter-go puts a `statement_list` between a block and its statements,
  so the reader saw one unknown node where a body should be. Invisible to the
  round-trip tests, because a body that is entirely a comment still parses — found by
  translating a Go file to Rust for a demo page. `return x` was the same shape a second
  time: the value is wrapped in an `expression_list`. Rust → Go now carries nothing at
  all for an ordinary function.

- [x] B89: **the recipe runner planned each step against the file on disk.** The
  refactorings read source through `crate::vfs`, not from the runner's in-memory
  workspace, so a plan made after one step was measured against the text before *any*
  step ran: `edit at 1..301 extends past end of file (226 bytes)`. The in-memory
  backing was gated to the browser build although nothing in it is wasm-specific; it is
  compiled everywhere now, and the runner installs the workspace on it before each
  step. `vfs::use_filesystem()` hands it back before anything is written.

- [x] B88: **the recipe runner planned every selected symbol against one snapshot.**
  Two deletions in one file produced `conflicting edits: 0..396 overlaps 26..170`,
  because one deletion moves every span after it. Subjects are named rather than
  identified by `SymbolId` — an id does not survive a rebuild — and each is planned
  against an index built from what the previous one left.

- [x] B87: **the recipe report dropped the warnings its steps produced.** `fr rename`
  prints what it left alone — a reference that resolved too weakly to rewrite — and the
  recipe swallowed it, so a step that left work behind reported a clean run. That is
  the accept-and-ignore this codebase bans elsewhere. Warnings are now a field of the
  step report and print under `left`.

- [x] B86: **a Java method call resolved to nothing.** `receiver_of` decided the
  receiver positionally — "the member is the last child" — which holds for Go's
  `selector_expression` and not for Java's `method_invocation`, where the argument list
  follows the name. Every method call in the language had no receiver and so never
  reached even `field-based`. It now prefers the field the grammar names (`object`,
  `operand`, `receiver`) and falls back to the positional rule.

- [x] B85: **`fr signature` and the CLI each had their own copy of the change parser,**
  and the same for `path:line:col-line:col`. Both moved beside what they produce —
  `Change::parse` and `span::parse_range` — because a recipe's `signature "…"` and
  `extract … at "…"` write the same syntax and two parsers for one syntax is two
  chances to disagree.

- [x] B84: **a bare `xs.filter(p)` did not translate, and a comprehension that kept
  every element it selected wrote out an identity `map`.** The TypeScript reader
  recognised `xs.filter(p).map(f)` and nothing else, so the commoner of the two came
  out as a comment; the writer emitted `xs.filter(p).map((x) => x)` for
  `[x for x in xs if p(x)]`, which is the same thing with three extra words. Both sides
  of the pair the translation page showcases.

- [x] B83: **inlining a variable was refused whenever any name in its value appeared
  anywhere else in the file.** The capture check asked "does this name mean the same
  thing at the use site?" by taking the first reference with that name starting within
  *two hundred bytes* of the use site — not a question about scope at all. In a
  seven-line file every reference is within two hundred bytes, so
  `total = price_of(order)` could not be inlined because the *other* function's
  parameter is also called `order`. The index records lexical scopes and
  `definition_at` was being computed on the line above and discarded (`let _ = at_use;`).
  Now both sites resolve the name through their own scope chain. A genuine capture is
  still refused, and it has its own refusal — `NameCaptured` — because "renaming would
  shadow or collide with it" described neither the operation nor the fault.

- [x] B82: **`fr signature X 'add:1:flag: bool:false'` — the example in the tool's own
  error message — did not work.** `splitn(4, ':')` handed the parser only the first
  word of the declaration and dropped the rest, so any declaration containing a colon,
  which is to say any typed one, failed with the message that recommends it. Three
  fields, not four: everything after the position is one field and the argument comes
  off its end.

- [x] B81: **the catalogue page's report pane dropped three quarters of what the tool
  said.** `docs/panes.js` separated the diff from the report by matching diff lines
  with a pattern, and a diff context line starts with a space — as does every indented
  line of a report, which is most of one. Split by position instead.

- [x] B80: **`commit` chose how to write by feature flag rather than by where the
  writes go.** `#[cfg(feature = "cli")]` selected filesystem staging, so a build with
  both `cli` and `wasm` compiled in staged a temporary file beside a path that exists
  only in a browser's memory and has no directory on disk — every refactoring in that
  build would fail to apply. Neither shipped build has both, so it was latent; it
  surfaced the moment the two feature sets were compiled together, which is now what
  CI does. Where a write lands is a fact about the active backing, and the question
  moved to `vfs::is_in_memory()`.

- [x] B79: **`src/wasm.rs` could not be compiled without a wasm toolchain, so every
  edit to the browser API was checked only by CI.** `vfs::Handle`, `new_handle` and
  `activate` were gated on `target_arch = "wasm32"` although nothing in the in-memory
  backing is wasm-specific, and the constructor called the libc shim unconditionally.
  The cost was paid: a field added to a struct at six call sites was missed at one,
  `cargo test` and `cargo clippy -D warnings` both passed, and the playground job found
  it three minutes later. The backing now follows the *feature*, with the memory map
  shadowing the filesystem while a workspace is handed over — so `activate` is not a
  silent no-op on a host — and `Workspace::load` takes plain Rust values so only the
  `JsValue` conversion is left in the constructor. `tests/wasm_native.rs` drives nine
  cases through the API under `cargo test`, and CI runs clippy and the suite with
  `--features wasm`. It also turned up B80 and a clippy lint in `src/wasm.rs` that
  nothing had ever compiled.

- [x] B78: **a foreign name that is a keyword in the target made the whole file
  unwritable.** `select` is a name sqlmodel exports and a keyword in Go, so
  `select(User)` was output Go's grammar rejects and the translator refused the file
  outright — correct, and useless: the reader gets nothing instead of a draft with one
  line to fix. Reserved words are now escaped (`r#select` in Rust, a suffix elsewhere)
  and every escape is reported.

- [x] B77: **Python's `*`, `/`, `*args` and `**kwargs` were read as ordinary
  parameters.** `def create_user(*, session, user_create)` produced
  `export function createUser(*: unknown, …)`, which TypeScript will not parse — caught
  by the translator's own parse check, on real code, in a file 1,300 fixture tests had
  never seen. A `*` is a rule about the parameters around it, and dropping it silently
  would be worse: the signature would look carried when the way callers must invoke it
  had changed. `ParamKind` now models all four and `signatures_with_changed_calls`
  counts the difference.

- [x] B76: **an optional chain was written away.** `session?.user.id` came out as
  `session.user.id` and then, inside an object literal, as **`None.id`** — code that
  compiles, runs, and throws where the original returned undefined. Two causes: the
  reader ignored `optional_chain`, and `has_unsupported_expr` did not look inside
  `MapLit`, `Template` or `Comprehension`, so a statement containing an untranslatable
  sub-expression was written anyway. That check is now exhaustive with no `_` arm, so
  a new variant cannot join them quietly.

- [x] B75: **a TypeScript type assertion became `None`.** `params.postId as string` was
  an unhandled node and fell to the catch-all. `as`, `satisfies` and `!` are assertions
  to the type checker with no runtime effect whatever, so the translation is exact
  rather than a gap.

- [x] B74: **comments were reported as untranslatable constructs.** Every one of these
  languages has comments and only the marker differs. Reading one as a failure put
  ordinary prose in the output under a "not translated" marker and counted it among the
  real gaps — which is how a fidelity count stops being read.

- [x] B73: **`try`/`catch` had no counterpart in the IR, so whole handler bodies came
  out as one comment.** Python and TypeScript both have it. A typed `except` becomes an
  `instanceof` test inside TypeScript's single `catch`, with a trailing `throw` so an
  unmatched error still propagates; Rust and Go carry it, since neither has anything a
  catch block translates into. `instanceof`/`isinstance` and `throw`/`raise` came with
  it. On the hardest route in the corpus this took the count of carried constructs from
  15 to 3.

- [x] B72: **the Python writer decided "did I write anything" in each match arm.** A new
  arm that forgot `wrote = true` left a stray `raise NotImplementedError` after a
  perfectly good body — which is exactly how the `try` arm arrived broken. It is a
  property of the statement and is now asked once.

- [x] B71: **the naming convention was applied at declarations and not at uses.**
  `interface User { userName }` became `class User: user_name` whose bodies still said
  `.userName`, because each site re-cased with whichever helper was to hand. One map,
  built from the declarations and consulted everywhere. Three things fell out of it:
  a name the module does not declare is foreign and is never re-cased; record fields
  are a separate namespace, since a Go `Reading` with an exported `sensor` field is
  `Sensor` while a parameter of the same name stays lowercase; and `snake_always` split
  before every capital, turning `HTTPServer` into `h_t_t_p_server` and `MAX_RETRY_COUNT`
  into `M_A_X__R_E_T_R_Y_C_O_U_N_T`.

- [x] B70: **the Next.js route matcher required a leading slash, so no relative path
  was ever a route.** `is_api_route` and `route_for` both tested
  `path.contains("/pages/api/")`, and the leading slash was there to stop `capi/`
  matching. The cost was that `pages/api/users.ts` — exactly what a caller who has a
  workspace-relative path hands over — was refused as "not a Next.js API route", while
  `route_for` happened to answer correctly for the App Router by accident. Both now
  match on path *components*, which is the thing the rule was always about: a `pages`
  directory immediately followed by `api`, or the last `api` above a `route.ts`.

- [x] B69: **`await` was not in the IR, so every line containing one was carried
  verbatim.** Three of the four languages have it and mean the same thing by it; only
  the spelling differs, prefix in Python and TypeScript and postfix in Rust. `const
  body = await request.json()` — a line with an exact Python counterpart — came out as
  a comment. `Expr::Await` now carries it, and the Go writer, which has no
  counterpart, carries it rather than dropping the keyword and turning a suspension
  point into a plain call.

- [x] B68: **the Next.js translation counted handler signatures as failures and
  overwrote the helper count.** `fidelity.functions = handlers.len()` discarded
  whatever the ordinary writer had counted for the file's non-handler functions, and
  `signatures_complete` was never incremented for a handler at all — so a translation
  that got every signature right reported `0/2 complete`.

- [x] B67: **the Next.js translation printed a Rust `Debug` dump where the source
  should have been.** A statement that read the request or context object was replaced
  with `Unsupported { source: format!("{stmt:?}") }`, so the output carried
  `# Let { name: "id", ty: None, value: Some(Field { .. }) }` instead of
  `# const id = context.params.id;`. The IR's `Stmt` has no source text and inventing
  one from its `Debug` form shows the reader less than the translation does. Root
  cause was the premise: those statements are not untranslatable. `context.params.id`
  is *redundant* — FastAPI supplies the path parameter — so it is dropped with a note,
  and the request object is *kept*, because `NextRequest` is Starlette's `Request`.

- [x] B65: **a template metavariable the pattern never bound was caught by the wrong
  check.** `restructure 'len($X) == 0' 'not $Y'` produced the literal text `$Y` in the
  source, and the engine's reparse check rejected the result with "parses cleanly now
  but would not after the change" — true, and silent about the actual mistake. The
  pattern and template are now compared before anything is rewritten: *"$Y is not bound
  by the pattern, so there is nothing to put there — the pattern binds $X."*

- [x] B66: **`?repo=` picked the workspace for the JSON renderings and was ignored by
  the page.** One parameter meaning two different things depending on `render_as` is a
  trap for anyone sharing a link.

- [x] B64: **a Rust method call resolved to a Zig method, and a rename rewrote it.**
  Resolution matched candidates by name across the whole workspace without asking what
  language they were written in, so `out.push(…)` in Rust — a `Vec::push` — resolved to
  `Ring.push` in a `.zig` file at `import-qualified`, a tier the tool rewrites.
  Renaming the Zig method turned the Rust call into `out.pushReading(…)`: two
  languages, no relationship, an ordinary-looking diff. `lang::may_resolve_across` now
  enumerates the boundaries a reference may cross — markup to stylesheet, TSX to
  TypeScript, template to values — and no pair of imperative languages is among them.
  Found by measuring every crossing in the bundled sample rather than by a test that
  thought to look.

- [x] B63: **`fr refs` under-reported for anything declared more than once.** A CSS
  class written in a stylesheet and again in a theme is one class; references resolve
  to one of those sites, and `references_to` counted per site. So `fr refs` on the
  second declaration of `.sensor-table` found nothing while `fr rename` at the same
  position changed five sites — you look before you leap, see nothing, and five things
  move. `usages` already followed the definition group; `refs`, the browser and the
  call graph did not. Fixed in the index, so all of them agree.

- [x] B62: **a rename buried its success under twelve thousand warnings.** Renaming a
  YAML key called `path` reports every string and comment in the workspace containing
  that word — 12,032 of them across `psf/requests` — one per list item, which is a
  wall of "left unchanged" that reads as a failed rename rather than a finished one
  with a long footnote. They are now grouped by kind, explained, capped at
  twenty-five, and the count is stated exactly: *"118 × textual-occurrence … and 93
  more, not listed"*.

- [x] B61: **an edit re-parsed every file in the workspace.** `Workspace::apply`
  rebuilt the index from source after every change, so a rename touching one file
  re-parsed the other four hundred and thirty-seven: 3.1 seconds in `zod`, which is
  long enough that a person concludes the button did not work. Extraction is per-file
  and parsing dominates it; resolution is global and cheap. Splitting the two —
  `Index::build_from_facts`, and per-file facts kept in the workspace — makes an edit
  cost the files it wrote. zod 3144ms → 624ms, ripgrep 860ms → 149ms.

- [x] B60: **a file a refactoring created was never indexed.** `fr move` to a new
  module wrote the file into the virtual filesystem, and the re-index walked only the
  paths present at load, so the destination had no symbols, did not appear in the file
  list, and was invisible to every later question. The move reported success.

- [x] B59: **a message rendered the workspace root as nothing.** `Path::display` on
  the parent of a top-level file is the empty string, so `fr move` to a file at the
  root printed *"no .go file in declares a package"*. Fifteen messages across move,
  signature and provenance interpolate a directory this way; `vfs::describe_dir` now
  names it.

- [x] B58: **the coordinate button claimed a copy that had not happened.** The
  clipboard is unavailable over plain http on any origin but localhost, and
  `writeText` rejects when the page is not focused. Both were swallowed by an
  optional call, and the button said "copied" regardless. It now says what happened
  and shows the coordinate to select by hand.

- [x] B57: **the status bar reparsed the open file on every keystroke.** Answering
  "what is the cursor on" parsed the whole file — 17ms on `requests/models.py`, paid
  on every arrow key, which is a dropped frame each time one repeats. The parse is
  memoised for the one file that is open, keyed by the source text, so there is
  nothing to invalidate: an edit changes the text and the next question misses. 3ms.

- [x] B56: **`DefinitionRole` serialised as a Rust variant name.** `--json` and the
  browser printed `Primary` where every other enum here emits a kebab-case token and
  the terminal printed the prose "definition". Now `primary`, and the view renders
  the same words the terminal does.

- [x] B55: **`fr unused` named symbols `fr delete` could not remove.** The span the
  index keeps is the span a *rename* rewrites. For `export const defaultLimits = {…}`
  that is the declarator, so deleting exactly it left `export const ;` and the
  engine's reparse check rejected the whole edit — loudly, but the report had
  promised something the tool could not do. Zig and TypeScript struct fields had the
  same shape. It had been fixed once for CSS as a special case; it is one rule, and
  generalising it (climb while the symbol is the only child of its kind; take the
  separator when it is not) let the CSS-specific code go.

- [x] B54: **a CSS class used by the markup was reported as dead.** A class declared
  in both a stylesheet and a theme is one class, but uses were counted per
  declaration site: the markup's reference resolves to one, so every other
  declaration read as unreferenced. `.nav-link`, worn by three anchors, was reported
  dead twice — while `fr delete` refused to remove it and named those same three
  uses. Kinds that admit several declaration sites are now counted as one entity.
  Found by running `find_unused` and `delete` against each other over a polyglot
  workspace: thirteen disagreements out of fifty-nine, now zero, and checked across
  nine languages rather than one Rust function.

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
