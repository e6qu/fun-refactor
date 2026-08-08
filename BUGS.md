# BUGS

Known defects and limitations, and their status. Updated alongside PLAN.md at every
stage.

Format: `- [ ] B<N>: <symptom> — <where> — <status/notes>`

Open entries are characterised limitations — the behaviour is reported and no operation
silently does the wrong thing — with one exception, B258, which is uncharacterised.

## Open

- [ ] B263: **a Terraform input variable and a local sharing a name are one symbol.**
  `var.x` and `local.x` are separate namespaces, and the index records both declarations
  as `SymbolKind::Variable` with no qualifier, so nothing tells them apart. With
  `variable "thing"` and `locals { thing = … }` in one file, `fr refs` on the variable
  returns two references — `var.thing`, which is its, and `local.thing`, which is not —
  and `fr refs` on the local returns none. Both drop to `field-based`, so `fr rename`
  rewrites the declaration and leaves every use for review; nothing is corrupted, and
  nothing is usable either. `fr symbols` prints the two declarations identically, so the
  ambiguity error's advice to "name one of these" cannot be followed. Without a name
  collision both resolve `exact` and correctly.

  In `terraform-aws-vpc`, 18 of 81 locals share a name with a variable.

  The reference side is one line: the query captures the namespace as `@_ns` and
  discards it, next to a `data.TYPE.NAME` pattern that captures its type as
  `@reference.type`. The symbol side is not: `var` and `local` appear nowhere in a
  declaration, and a query cannot synthesise a name, so the qualifier would have to be
  assigned in `extract.rs` by language and kind — which changes every HCL qualified name
  and so the cache schema. Exporting the namespace alone changes nothing, which I
  checked before recording this.


- [ ] B258: **`a_rust_number_leaves_its_width_behind` failed once and has not repeated.**
  During one `cargo test --all-targets`, the Java writer emitted Rust's `0usize` /
  `1i32` suffixes, which that test exists to catch. It has since passed 5/5 runs in
  isolation and 3/3 full runs, on the same commit. Ruled out: `tests/transpile.rs` never
  activates a VFS handle, so a stale one cannot be the cause; the fact cache writes to a
  temporary file and renames, so a concurrent reader cannot see a partial entry.
  Mechanism unknown. Recorded rather than dismissed because the test would have caught a
  real defect and did, once.

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

- [ ] B234: `tree-sitter-python` cannot read a type parameter default —
  `type A[T = int] = float`, PEP 696, Python 3.13. A type alias without one reads
  cleanly. Found in `psf/black`'s test data. Upstream grammar work.

- [ ] B233: `tree-sitter-python` cannot read a starred *literal* in an unparenthesised
  tuple. `g = 1, *[2]` is ordinary Python, and so are the `*(2,)`, `*{2}` and `*"ab"`
  forms; a starred *name* or *call* in the same position reads fine, and so does the
  whole thing in brackets. Found in `psf/black`'s `expression.py`, where the line is
  `g = 1, *"ten"`. Upstream grammar work.

  Both are pinned by `tests/known_grammar_gaps.rs`, from both sides: the failing form
  and the neighbouring forms that work. A grammar upgrade that fixes one should retire
  its entry, and one that starts reading it *without* an error node while building the
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
  in a long list of properties. Upstream grammar work.

- [ ] B231: `tree-sitter-typescript` cannot read an import type —
  `import("@babel/types").Statement[]` in a type position. Valid TypeScript and common in
  generated declarations; found in `vuejs/core`'s compiler-sfc. Upstream grammar work.

- [ ] B133: `tree-sitter-zig` requires at least one member in a struct, so it cannot
  parse `const Foo = struct {};` — which is ordinary Zig, and is the only parse failure
  across 29 files of Zig's own standard library (`json/static_test.zig:465`). The tool's own check would
  therefore refuse to write a correct file, so an empty record is written with an empty
  `comptime {}` block in it, under a comment saying why. That block does nothing, both
  Zig and the grammar accept it, and the alternative was refusing to translate a type
  with no fields at all. Upstream grammar work.

## Fixed

- [x] B268: **five more refusals reported a resolution that had not happened.** B266 fixed
  one site; sweeping every `Refusal::TooWeak` found five others of the same shape, each
  identifiable by what it filled the field with. A site reporting a real reference writes
  `confidence: reference.confidence`; these five wrote `Confidence::NameOnly` because no
  reference existed to ask. Their own text says so — "sources a path that is not a
  literal, so what is in scope there cannot be known", "a call site inside a syntax error
  is invisible to the index" — and the wrapper prefixed each with "resolution is only
  'name-only'". `Refusal::Unknowable`'s doc comment names "a shell script that sources a
  path computed at run time" as its example, which is two of these five verbatim.

  `TooWeak` now takes a `ResolvedConfidence`, whose field is private to `model` and which
  only `Reference::resolved_confidence` produces, so the variant cannot be built without
  a reference to take a confidence from. Verified the compiler refuses:
  `ResolvedConfidence(Confidence::NameOnly)` outside `model` is
  "private tuple struct constructor". `signature.rs` no longer names `Confidence` at all.


- [x] B267: **a remedy was offered where it would not work.** Every indeterminate-argument
  refusal ended with "quote it to make it one argument", which is true of an unquoted
  `$x` and false of `$@` — quoting that gives one word per parameter, the same problem
  again, as the function's own comment says. The advice now travels with the problem that
  earns it, so `$@` gets none and a glob gets "quote it to stop the shell expanding it".

- [x] B266: **an argument the shell decides at run time was reported as weak resolution.**
  `f $x two` refused with "resolution is only 'name-only'", but the call resolved; what
  is unknown is how many words `$x` becomes. `Refusal::Unknowable` exists for exactly
  this and its doc comment names the symptom — "resolution is only 'exact'", a sentence
  that contradicts itself — so the fix was already written down and this site had not
  been changed. Two clauses composed by two functions also garbled the sentence: the
  remedy landed mid-sentence, leaving "so the position of everything after it is only
  known at run time" attached to the fix rather than the problem.

- [x] B265: **a signature change refused by talking about renaming.** Two bash functions
  of one name make every call site ambiguous, which is a reason to refuse — but it raised
  `Refusal::NameCollision`, whose message is "'f' is already defined in …; renaming would
  shadow or collide with it". Nothing is renamed and nothing is introduced; both
  definitions were there. `Refusal::AmbiguousDefinition` says what is actually wrong, and
  `NameCollision`'s own wording no longer says "renaming" either, since `extract` and
  `fr signature add` raise it while introducing a name rather than changing one. An
  existing test asserted the old wording, so it had pinned a message that was wrong.


- [x] B264: **`zig-test` matched a test's description, not the construct.** Zig writes a
  test as `test "any prose you like" { … }`, and the query makes that description the
  symbol's name — so `name_prefix: test` matched the tests whose description happens to
  begin with "test": 12 of the 495 in Zig's own standard library. The other 483 were
  reported as dead code, along with everything only they called. Matchers gained
  `declaration_keyword`, which reads the declaration's opening keyword and requires it to
  end there, so `const testing = …` does not match. Entry points 12 → 472; dead-code
  findings over that corpus 643 → 204, and 538 → 99 with `--internal`. The other five
  repositories are unchanged.

  This is the third catalogue predicate that is not a property of a name, after Python's
  `__main__` guard and Next.js's `"use server"`.


- [x] B262: **`fr unused` reported HCL blocks Terraform gives no address to.**
  `terraform {}`, `required_providers {}`, `lifecycle {}` and a `dynamic` block's
  `content {}` carry no label, so nothing in the language can reference one and every one
  of them answers "nothing uses this". `terraform-aws-vpc` reported 46, all of those four
  shapes, out of 46 block findings. A labelled block takes its name from a string label
  and an unlabelled one from the block-type keyword, so the quote before the name settles
  it — no list of block types to keep up with as Terraform adds them. The repository's
  answer drops from 369 to 323, and what remains is Markdown headings.


- [x] B261: **two capability predicates returned `true` for every language, behind
  branches that could not run.** `delete::reports_unused` and `duplicates::supported`
  both ignored their `language` argument and answered `true`. Each caller in
  `capabilities.rs` tested it and had an `else` arm building a `Support::NotApplicable`
  with a refusal message — "nothing in this language declares a name that something else
  could fail to use" — that nothing could ever print. The matrix read as computed per
  language where it was constant. Both predicates and both branches are gone; the two
  cells state `Support::Yes` with the reason in a comment. `fr capabilities` still
  reports 270 supported.

- [x] B260: **three commands took two or three bare booleans in a row.**
  `cmd_extract(&cli, range, name, *function, *all, *write)` passes three, and the clap
  field names differ from the parameter names, so any two could swap and compile. Each
  now has a type: `Extract::{Variable, Function}`, `Occurrences::{First, All}`,
  `Inline::{Variable, Call}`, and `FlagValue(bool)` for the value a removed flag is fixed
  at, which sits beside `write` in `cmd_remove_flag`. No two parameters of any of the
  three share a type now. The other 23 functions taking `write: bool` keep it: `write` is
  their only boolean, so nothing can swap with it.

- [x] B259: **line and column travelled as a bare `(usize, usize)` in six places.**
  `LineIndex::line_col` returns `LineCol`, and six helpers took that value apart and
  returned the pair — `delete.rs` built a `LineCol` and immediately destructured it. A
  caller reading the two in the wrong order gets a position that looks plausible and
  points somewhere else. All six return `LineCol` now. `Cache::stats` returned hits and
  misses the same way and returns `CacheStats`.


- [x] B256: **`fr unused` did not treat an HTML attribute value as a string.**
  `is_string_kind` matched node kinds containing "string"; the HTML grammar names an
  attribute value `attribute_value`. Templates name code there — `th:text="${owner.address}"`,
  `v-on:click="submitOrder"`, `class="table-striped"` — so the correction that spares
  names spelled in strings missed all of them. spring-petclinic: 80 CSS classes reported
  dead while its templates used them.

- [x] B255: **`fr unused` reported containers of entry points.** JUnit constructs a test
  class to run its `@Test` methods; nothing names the class. spring-petclinic: 11
  reported. The check walks the containment chain rather than testing the language, so it
  covers Rust `mod tests` and Python classes of pytest cases too. A dead method beside a
  live test still reports.

- [x] B254: **`fr unused` reported JavaBean accessors reached by their property.**
  `Owner::getAddress` reported dead; the template writes `${owner.address}`, the tests
  write `param("address", …)`, nothing writes `getAddress`. Java templates, JSON mappers
  and Spring's binder all reach a getter by the property name. An accessor whose property
  is named where the method is not is now spared, with the reason. The rule requires an
  uppercase letter after the prefix, so `gettysburg` is not an accessor for `tysburg`.

- [x] B253: **three Spring conventions missing from the catalogue.** `@InitBinder`,
  `@ModelAttribute` and `@Configuration`: the container calls them, the source does not,
  same as the eight callbacks B236 added. B236 came from enumerating what Spring calls;
  these came from running the tool over spring-petclinic.

- [x] B252: **`fr unused` reported every package clause, one per file.** Java classes in
  one package never write the package name, and nothing imports Go's `main`, so no
  package declaration ever has a reference. spring-petclinic: all 49 reported. Removing
  one is a syntax error. Rust's `mod helper;` shares the symbol kind and differs: a child
  module nothing references is a finding, so the exclusion tests the language, not the
  kind.

  B252-B256 together take spring-petclinic's code findings from 35 to 3: a constructor
  Spring calls, a testcontainers `@Container` field, a nested `@TestConfiguration`.

- [x] B257: **`fr unused` printed a count without a breakdown.** spring-petclinic: 3,554
  findings, 3,439 of them in one vendored stylesheet, with nothing in the output saying
  so. An answer of 50 or more now lists its top five kinds, plus the file holding the
  findings when one file holds over half. vuejs/core: 1,640 keys in `pnpm-lock.yaml`.
  Nothing is excluded from the analysis.


- [x] B251: **a recipe's misspelled predicate value blamed the repository.** `kind=functoin`
  matched nothing and the step failed with "matched nothing. That is not success" — true,
  and unhelpful: nothing in the workspace was wrong. The predicate's *name* has been
  checked with a suggestion and the full vocabulary all along, for exactly this reason;
  its value was not. `kind` is now checked by parsing the value into `SymbolKind`, so the
  vocabulary comes from the type rather than a list kept beside it, and serde's error
  names the alternatives — which reads correctly only because B247 made those spellings
  the same as the ones the tool prints. `lang` gets the same treatment via
  `Language::from_name`, with the "did you mean" the predicate names already had.

- [x] B250: **three matcher conditions did not count as conditions, and an empty matcher
  matched nothing without saying so.** The empty-matcher guard listed `is_some()` checks
  that had drifted from the fields that exist; `symbol_kind`, `exported` and `top_level`
  were absent, so a rule using only one of those counted as empty and matched nothing.
  The guard returned false rather than reporting. Now one method on `Matcher`
  destructures the struct, so a new field fails to compile instead of being omitted.
  Two callers: the loader refuses such a catalogue and names the rule; `rule_applies`
  keeps returning false as a backstop for a `Catalog` assembled directly rather than
  loaded.

- [x] B249: **`fr type --json` answered with numbers nobody can use.** It serialized the
  analysis struct directly, so `"symbol": 1` and `"defined_at": 0` were `SymbolId`s —
  positions in one run's index, meaningless to whoever reads the output, and unstable
  between runs. `defined_at` read like a line number. Every other command answers with a
  qualified name and a place, and the text rendering of this one resolved them all along;
  only the machine-readable half did not. Both now say the same thing.

- [x] B248: **`as_str` named two different things.** On `SymbolKind`, `Confidence`,
  `EntryKind` and `HierarchyBasis` it returns an identifier: a token that goes into JSON,
  into a catalogue, into a command line. On `Capability`, `Basis` and `DefinitionRole` it
  returns display text — "call graph", "from the literal", "also declared here". Nothing
  separated the two, so nothing said which had to match their serde spelling; B247 is the
  consequence. The display three are now `label()` and `describe()`. A round-trip test
  covers the identifier ones, reading the spellings out of the exhaustive `as_str` match
  so a new variant needs no list updated.

- [x] B247: **the tool's JSON could not be read back into the tool's own types.**
  `SymbolKind` derives serde with `rename_all = "snake_case"` and has a hand-written
  `as_str` that the output actually uses, and three of twenty-one variants disagreed:
  `as_str` gave `type`, `link-def` and `element-id` where serde expected `type_alias`,
  `link_def` and `element_id`. So `fr symbols --json` emitted `"kind": "type"` and
  deserializing it failed. Nothing was checking, because nothing had reason to think two
  spellings existed. Three per-variant renames, and the cache schema is bumped because
  cached facts carry the old spelling.

- [x] B246: **a misspelled value in a catalogue loaded and matched nothing.**
  `deny_unknown_fields` rejects a misspelled *key*; a misspelled *value* was accepted,
  because `symbol_kind` was a `String` and `languages` a `Vec<String>` compared against
  the real enums by name. `symbol_kind: functoin` and `languages: [pyhton]` both parsed,
  loaded and never fired — indistinguishable from a rule that is present and simply never
  true, which is the failure mode the `annotation_argument_prefix` check was added for one
  PR earlier. Both are parsed into the type they denote now, so the error arrives at load
  with the line, the column and the values that would have worked. `*` keeps meaning every
  language, as `AppliesTo::Any` rather than a magic string.

  `Rule.provenance` went with them: a `String` field defaulting to `"manual"`, set by no
  catalogue and read by nothing — a distinction the type promised and the code never made.

- [x] B245: **the same overclaim, one branch up — and B243's fix did not reach it.** Step 1
  settles a name by lexical scope, and let itself settle a *member* access too whenever
  only one member in the workspace had that name, reasoning in its comment that there is
  then "nothing to be wrong about". There is: the workspace does not contain every type.
  With the call written inside the declaring class, `client.total()` on an unannotated
  parameter was still `exact` and still rewritten after B243. The two branches held the
  same belief and fixing one left the other, which is what a rule kept at its use sites
  does — there are twenty-eight places in the resolver pairing a symbol with a tier, and
  each is a chance to disagree with the others. The rule now lives in one place:
  `resolve_one` resolves, then caps what the answer may claim, and the branches below it
  no longer decide. Three receivers stay known — `this`/`self`, a module path, and an
  import binding — because each names something the source declared. A further 26 of
  black's exact edges and 77 of vuejs/core's move to field-based, on top of B243's.

  The related illegal state, `(None, Exact)` — "I cannot say what this is, and I am
  certain" — is representable in the same pair and is what `call_graph.rs`'s
  `resolved.is_some() && confidence.is_safe_to_rewrite()` is guarding against. No branch
  produces it. That is now asserted over whole workspaces rather than by reading the
  branches, since reading the branches is what missed the receiver overclaim twice.

- [x] B243: **a member access claimed to know a receiver it had never seen.** A single
  definition of a name in a file resolved any use of that name to it, at `Exact` — and
  that rule did not exclude member accesses. So one class declaring `total` made
  `client.total()` exact for any `client` whatever, and since only the top two tiers are
  rewritten, `fr rename total sum` silently turned a call on a boto3 client into
  `client.sum()`. `Confidence::FieldBased` is defined as "matched by member name without
  knowing the receiver's type — plausible but unproven; refactorings must not silently
  rewrite these", which is this case exactly: the tier already existed and was not being
  used. Uniqueness is evidence about a *name*; a member access is a question about a
  *receiver*, and nothing at that layer knows one. Calls through `self` and `this` are
  unaffected — lexical scope settles those a step earlier, and they are the large and
  legitimate category. It costs 60 of black's 881 exact edges and 67 of vuejs/core's
  2384; those are now reported for review instead of rewritten unasked.

- [x] B244: **the list of commands named 28 of 32.** `usages`, `implementations`,
  `recipe` and `translate` had all shipped without reaching PLAN.md's closing list. A
  summary line is never wrong about what it says, only about what it leaves out, so a
  test now compares it against the binary's own list of commands — the mirror of the
  existing check that every command the site names still exists.

- [x] B242: **`fr type --help` said the command does not do what the command does.** The
  help read "Nothing is inferred. A binding with no annotation is reported as having
  none", which was true when it was written and stopped being true when inference landed
  — the command's own output says "The source wrote no type here. The above was worked
  out from the evidence named". A reader consulting `--help` would conclude the tool
  cannot do the thing it had just done. The text now describes both answers and why they
  are kept apart.

- [x] B241: **passing locally and passing in CI meant different things.** The `check` job
  listed formatting, clippy and the tests as separate steps, and there was no single
  command that ran them. A local pass over a subset — clippy and the tests but not
  `cargo fmt --all --check` — reported green for a branch CI then rejected, which is
  exactly what happened to the change above. `tools/check.sh` holds the commands and the
  workflow calls it, so the two cannot drift; the wasm feature set, which neither default
  clippy nor the default test run compiles, is part of it.

- [x] B240: **entry-point detection read every file once per rule.** Three of the matcher's
  predicates need the file's text, and each asked for it independently, so the whole file
  was read and allocated once for every rule in the catalogue that reached it. It went
  unnoticed while no `annotated_with` rule applied to TypeScript; adding the NestJS rules
  made `fr entrypoints` on `vuejs/core` take 17.3s against an 8.3s index, with 3.9s of it
  in the kernel. Reading once per symbol — the index groups a file's symbols together, so
  remembering the last one suffices — brings it to 9.4s with 0.25s system time, which is
  faster than before the rules were added. The cache cannot change an answer: a miss
  re-reads.

- [x] B239: **a decorator's name is not unique across libraries.** Adding a rule for
  FastAPI's `@app.patch` tagged twenty-two of `psf/black`'s test methods as remotely
  reachable HTTP routes, and one of its `self` parameters, because `@patch` is
  `unittest.mock`'s far more often than it is FastAPI's. Matching harder on the name
  cannot separate them — `@mock.patch` is as qualified as `@app.patch`. What separates
  them is what the decorator *names*: a route names a URL path, a mock names a module.
  Matchers gained `annotation_argument_prefix`, and the Python route rules ask for `/`.
  Black's twenty-two false positives go and its two real `@app.get("/path/")` handlers
  stay. A path held in a constant — `@app.get(PETS)` — is not matched, which the
  catalogue says rather than hides. Asking for the argument without naming the
  annotation is now rejected when the catalogue loads: `deny_unknown_fields` catches a
  misspelled key, and this catches a well-spelled one in a combination that has no
  meaning, which would otherwise parse, load and match nothing — indistinguishable from
  a framework that is covered and simply absent.

- [x] B238: **a modifier between the annotation and the declaration ended the run.** The
  search for an annotation above a symbol walked back through whole lines, so for
  `export class C` the line before the declaration was `export`, which is not an
  annotation and stopped the walk before it reached the decorator. `export class` is how
  TypeScript writes almost every class, so `annotated_with` did not work on exported
  classes at all — a NestJS `@Controller`, the class the framework is organised around,
  matched nothing while a bare `class C` matched. What precedes a symbol on its own line
  is part of the declaration, not a line before it, so the walk now starts at the
  beginning of the declaration's line — unless that text opens or closes something, in
  which case the symbol is nested inside whatever the annotation above annotates and does
  not carry it. Both halves are load-bearing and both are pinned: `fn outer() { fn inner()`
  must not let `inner` inherit `outer`'s `#[test]`, and the `payload` in
  `@KafkaListener void consume(String payload)` is not itself a queue consumer. That
  second one was a false positive this fix introduced, which the full suite passed over
  and re-running the fixtures caught.

- [x] B237: **a dot in an annotation's arguments hid the annotation.** The name was read
  by stripping the qualifier first and cutting the argument list off second, so the last
  dotted piece of an *argument* became the name: `@ExceptionHandler(RuntimeException.class)`
  read as `class` and `@GetMapping(Routes.PETS)` as `PETS`. The visible symptom was a
  version number in a route path — `@app.route("/v1.0/status")` matched nothing while
  `@app.route("/status")` matched — so a live handler turned into dead code on a detail of
  its URL. Cutting the arguments off first and stripping the qualifier second fixes both;
  the qualified and bracketed spellings this ordering was written for (`#[tokio::test]`,
  `@org.junit.jupiter.api.Test`) still hold, and are now pinned by a test.

- [x] B236: **route handlers, queue consumers and scheduled jobs were dead code.** Asking
  the question B235 raised — what does each framework call that the source never does? —
  of every framework the catalogues claim, rather than waiting for a repository to surface
  it. FastAPI and Flask route handlers, Celery tasks, Django signal receivers, Spring's
  `@Scheduled`, `@EventListener`, `@KafkaListener`, `@PostConstruct` and the method-level
  `@GetMapping` family all reported no detected use, as did NestJS controllers and their
  handlers, and actix-web and Rocket route attributes. Two things made it hard to see. Java
  covered `@RestController` and `@RequestMapping` but not the specific mappings modern
  code actually writes, so the coverage looked complete from the class down. And a Flask
  handler is often saved by accident: `@app.route("/health")` above `def health` spells the
  name in a string literal, which `fr unused` deliberately skips — rename the route to
  `/status` and the handler dies — and actix-web's `#[get("/health")]` above `fn health`
  was covered by exactly the same coincidence. Rules added across `python.yaml`,
  `java.yaml`, `typescript.yaml` and `rust.yaml`, which also gives `queue-consumer`,
  `websocket` and `scheduled-job` their first rules: three `EntryKind` variants that were
  declared, matched on and printed by name, but that nothing could previously emit.

  `fr translate <route> fastapi` emits `@router.get(...)` handlers, so the tool's own
  output fed back in reported its handler as having no detected use — the same shape as
  the enum-variant struct literals in B214.

- [x] B235: **a Next.js server action was dead code.** `"use server"` marks an exported
  function the framework makes reachable over the network, called by nothing in the
  source — the same case as Java's `@RestController` and pytest's fixtures, both of which
  the catalogue already covers. It was missed because every other Next.js rule matches a
  *filename*: `page.tsx`, `layout.tsx`, `route.ts`. A server action lives in a file called
  whatever you like, and in `vercel/commerce` it is `components/cart/actions.ts` — five
  live network endpoints, all reported as having no detected use. Catalogs gained
  `file_directive`, which reads the first statement of a file or of a function body, so
  both spellings are covered and a mention of the words in a comment is not.

- [x] B230: **a parse failure said how many and never where.** `fr parse` named the file
  and the number of error nodes and stopped, so the one thing a reader wants from
  "this file did not parse" — which part — was the one thing missing. The spans were
  already computed; only the printing dropped them. Every position is reported now, up to
  four per file, because a file with two hundred error nodes is one the grammar cannot
  read at all and listing them would say so two hundred times.

- [x] B229: **a Go type implemented an interface it does not implement.** The hierarchy
  pass compared a method's *name and arity*, under a comment saying a covered method set
  "is the whole of what implementing an interface means there" — which is true of Go and
  was not true of the code. `Run() string` therefore satisfied an interface asking for
  `Run() error`, and in helm/helm that produced 5,936 dispatch edges between types that
  do not implement each other, 29% of the layer. Signatures are compared where both are
  legible, and the arity answer stands where either is not, because a dropped edge here
  becomes a live method reported as dead code. Package qualifiers are dropped first:
  `kube.ResourceList` from outside a package and `ResourceList` from inside are the same
  type, and comparing them as written refused seven implementations helm plainly has —
  a fault introduced by the first version of this fix and caught by measuring dead code
  before and after rather than by trusting the edge count to have gone the right way.

- [x] B228: **the tool printed names it would not accept.** Every listing gives a
  qualified name — `Box::size`, `HookEvent::String` — and `resolve_target` matched on the
  bare name only, so the spelling the tool shows you everywhere was the one spelling that
  came back "no symbol named". It mattered at scale: in helm, `String` is twenty methods,
  and the only route through was a line and column somebody had to go and look up. A
  qualified name is tried first now, and where one is still ambiguous — two packages both
  declaring `HookEvent` — the files are named. The message for an ambiguous *bare* name
  lists the qualified names that would select each candidate, so the fix is to copy a
  line instead of going to find a line number.

- [x] B227: **three tests counted what came back and never looked at it.**
  `textual_sweep_respects_word_boundaries` found one occurrence and did not check which,
  in a test whose entire subject is telling `helper` from `helperful`.
  `cross_language_references_are_labelled_as_such` found two and did not check they were
  the HTML and the TSX, so the CSS definition mislabelled with the TSX missed would have
  satisfied it. `unreadable_files_are_skipped_and_reported` counted one skip without
  asking which file or why, and a skip that says neither is a count rather than a report.
  All three now name what they expect.

- [x] B226: **a test named for path order never checked the order.**
  `usages_group_by_file_in_path_order` asserted that more than one group came back and
  stopped there. The grouping is a `BTreeMap`, so the order held by construction — and
  swapping it for a `HashMap`, which is the obvious thing somebody does to a map that
  looks like it only needs lookup, would have broken the documented behaviour with every
  test still green. The order is asserted now.

- [x] B225: **the entry-point coverage report was checked for having names in it.**
  `coverage_gaps_are_reportable` asserted that each gap's name was a non-empty string,
  which is true of every `&'static str` in the enum: it would have passed had the
  function returned nothing, everything, or the wrong languages. What the report says is
  which languages have no rules, and it is worth printing only if it agrees with which
  languages have none — so it is compared against `has_rules_for` in both directions, and
  refuses a list that is empty or complete, since either makes the comparison vacuous.
  Verified by gutting the function and watching the test fail.

- [x] B224: **the cache's own claim was tested for stability and not for meaning.** The
  cache tells every reader that "editing a query file makes every stale entry unreachable
  rather than wrong", and the only test of that asserted the fingerprint was the same
  twice and sixteen characters long. A function that ignored the queries entirely and
  hashed a constant would have passed it — and the claim would have been false in the
  worst direction, returning confident answers computed by code that no longer exists.
  The fingerprint is now recomputed over a set with one query altered, once per language,
  and the answer has to differ each time; the language's name has to matter too, so two
  languages swapping queries is a different set. Verified by removing the name from the
  recipe and watching the test fail.

- [x] B223: **`fr duplicates` named its threshold only when it found nothing.** An empty
  answer said "No duplication of 60 tokens or more", which states what was looked for; a
  non-empty one said "3 duplicated block(s)" and stopped, which reads as all of them.
  The non-empty answer is the one somebody acts on. Both now name the threshold, and the
  footer says outright that smaller copies exist in most codebases and are not counted.

- [x] B222: **the published site was three commits behind and said nothing.** Five Pages
  deploys in a row aborted with "Timeout reached" while the deployment sat in
  `deployment_queued`; the build succeeded every time and the artifact uploaded, so
  everything a person looks at stayed green while the site served an older version. The
  types tutorial was a 404 for half a day. The deploy now waits thirty minutes instead
  of ten, and every page states the commit it was built from in its footer, so a stale
  site can be recognised by looking at it. `site_integrity` asserts the stamp is present
  on every page.

- [x] B221: **`fr type` answered half the question.** It reported what a symbol was
  declared with and stopped, so every unannotated binding came back identical whether the
  source had lost track of the value or merely not written the type down. It now derives
  a type from the six steps that follow from something stated — a literal, a class
  constructed, a declared return type, another binding, a record's field — and names the
  step and the evidence for each. A declaration still wins over a derivation, and the two
  are reported apart. A call outside the workspace and an object literal both yield
  nothing, deliberately: the chain stops where the evidence does, and answering `dict`
  for a dictionary agrees with the code rather than describing it.

- [x] B220: **the published site was checked by hand and by nothing else.**
  `cargo test --test site_data` asserts that every result shown on the site is what the
  tool produced; nothing asserted that a link goes somewhere, that an anchor names a
  heading that exists, that an `id` is unique, or that a command the prose tells a reader
  to type is a command. A full browser pass over the deployed site found no defect — and
  found that finding one would have depended on somebody looking. Now four checks over
  the `docs/` tree, offline, with the command list asked of the parser rather than
  written down beside it. The first version of the link check failed in CI and was right
  to: every page links to `playground/`, which Vite emits and which is not committed, so
  it is present locally and on the published site and absent from a clean checkout. A
  link into the frontend's build output is live by construction, and which directory that
  is comes from the build's own `outDir` rather than a name written down again.

- [x] B219: **`fr impact` reported a bounded search as a complete answer.** The caller
  walk stops at `--caller-depth`, which defaults to 3, and said nothing when it did — so
  a five-deep call chain produced "l0 affects 4 site(s)" and a list of four, with two
  functions it had never looked at. A definite count of an incomplete search, from the
  command a person uses to decide whether a change is safe. `fr flow` has said "depth
  limit reached; more may lie beyond" since it was written; the call-graph walk recorded
  cycles "rather than silently pruning them" and did not record this. The frontier is
  recorded on the same footing as a cycle now, and both `fr impact` and the call-graph
  tree say what the bound excluded. Nothing is said when the walk finished, because a
  note nobody needs is how a note somebody needs gets missed.

- [x] B218: **every hop of a forward flow was printed twice.** The use and the binding
  it initialises are the same line, and both were pushed — so `parsed = int(cleaned)`
  appeared at one indent and again at the next, all the way down, and each duplicate
  spent a level of depth on a line the reader had already seen. The chain now costs one
  level per hop, which is also twice as far before the depth limit.

- [x] B217: **forward flow stopped at the first hop in Rust.** The search for what a use
  initialises walks outward looking for a node whose value contains the reference, and
  gave up entirely the moment it found one that named nothing. Rust's `cleaned as i64`
  is a `type_cast_expression` whose `value` is the reference, and `raw.len()` is a
  `field_expression` whose `value` is the receiver — so it gave up on the `let` two
  levels further out, every time. It keeps walking now.

- [x] B216: **a value was reported as flowing into the function around it.** Having
  found the name a use assigns to, the search accepted any symbol whose `name_span`
  matched **or** whose `full_span` merely contained it, and took the first in
  declaration order. Every enclosing function's span contains everything, and functions
  are declared first — so `parsed = int(cleaned)` said the value flowed into `load`, and
  `fr flow fwd` went one hop and then wandered off into the callers, looking for all the
  world like it had traced something. An exact name is the answer where the grammar
  gives one; otherwise it is the *smallest* binding whose span holds the name, and never
  a function.

- [x] B215: **a constructor body that builds and returns was thrown away for three
  targets.** Rust, Go and Zig have no constructor, only a function that returns the
  type, and the writer cleared the body for all three under a rule about bodies that
  *assign through a receiver* — which a Rust constructor's does not. Its body already
  builds a value and returns it, which is the shape those three want. Kept now when the
  source's constructor took no receiver, and cleared with the same note as before when
  it took one. The mirror case is also handled: a body that builds and returns becomes
  field assignments for Python, Java and TypeScript, because an `__init__` that returns
  a value raises and a Java constructor that returns one does not compile.

- [x] B214: **an enum variant was read as a record.** `StopReason::Conditional { … }`
  builds a tagged union, which no target here has, and writing the path through produced
  Go that says `StopReason::Conditional{…}`. Caught by the round-trip sweep over the
  tool's own source before it shipped; a struct literal whose type is a path is refused
  as it was before B213.

- [x] B213: **a Rust struct literal was not read at all.** `Counter { value: 0, step }`
  is the one way Rust builds a record and the line every constructor is made of, and
  nothing read it — so every constructor body came out as "not translated" in all five
  targets. The IR gained a record literal with its fields still named, because four of
  these languages construct one that way and two do not, and a positional list assembled
  at read time would be in the source's declaration order — a fact about the source
  rather than about any constructor a caller will call. Written exactly by Rust, Python,
  Go and Zig; turned into field assignments inside a constructor for Java and
  TypeScript, and reported elsewhere for those two.

- [x] B212: **a Zig method that changed its own object did not compile.** Four of these
  languages hand a method a reference and let it assign through it. Zig hands it a
  value, and a value parameter there is const — so `pub fn bump(self: Counter)` with
  `self.value = …` in the body is not a slow method, it is a file the compiler rejects,
  from a source that said `&mut self` and a report that said every signature carried
  across with its types intact. The receiver is a pointer when the body assigns through
  it and a value when it does not, recognised by whatever the source called it: `self`
  in Rust and Python, `this` in TypeScript. Go already took a pointer receiver for every
  method, which is safe, and is untouched.

- [x] B211: **the Java output named types the file had never imported.** `List`, `Map`
  and `Optional` are the three names this writer reaches for that Java does not have in
  scope, and it emitted all three and imported none — so a signature the report called
  "carried across with its types intact", which it was, named a type the file had never
  heard of. The published translation example on the site was one of them. Read from the
  IR rather than the finished text, because an import has to be written before the class
  that uses it and because a `List` inside a string literal is not a use. This is the
  same defect as B209, one language over: the fix for that one was found by asking
  whether the Zig output bound the `std` it reached for, and nobody asked Java the same
  question.

- [x] B210: **joining two strings produced Zig that does not compile.** Java, Go, Python
  and TypeScript all concatenate with `+`, and so did the source; Zig has no `+` for
  slices, because joining them means allocating and the allocator is a parameter the
  function does not have. Inventing one would change the signature every caller was
  written against, so the operation is refused rather than guessed at — as an
  `@compileError` naming the reason, which is a value anywhere one is expected and
  cannot be mistaken for code that works, where an empty slice quietly could. Zig has no
  block comment, so a marker beside the value would have swallowed the rest of the line.

- [x] B209: **the Zig output named a standard library it had never bound.** Comparing
  two strings there is `std.mem.eql`, and nothing in the writer emitted
  `const std = @import("std");` — so the fix for B208 produced a file that referred to
  `std` out of nowhere. The binding is written when the module reaches for it and not
  otherwise, decided from the IR rather than from the finished text, because it has to
  be written before the code that uses it.

- [x] B208: **comparing two strings meant something else in Java, and nothing at all in
  Zig.** Every writer rendered `==` as `==`. Rust, Go, Python and TypeScript compare a
  string's contents that way, and so did the source. Java compares *references*, so the
  translation was quietly false for two equal strings that were built rather than
  interned; Zig will not compile `==` on a `[]const u8` at all, so the output looked
  like the other five and did not build. Written as `java.util.Objects.equals(a, b)` —
  which answers for null on either side, where `a.equals(b)` throws — and
  `std.mem.eql(u8, a, b)`. Integers keep `==` everywhere, and the four languages that
  already agreed are untouched.

- [x] B207: **`a %% b` on two integers meant something else in Python, silently.** The
  same disagreement as division, found by asking the same question of the next operator
  in the table: every other language here takes the remainder's sign from the dividend
  and Python takes it from the divisor, so `-7 %% 2` is -1 on one side and 1 on the
  other. Unlike division there is no readable Python form — writing it exactly means
  `a - b * int(a / b)` — so the idiomatic operator is kept and the difference is
  reported, with the expression quoted and the exact condition named. Float remainders
  agree and are not reported, because a note nobody needs is how a note somebody needs
  gets missed.

- [x] B206: **`a / b` on two integers became float division in Python.** Rust, Go, Java
  and Zig all truncate; Python's `/` produces a float and its `//` floors, so neither
  operator means what the source meant. `half(7, 2)` returned 3 on one side and 3.5 on
  the other, with the report calling the signature complete and the annotation still
  saying `-> int`. Written as `int(a / b)` now, which truncates toward zero and is
  exactly what the source said. Nothing is inferred: a binding whose type the source
  never wrote down is not known to be an integer and its division is left as it was,
  because guessing would be the same mistake pointing the other way.

- [x] B205: **a Rust function's tail expression translated to nothing.**
  `pub fn f(a: i64, b: i64) -> i64 { a + b }` is the ordinary way to write one, and the
  tail is its result. Reading it as a plain statement dropped the return in every target
  at once: Python got a function returning `None`, Zig one saying `_ = a + b;`, and Go,
  Java and TypeScript ones that do not compile — each still declaring the return type
  the signature carried across faithfully. Only the body's own tail is read as a return;
  a tail inside an `if` needs the whole of Rust's block-expression rule and is left as
  it was rather than half-done.

- [x] B204: **every translation dropped the brackets.** All six writers rendered a
  binary expression as `left op right` and nothing else, so `(a + b) * c` came out as
  `a + b * c`, `a - (b - c)` as `a - b - c`, and `!(a && b)` as `!a && b` — in Python,
  TypeScript, Go, Java and Zig, in both directions, for the most ordinary expression
  there is. Brackets are decided from precedence now rather than copied from the source,
  so the result is right where two languages disagree about binding and a group that was
  never needed does not survive the trip either.

- [x] B203: **the grouping nearly went into CSS.** The node kinds that bind their
  operands are recognised by substring, because six grammars name the same thing six
  ways — and CSS names a `descendant_selector` and an `attribute_selector`, which read
  as operator kinds and are nothing of the sort. Bracketing a selector is not a
  grouping, it is a syntax error. Caught by the reparse guard before it shipped;
  grouping is now asked of the same predicate `inline` asks, so there is one answer to
  "does this language group with brackets" rather than two.

- [x] B202: **`fr restructure` changed what the code computes.** A captured expression
  is substituted as text, so `double($X)` → `$X * 2` turned `double(x + 1)` into
  `x + 1 * 2`, which is `x + 2`. And the replacement itself is dropped where the match
  was, so `2 * double(y)` with a template of `$X / 2` gave `2 * y / 2`, which for
  integers is not `2 * (y / 2)`. Both halves are the defect already fixed twice in
  `inline`; this is the third place an expression is moved into a context it was not
  written for, and the one whose whole purpose is moving expressions. A capture the
  template will bind is bracketed, and a replacement that binds is bracketed where it
  lands — neither when the text is a single thing already.

- [x] B201: **a move left behind what the moved code needed, in every language with its
  own move path.** The generic path — TypeScript and Python — writes an import pointing
  back at a name the source file *defines* and the moved code still uses. Rust, Zig and
  Go have their own paths, and those looked only at what the source file *imported*, never
  at what it declared. So a Rust function using a `const` beside it landed in a file
  where that name means nothing: `cargo check` answered `cannot find value PI in this
  scope` and suggested the exact `use` the tool should have written, and `fr move`
  reported no warning at all. Rust now writes it, and makes the item `pub` where it was
  not, since a private item is invisible from another module. Zig imports a module and
  qualifies rather than binding a name, so there is no import to write and the reference
  itself would have to change; that is reported rather than guessed at. A Go move inside
  one package is one scope and needs nothing.

- [x] B200: **deleting a lone Java field deleted the class around it.** The widening
  climbs while the symbol is the only child of its kind in its parent, on the grounds
  that a parent left with none of them has nothing left to be. That is true of a CSS rule
  set whose last selector goes, which is what it was written for. It is false of a Java
  class body, whose other members are methods — a *different* kind, so the test passed
  and the climb went field → class_body → class_declaration. One unused constant took the
  class and every method in it. The climb now stops below anything its own parent names
  as its body, which is the general form of "this container is not optional".

- [x] B199: **`fr remove-flag` never worked on a TypeScript file.** It deleted the flag's
  own span, which there is the declarator inside the declaration: taking
  `NEW_UI = true` out of `const NEW_UI = true;` leaves `const ;`. The edit guard caught
  it every time — the file was never damaged, the command simply always failed, in a
  language where four others succeeded. It removes the definition through the same two
  steps `fr delete` takes now, because it is the same question. A side effect visible in
  the published examples: the flag no longer leaves a blank line where it was.

- [x] B198: **extracting a region that awaited produced code that does not compile.**
  The body kept its `await` in a function that is not async — `tsc` says `TS1308`,
  CPython says `SyntaxError: 'await' outside async function` — and the call site handed
  back a promise where the code after it expected a number. The extracted function is
  marked async now and the call awaits it. Rust writes `.await` as a postfix, so there
  is no keyword to move; that is refused rather than half-done.

- [x] B197: **extracting a region containing `yield` silently did nothing.** In Python
  the call constructed a generator and never ran it, so the loop body had no effect at
  all and the accumulator it also updated stayed at zero; in TypeScript the result is a
  `yield` outside a generator, which `tsc` rejects. `return`, `break` and `continue`
  were refused from the day this was written, on the grounds that a call cannot
  reproduce a jump the enclosing function can see. A `yield` has exactly that property
  and was not among them.

- [x] B196: **an expansion of two bracketed halves was left unbracketed.** The check for
  "this is already inside brackets" read the first and last character, and
  `(p + 1) / (q - 1)` starts with one and ends with one. So `2 * scale(p + 1, q - 1)`
  expanded to `2 * (p + 1) / (q - 1)`, which for `p = 1, q = 4` is 1 where the call
  returned 0. The check balances the brackets now.

- [x] B195: **`fr inline --call` changed what the program computes, in all seven
  languages.** The body binds its parameters at whatever precedence it was written with
  and the argument arrives as text, so `n * 2` with `x + 1` for `n` produced
  `x + 1 * 2` — `double(x + 1)` returning `x + 2`. Inlining a *variable* was fixed for
  exactly this in an earlier pass; inlining a *call* substitutes in the other direction
  and never was. The grouping test was reached through
  `extract::supports_imperative_extract`, which is a different question with a mostly
  overlapping answer, and the overlap is where the wrong ones live: Java groups with
  parentheses like every other C-shaped language here and is absent from that list
  because it has no inferred declaration to extract into. It now asks whether the
  language groups with parentheses, which is what it wanted to know.

- [x] B194: **a pytest fixture and a `unittest` fixture matched no rule.** Nothing calls
  either by name — pytest injects a fixture by matching the parameter, and `unittest`
  calls `setUp` itself — which is the same reasoning the Java catalog already gives for
  `@Bean` and `@Component`. A fixture in `conftest.py`, where the shared ones live,
  matched nothing at all: neither the file nor the function is named `test_*`. The ones
  in a `test_*.py` file were found only by the file rule, so they looked covered.

- [x] B193: **a Python script with a `__main__` guard reported no entry point.** Every
  other catalog says `name: main`, because every other language here agrees that a
  program starts in a function so called. Python's starts in a *statement*, and what it
  calls can be named anything — so the rule that works everywhere else answered nothing
  for an ordinary script, from the command whose only job is that question. The report
  named css, scss and yaml as the languages without rules, which said Python was
  covered. Catalogs gained `called_from_main_guard`, the first predicate here that is
  not a property of a name.

- [x] B192: **a type argument was read as a supertype.** The heritage reader took every
  type name under an `extends`/`implements` clause, so `implements Holder<Pet>` filed the
  class under `Pet`. A call that reached it by method name alone was then reported as
  reaching it through a relationship somebody had declared — the same edge, presented on
  evidence that did not exist. Found while adding Java, and the helper's doc comment had
  claimed the opposite of what it did since it was written.

- [x] B191: **class hierarchy analysis skipped Java.** `Family::of` mapped Rust, Go,
  TypeScript and Python, and everything else fell into a `_ => None` documented as being
  about Zig's comptime duck typing and Bash's lack of methods. Java has neither excuse:
  it states its hierarchy in as many words, and it was the one language here whose
  hierarchy went unread. A call through an interface reached no implementation, so
  `fr callers` on an overriding method answered with nothing at all while the same code
  in TypeScript answered correctly. Reading three vendored gson files now yields 80
  hierarchy edges where it yielded none.

- [x] B190: **the number describing the matrix was not checked, and had drifted.** The
  table is regenerated and asserted against the code; the sentence above it counting the
  cells was prose, and said 260 supported where the rows it introduced counted 261.
  PLAN.md was staler still, quoting a total from before six capabilities and a language
  existed. Now asserted in all four places it is published, against the same computation
  that produces the table.

- [x] B189: **a shell function was told it needs a return type and modifiers.** The
  fallback reason for an absent capability is chosen by language *class*, so every
  imperative language gets the same sentence — which was written for Java. The same
  defect was fixed once before, when the reasons had been written for markup and told
  Java it was a stylesheet; a second imperative language landing on the same arm brought
  it back. The reason-check test now carries a word-to-language table rather than one
  list of markup words, so a sentence describing a different language fails the build.

- [x] B188: **the capability row for `fr stitch` was a transcription of the accessor
  table, and it had drifted from it.** `support()` documents that every arm asks the
  refactoring's own predicate; this one listed the languages by hand. It now asks
  `stitch::reads_environment`, which is the list.

- [x] B187: **`fr stitch` could not see a Java or Zig program read its configuration.**
  The accessor table was written for the first five languages and never revisited. A
  Helm chart feeding a Java service therefore reported every variable as configuration
  with no consumer — the exact finding the command exists to produce, produced
  backwards. Zig needed more than a new prefix: `getEnvVarOwned` takes the allocator
  first, and the reader assumed the name was the first argument, so it read `allocator`,
  which the upper-case filter then dropped without a word. Accessors now carry how many
  arguments stand before the name.

- [x] B186: **a reference in an argument position could be mistaken for the call.** The
  hunt for a call walks up to eight parents from the reference, and once it stopped
  filtering on the recorded kind (B185) a type named inside somebody *else's* argument
  list — `register(Pet.class, 7)` — found that enclosing call and would have reordered
  its arguments as though they belonged to `Pet`. Found while fixing B185, before it
  could ship. The walk now requires the reference to sit before the argument list of the
  call it lands on, which is what "this reference names the thing being called" means.

- [x] B185: **a constructor's parameters were reordered and every `new` left as it
  was.** `new Thing(1, "x")` is written down as a reference to the *type* — which it
  also is — and the call-site loop skipped everything whose recorded kind was not
  `Call`. So the declaration changed, thirteen construction sites did not, and nothing
  warned: the same silently-partial result already fixed for `rename`. The grammar now
  decides whether a reference is a call, not the kind the extractor wrote down. A
  mention that really is not a call — the `C` in `static C make()` — has no arguments to
  change and is passed over; a recorded call the grammar will not show as one is still
  refused.

- [x] B184: **`fr signature` refused at every Java call site there has ever been.** The
  lookup matched on `kind().contains("call")` plus one named exception, under a comment
  claiming SCSS's `include_statement` was "the one call form whose kind does not say
  call" — true of the languages it was written against. Java spells a call a
  `method_invocation` and a construction an `object_creation_expression`, so both were
  invisible and the refusal came out as a sentence about resolution strength for a
  reference that had resolved exactly. The message for a call the grammar genuinely
  will not expose is now a `Refusal::Unknowable`, which says what is actually wrong
  instead of blaming a confidence that was never the problem.

- [x] B183: **the imports a moved symbol needs were written above the code rather than
  where imports go.** Prepending them to the moved text put an `import` statement in the
  middle of the destination — legal in Python, a syntax error in half the other targets,
  and wrong-looking in all of them.

- [x] B182: **moving a symbol into a file that imported it left the import behind.**
  `from .b import area` in the file `area` is moving *into* points at a file that no
  longer defines the name, so the destination fails on the line that used to make it
  work. Nothing was adding that import, so nothing was removing it either. An import
  naming several things is narrowed rather than deleted: the rest are still over there.

- [x] B181: **an `if` that binds what it tested could be inverted.** Zig writes
  `if (maybe) |value| { … }`: the condition is an optional and the payload binds what
  was inside it. Inverting gave `if (!maybe) |value|`, which is not a program — the
  binding has nothing to bind and `!optional` is not a boolean there. The reader had
  refused the same shape for the same reason since it was written; the rewrites had not.

- [x] B180: **Zig fell into the C arm of the boolean spelling table.** It writes `and`
  and `or` as words, as Python does, and negates with a sigil, as C does — so it matched
  neither and every rule that looks for a boolean operator was blind to it. `!(a and b)`
  is also an `error_union_type` in that grammar, because `!T` is an error union where a
  type is expected and a negation where a value is, so De Morgan could not find the
  negation either.

- [x] B179: **`invert-if` negated half a condition and swapped the branches anyway.**
  `if a == 1 and b == 2` became `if a != 1 and b == 2` — a different program that
  compiles, parses and answers differently. The guard excluded `&&` and `||` and knew
  nothing of the languages that spell them as words, and the comparison it flipped was
  the first one found in the text rather than the one the condition makes: `g(a == 1) ==
  2` flipped the inner one. The negation is only simplified when the comparison is the
  whole of the condition and sits at the top level; otherwise it goes round the outside,
  which is what De Morgan is there to distribute afterwards.

- [x] B178: **a shorthand object property refused the whole object.** `{ species }`
  means `{ species: species }` and is how every modern TypeScript file is written;
  reading it as something unrecognised refused the object, and refusing the object
  refused the statement the object was in. In the pet store that cost `GET /pets` its
  entire body.

- [x] B177: **`??` had no counterpart in the IR**, so every nullish coalescing in every
  TypeScript file was carried verbatim — and in the pet store it took a whole `const`
  statement with it, which is how a query parameter went missing from both sides of a
  crossing that otherwise agreed perfectly. It asks whether a value is absent, which is
  a question rather than an arithmetic operator: Zig spells it `orelse`, Rust reaches
  for `Option::unwrap_or`, Java for a static method, Python has to name the value twice,
  and Go cannot say it at all. The two that must name the value twice refuse when naming
  it twice would call it twice.

- [x] B176: **a query parameter did not survive the crossing.** Next.js has no
  declaration for one — a handler reads it out of the URL — so the translated router
  said it took no query, and a caller sending `?species=cat` was outside a contract that
  claimed to describe it. The parameter is declared now, `str | None = None`, which is
  exactly what `searchParams.get()` returns; every read of it in the body becomes that
  name; and the binding that was doing the reading is dropped, because a binding of a
  name to itself is a statement that does nothing. The whole contract now survives, and
  the build asserts it.

- [x] B175: **the contract could only be derived from one side of the crossing.** The
  method says to diff the baseline against the finished service, and half of that check
  needs no server: a FastAPI router declares its contract on the decorators and in the
  signatures, which is where FastAPI itself reads it. Run over the pet store, thirteen
  operations go in and thirteen come out with every URL, method and path parameter
  identical — and the one thing that does not survive, a query parameter the translated
  handler still reads off the request object, shows up as a line of diff instead of as
  nothing at all.

- [x] B174: **a handler's inline `context.params.petId` was left naming an object
  Python does not have.** Dropping the *statement* `const id = context.params.id` was
  only half of it: a handler that reads the path parameter inline — inside a `where`
  clause, inside a call — kept the Next.js spelling, so the translated endpoint answered
  every request with a `NameError`. The path parameter arrives by a different route in
  FastAPI and every use of it now arrives with it.

- [x] B173: **the contract had no query parameters at all.** Next.js declares none —
  a handler reads them out of the URL — so a document built from declarations said the
  endpoints take no query, and a caller passing `?species=cat` was outside a contract
  that claimed to describe it. Read from `searchParams.get("…")` now; and where a
  statement could not be read at all, the document says a query parameter inside it may
  be missing, because a gap nothing mentions is the failure this is all about.

- [x] B172: **the contract listed schemas that nothing referred to.** A `components`
  section with no `requestBody` pointing at it says every endpoint takes no body. The
  link comes from the `petCreateSchema.parse(json)` call inside the handler, which is
  the only place a Next.js route records it.

- [x] B171: **a zod schema declared in another module was invisible.** A real Next.js
  application keeps its shapes in something like `lib/schemas.ts` and imports them, and
  only the route file was read — so the contract came out with an empty `components`
  section. Every `.ts` file in the tree is read now.

- [x] B170: **removing a parameter the body still reads.** `def f(a, b): return a + b`
  with `remove:1` produced `def f(a): return a + b`, which names something nothing
  supplies. The rule existed for shell functions — "the body still reads `$2`" — and for
  nothing else. Two SCSS tests were asserting the broken output.

- [x] B169: **extracting an expression that *is* its statement left a statement that
  only names the binding.** `zzx;` is a parse error in Zig, an unused value in Go, and
  nothing at all in the other three. The value is already being computed for its effect,
  so there is nothing to hoist; the extraction is refused with that reason.

- [x] B168: **`inline` refused every Zig binding there has ever been**, while the
  capability matrix said it worked. tree-sitter-zig names nothing on a
  `variable_declaration` — the `=` is an anonymous token with the value after it — and
  the lookup asked only for a `value` or `right` field.

- [x] B167: **`inline` changed what the code does.** `b = a + 1; return b * 2` became
  `return a + 1 * 2`, which is `a + 2`. Every language with an expression grammar, since
  the operation was written. A refactoring that changes the answer is the one thing this
  tool must never do. The substituted value is parenthesised now unless nothing
  surrounding it could split it — a name, a literal, a call, a field, an index. That errs
  toward a redundant pair, which is the price of not keeping a precedence table per
  grammar; such a table is wrong somewhere, silently, in exactly this way.

- [x] B166: **an overload set was resolved by proximity, at `Exact`.** Two methods
  declared in one class body are equally plausible targets for a bare call, and the
  nearer one is a coin flip — so Java's `add(int)` beside `add(String)` sent both
  `add(...)` calls to whichever was written second, with the highest confidence there
  is. Renaming that one rewrote calls belonging to the other. Proximity is evidence for
  a binding, where it reads as shadowing, and not for a callable.

- [x] B165: **a rename that left a call behind said nothing at all.** Same-named
  references were reported only when they resolved to *nothing*; one that resolved
  weakly to some other symbol was skipped in silence, because the winner was not the
  symbol being renamed. The rename went through, the calls stayed, and the warning list
  was empty. A weak resolution is a guess wherever it lands.

- [x] B164: **a member access was resolved through the lexical scope chain.** `c.run(1)`
  names a member of whatever `c` is, and the scope chain has nothing to say about that —
  but it answered anyway, at `Exact`, by picking whichever same-named method sat in an
  enclosing scope. With two classes declaring `run` in one file, a call on one was
  attributed to the other. The chain still answers where it can settle it: a receiver
  that *is* the enclosing instance, or a name only one member in the workspace has.

- [x] B163: **the rename collision guard was file-scoped, not scope-scoped.** A parameter
  is written outside the body it belongs to, so the scope it falls in is the one *around*
  its function — the file. Every parameter of every function therefore shared a scope,
  and renaming one to a name used by an unrelated function was refused as a collision.
  Measured over the vendored corpora, that was most of the renames a real file offers.

- [x] B162: **a Go type's recursion was not its entry point.** The value of a
  `map[string][]SymbolId` resolved one layer and lost the slice: the outer map was read
  by one rule and the inner type by a helper that only knew scalars. The same lesson was
  already written down for TypeScript, in a comment, one reader away.

- [x] B161: **a Rust reference was stripped after the containers were checked**, so
  `&HashMap<K, V>`, `&Vec<T>` and `&Option<T>` — which in Rust is most of them — were
  read as names rather than as what they are. `&'a str` kept its lifetime and became a
  type this tool could not write; `Node<'_>` read the lifetime as a type argument and
  produced a type with an empty name; and generic arguments were split on the first
  comma, so `HashMap<String, Vec<A, B>>` split in the wrong place.

- [x] B160: **`readonly string[]` was read as an array of `readonly string`.** No other
  language here has anywhere to put "you may not write to it".

- [x] B159: **every Zig type read from text was read wrong.** The grammar binds `?`
  tighter than `.`, so `?http.Request` arrives as a field expression whose left side is
  a nullable `http` — inside out. A generic type is a name *applied* to its arguments,
  and reading `std.StringHashMap([]const u8)` as one name turned a dictionary into a
  type with that name. Zig's own scalar names were recognised only on a `builtin_type`
  node, so an `i64` read from text stayed an `i64`. And a type's text can span lines and
  hold doc comments, which went into the name: two hundred characters of prose where a
  type should be.

- [x] B158: **the Zig reader required a named node after `=`**, and `undefined` is an
  anonymous token — so every constant this tool's own Zig writer emits for something it
  could not translate was lost on the way home.

- [x] B157: **the Python reader would not read back what the Python writer writes.** It
  required SCREAMING_SNAKE for a module constant; the writer spells a constant bound to
  anything but a literal in lower case, on the grounds that shouting the name of
  `schema = z.object(...)` would be wrong. Two rules deciding one thing, disagreeing.

- [x] B156: **the round trip checked functions and not data.** A field that vanishes is
  exactly as bad as a parameter that vanishes, and nothing was looking. Fields,
  constants and now parameter and return *types* are compared — as shapes, since
  TypeScript has one numeric type and Go writes nothing at all for a function that
  returns nothing.

- [x] B150: **the methods of every generic Rust type became free functions.** An
  `impl<'a> Ctx<'a>` was read as an impl on `Ctx<'a>`, which matches no record in the
  file, so its methods never joined their type — and each one gained a `self` parameter
  bolted on by the rule that binds an orphaned receiver.

- [x] B149: **a constructor had no counterpart in the IR at all.** Three of these six
  languages have one and three have a habit — `Thing::new`, `NewThing`, `Thing.init` —
  and none of the six could carry it, so a Java constructor was a class member nothing
  recognised. What carries is now that it *is* one; the name is the target's business.
  The habit is read as a constructor only when the function also **returns the type**,
  since a `new` that returns something else is an ordinary function with a common name.

- [x] B148: **a constructor's own name claimed a spelling in the naming map.** Java
  names it after the class, so every Java class translated came out named after its
  constructor: `class a` where the source said `class A`.

- [x] B147: **a Rust raw identifier grew an `r` every time it crossed.** `r#where` *is*
  the identifier `where` — the prefix is how Rust spells a name that collides with a
  keyword, and every writer here puts it on when it needs to. Leaving it on the way back
  in meant a round trip produced `r#r#where`.

- [x] B146: **Python's `self` was stripped from free functions too.** It is a convention
  inside a class and an ordinary name outside one, so stripping it everywhere lost a
  parameter from every module-level `def f(self, …)` — which is what every method of a
  Zig file-struct becomes after a round trip through Python.

- [x] B145: **a `@staticmethod` disappeared from its class.** The Python reader handled
  decorated definitions at module level and not inside a class body, so a decorated
  method fell to the member loop's catch-all. It was dropped and the report still said
  every signature had carried across intact — including for the `@staticmethod` this
  tool's own Python writer emits.

- [x] B144: **every reader's record member loop ended with `_ => {}`.** A member that is
  not understood is not a member that is not there; a record has no room for a construct
  it cannot translate, so it is carried beside the type and counted. Java constructors
  were the largest thing this had been swallowing.

- [x] B143: **there was no round-trip check at all.** "The output parses" cannot see a
  parameter that vanished or a function that did not come back — the file is still
  perfectly good in the target's grammar. `tests/round_trip.rs` translates every real
  source in the repository into every target, reads the result back, and compares which
  functions exist and what their parameters are called. It found four of the five
  defects above on its first run.

- [x] B142: **a note was reported only when something else had gone wrong.** Not every
  note is about a carried construct — a type the source never wrote down, a name the
  target reserves, a base class a language without inheritance cannot keep — and the
  report printed them only when `carried_verbatim > 0`. A translation that lost a
  supertype and nothing else gave a clean bill.

- [x] B141: **a base class was dropped without a word.** Three of these six languages
  have inheritance and three do not, and the IR had no room for one at all, so
  `class JsonPrimitive extends JsonElement` became a class that extends nothing. It is
  a different type. Carried into Java, Python and TypeScript now, and reported for the
  three that cannot say it.

- [x] B140: **there was no conditional expression in the IR**, so `a > 0 ? 1 : 2` was
  carried verbatim by all six writers — including the five languages that have one.
  Every ordered pair among Python, TypeScript, Rust, Java and Zig translates it now; Go
  is the only target without one, and turning an expression into an `if` statement
  needs somewhere to put the result, which does not exist inside an argument list.

- [x] B139: **`_` was put through the naming convention.** It is the word for "no name"
  in four of these languages, and asking what the empty word is called in `camelCase`
  returned the empty string — so `_ = x;` in a Zig file came out as ` = x;`. A rename
  that produces nothing is not a rename.

- [x] B138: **a Zig `comptime` parameter was read as an ordinary one.** `comptime T:
  type` is how Zig writes generics: the parameter is a *type*, supplied where another
  language writes `<T>`. Read as a value it produced
  `func Lazy(comptime type, comptime type) type`, a signature that means something
  else in every target. Refused and carried.

- [x] B137: **a Zig destructuring kept the first name and dropped the rest.**
  `const a, const b = pair;` binds two names and the IR binds one, so `b` vanished
  without a word.

- [x] B136: **Zig optionals and pointers were never read.** The grammar calls `?T` a
  `nullable_type`, so the arm written for `optional_type` matched nothing and every
  optional crossed as a foreign type spelled `?T`; a pointer had no arm at all, so
  `*Analyser` became `Unwritable_Analyser`. A pointer is how Zig writes a reference,
  and the languages without pointers still have the thing being pointed at.

- [x] B135: **a Rust raw or byte string was not read as a string.** `r"\d+"` fell to
  the catch-all, so every regex in a file lost its value *and* its constant-ness: a
  `const` bound to something the reader did not understand stops looking like a
  constant, and its name loses the convention that goes with one.

- [x] B134: **a parse error with no position reported none at all.** The self-check
  walks for the innermost error node; an empty Zig struct holds a zero-width missing
  identifier that `Node::children` does not yield, so the message came out with no
  line and no column. `error_spans` walks with a cursor and does find it, so it is the
  fallback rather than a second opinion — two walks were deciding one thing and
  disagreeing.

- [x] B132: **a comment inside a parameter list was read as a parameter.** A comment is
  an *extra* in every one of these grammars, so it can appear between any two nodes
  anywhere; every reader reads a parameter list either positionally or through a
  catch-all arm, and both read a comment as whatever they expected in that position. A
  four-line comment between two parameters of `generic()` became four parameters named
  after the sentence, in every target. Fixed at the choke point: `Cx::children` returns
  the children that are part of the structure, and the one place that wants comments —
  a statement block — asks for them by name.

- [x] B131: **every string escape was doubled on every crossing.** The IR held the
  source's *spelling* rather than the string's value, so a writer escaped the backslash
  again on the way out and `"line\nline"` crossed as `"line\\nline"` — a literal
  backslash and an `n` where there had been a newline. The output parsed, so nothing
  caught it. The reader decodes escapes now and each writer puts its own back on;
  `{:?}` was doing that job for all six, and it is Rust's spelling — it emits
  `\u{...}`, which is a syntax error in Python, Java, TypeScript and Go.

- [x] B130: **a method was written as a free function whose body reached through a
  receiver nothing bound.** Rust and Go declare methods apart from their type; the IR
  keeps them with the type, which is what lets one shape become the other — and the
  Rust reader said exactly that in a comment while pushing them out as top-level
  functions. Every writer then produced `def label(prefix)` whose body says `self.name`.
  A method whose type is not in the file gets its receiver as an ordinary first
  parameter, which is what Go and Zig write anyway.

- [x] B129: **a method with no receiver was written as one with a receiver.** Rust's
  `impl` holds `fn new() -> Self` beside `fn len(&self)`; one `bool` was answering both
  "is it inside the type" and "does it take a receiver". Python lost `@staticmethod`
  and TypeScript wrote `export function` inside a class body.

- [x] B128: **a multi-line comment got its marker on the first line only.** A
  `/* ... */` is one node however many lines it spans, so the rest of a JSDoc paragraph
  arrived in the output as code, asterisks and all. Fixed in three places that are each
  the only place for it: the reader strips the ` * ` leader from every line, a doc
  comment is one entry per line, and `Out::line` indents each line it is given.

- [x] B127: **a doc comment could end itself early.** `*/` closes a block comment, and a
  doc comment quoting `app/**/route.ts` carries that sequence mid-sentence. Java and
  TypeScript wrote it through, so the comment ended and the rest of the sentence was
  parsed as code — three words, two template strings and an optional chain, none of
  which the author wrote.

- [x] B126: **`0usize` was carried into every target.** A Rust literal writes its width
  into itself, which no other language here reads; the IR carries the type separately,
  so the digits are what crosses. `r"\d+"` and `b"bytes"` were not read as strings at
  all, which cost every regex in a file its value *and* its constant-ness.

- [x] B125: **a Rust tuple struct silently lost its payload.** A record in the IR is a
  named product and a tuple struct's field has no name, so reading one gave a record
  with no fields and `Vec<SymbolId>` vanished without a word. Refused and carried
  instead; `self.0` is refused for the same reason, since no target here has a field
  with a number for a name.

- [x] B124: **`let _ = f();` declared something with no name.** It binds nothing — it is
  a call whose result is deliberately dropped, which every target can say — and reading
  it as a binding wrote `const  = f();`.

- [x] B123: **a TypeScript class member is public unless it says otherwise, and every
  one of them was read as private.** A free function and a class member have opposite
  defaults, and reading both the same way made every translated method `private` in
  Java and unreachable in Go, Rust and Zig — while making every `private` field public,
  which is the same mistake pointing the other way.

- [x] B122: **Python's `x = 1` is a declaration the first time and an assignment every
  time after, and all of them were read as declarations.** `total = total + x` inside a
  loop became `let total = total + x;` in Rust — which *shadows* rather than
  accumulates, so the value outside the loop never changed. It parses, it type-checks,
  and it is the wrong program. Python's scope is the function rather than the block, so
  one set of bound names carried through the body in order is exactly its rule.

- [x] B121: **the receiver had six names and the IR recorded none of them.** `self`,
  `this`, or whatever the Go author called it — and because the receiver is the one
  binding that is *not* in the parameter list, it never went through the rename every
  other name goes through. Every translated method kept its source's word and referred
  to a name the output never binds. The IR records the word the source used and each
  writer puts its own back on.

- [x] B120: **`self` is the one keyword Rust refuses to raw-escape.** The escape that
  makes every other reserved word writable produced `r#self`, which is a compile error,
  so a method body that was correct became a file that does not build. The same is true
  of `crate`, `super` and `Self`, which take a suffix instead.

- [x] B119: **Go's `error` is Zig's keyword for an error set**, so a signature carrying
  the type across by name did not parse. Written `@"error"` — Zig's own spelling for an
  identifier that collides with one of its words — under which the name still says what
  the source said.

- [x] B118: **the Zig reader read named children only, and in that grammar the `:`
  before a type, the `=` before a value and every operator are anonymous.** Every field
  and parameter lost its type, `var sum = 0` declared a variable called `var`, `a * b`
  put the right operand where the operator should have been, and every `else` branch
  was silently dropped. Found by running the translation rather than by reading it.

- [x] B117: **a `for` over two sequences, and an `if`/`while` that unwraps an optional,
  were read as if they were the one-binding form.** `for (xs, ys) |x, y|` and
  `if (maybe) |value|` bind things the IR has no room for, and reading them as the
  simple form dropped half of what the loop said. Refused instead.

- [x] B116: **Zig rejects a `var` nothing writes to.** Only the Rust reader records
  mutability at all; every other one says "mutable" because it has nothing better to
  say. Taking that at its word turned a `const` file into one that will not build.
  Which keyword a binding takes is worked out from what assigns to it.

- [x] B115: **Zig has no block comment**, so a carried-over fragment written beside an
  expression swallowed the rest of the statement, semicolon included. Carried text is
  queued and flushed as whole-line comments above the statement instead.

- [x] B114: **the ordered-pair translation test covered sixteen of twenty pairs and
  asserted twelve.** Four source files for five languages, with the expected count
  written as a literal, so adding a language quietly shrank the fraction of the matrix
  under test. The count comes from `SUPPORTED` now and a missing source fails there.

- [x] B113: **`generic()`'s path separator reads as an argument separator.** It is the
  `::` in Rust's `std::fmt` and the `.` in everyone else's, and it sits in the signature
  directly after `args`. The Java writer was written on the natural misreading and
  turned `sync.Mutex` into `sync, Mutex`. Renamed to `path_separator`.

- [x] B112: **Java was missing from the transpiler's reserved-word table**, so a Python
  `defaultdict(float)` wrote the keyword `float` into an expression position and the
  output would not parse.

- [x] B111: **a Java catch clause lost both its exception type and its binding.**
  `catch (IllegalStateException error)` holds a `catch_type` and an identifier as plain
  children rather than as named fields, so asking for fields returned nothing and the
  body referred to a name the `except` never bound.

- [x] B110: **`d[k] = v` translated into Java as `d.get(k) = v`,** which is not a
  statement in the language. Java has no assignable subscript on a collection; it is
  `d.put(k, v)`.

- [x] B109: **the entry-points reason called YAML a stylesheet.** Found by the test
  written for the last round of this — a reason naming a language other than its own is
  the tell, and it is now asserted for every capability × language pair rather than
  spotted by eye.

- [x] B108: **`fr remove-flag` refused every Java flag, and then refused to fold it.**
  Two causes, one after the other. `SymbolKind::Field` is a struct member in Go and Rust
  and never a flag — but Java has no top level below the type, so `public static final
  boolean NEW_CHECKOUT` *is* the idiomatic flag and there is nowhere else to put it. And
  once the constant was substituted, `if (true)` did not collapse: Java names an `if`'s
  condition as the *parenthesised* expression, so the literal arrived as `(true)`.

- [x] B107: **`fr imports` told a reader that Bash "has no import statements to
  organize"** while `queries/bash/facts.scm` extracts every `source`. The true answer is
  structural and stronger: `source` *runs* the other file rather than declaring a
  dependency on it, so order carries meaning and a file sourced only for a side effect
  looks unused. The operation and the capability table each kept their own copy of the
  reason and had drifted; `why_not_organizable` is now the single authority.

- [x] B106: **`fr translate` and `fr openapi` were missing from the capability matrix.**
  The matrix is the tool's own claim about what it does per language, and translation's
  answer differs by language in two ways — a containment rewrite is the same bytes under
  another grammar, a translation between programming languages is a draft. A test now
  fails if a command with a per-language answer is left out.

- [x] B105: **`fr translate` denied a capability the tool has.** Refusing a Java file it
  said *"it needs a semantic model of both, and this tool has neither… Nothing here can
  do it, so nothing here pretends to"* — true when written and false since the
  transpiler landed, which translates between Rust, Go, Python and TypeScript. A message
  that denies a capability the tool has is worse than none, because the reader believes
  it and stops looking. It now names the four and says what adding a fifth costs.

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
