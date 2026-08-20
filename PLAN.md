# fun-refactor. Plan

Multi-language refactoring + code-intelligence CLI on tree-sitter, covering the funveil
language suite. Research and provenance for every design choice: see [RESEARCH.md](RESEARCH.md).

- **Crate**: `fun-refactor`, binary `fr` (provisional). Rust 2021, **AGPL-3.0-or-later**.
- **Repo**: `github.com/e6qu/fun-refactor`. Commits authored as `e6qu
  <2966430+e6qu@users.noreply.github.com>`; remote uses the `github.com-e6qu` SSH alias.
- **Languages** (16 variants): Rust, Go, Zig, Java, TypeScript/TSX, Python, Bash, HTML,
  CSS/SCSS, Terraform/HCL, Helm/YAML, XML, Markdown. Grammar pins inherited from funveil
  (tree-sitter 0.26 line). Java was added afterwards and priced at one query file, five
  lines of enum and three transpiler cases.
- **Feature families**: standard refactors (rename, extract/inline, move, change signature,
  safe delete, organize imports) + analysis (symbols/refs, call graphs, entrypoints,
  forward/backward flow, config-value provenance).

## Design decisions (baked in)

| # | Decision | Rationale (RESEARCH.md ref) |
|---|---|---|
| D1 | Self-contained binary; no LSP dependency in the core. LSP delegation is a late optional backend. | §3, §6.4, the unique value is where LSPs are weak; LSP drags in daemons/config discovery |
| D2 | Edits are byte-range splices on original source, applied descending by offset, validated by reparse + no-ERROR-node assertion. Never pretty-print. | §3, formatting/comment preservation for free; beats gopls's known comment loss |
| D3 | One unified property graph, shared nodes, independent edge layers (`REF`, `IMPORTS`, `CALLS`, `DFLOW`, `PROVENANCE`), built incrementally per language. | §6.3. Joern CPG model; queries degrade gracefully |
| D4 | Every resolved edge carries a confidence tag: `exact` / `import-qualified` / `field-based` / `name-only`, plus candidate counts on multi-candidate edges. | §6.4, characterized imprecision makes heuristic systems trustworthy |
| D5 | At unresolved call edges, flow queries stop and downgrade loudly, no silent over-approximation. Summaries (stdlib/framework) can extend reach explicitly. | §6.2, dev-tool honesty over scanner-style over-tainting |
| D6 | Config languages with a substitution model get provenance semantics (substitution/override chains, hop chains preserved immutably), not imperative dataflow; markup with neither model is refused by both and not answered emptily by one. | §6.2, deterministic evaluation models; fix Checkov's substitute-in-place flaw |
| D7 | Entrypoint detection is data (per-framework YAML catalogs, MaD-style schema), not hardcoded heuristics. | §6.5. CodeQL MaD + OWASP noir precedent |
| D8 | Unsupported operation × language combinations are refused with an explicit error naming the gap. No silent no-ops, no silent fallbacks. | engineering principle; also user convention |
| D9 | Every command has `--json` output; mutations default to dry-run unified diff, `--write` to apply, multi-file apply is atomic (all-or-nothing). | CLI-native + agent-friendly |
| D10 | Do not build on stack-graphs (archived 2025-09). Scope resolution via our own locals-style queries; graph construction may use tree-sitter-graph if the DSL earns its keep. | §3 |

**Open decisions.** None. The last one was whether to add the optional LSP delegation
backend, and it is answered below under Stage 8: the tool does not delegate.

Resolved since: the tool is `fun-refactor`, binary `fr`; extract-function landed for
both Zig and Bash without needing a CFG; and TSX `className` handles plain attribute
values but not helper calls or template literals, recorded as BUGS.md B14 and not
left as an open question, because it is a gap with known behaviour. It is not a decision.

## Language tiers

- **Tier A**, imperative, ecosystem-rich: Rust, Go, TypeScript/TSX, Python. Full refactor +
  flow surface. LSPs exist (future differential-test oracles and optional backend).
- **Tier B**, imperative, tooling-desert: Zig, Bash. Same feature shape as Tier A where
  syntax allows; instant best-in-class (no zls/bash-ls call hierarchy exists at all).
- **Tier C**, config/markup, string-keyed semantics: Terraform/HCL, Helm/YAML, CSS/SCSS,
  HTML, XML, Markdown. Rename/provenance/safe-delete are the stars; several features are
  structurally n/a and refused per D8.

## Reuse from funveil

Same author, compatible licensing, copy liberally, adapt aggressively. Every copied module
records provenance (source repo + pinned commit) in the importing commit message. Mechanism:
at Stage 0, clone `github.com/e6qu/funveil` at a pinned commit into a scratch checkout and
copy the modules in. Never depend on funveil as a crate. It is a binary crate, and the two
diverge immediately.

| funveil source | Reused as | Stage | Adaptation |
|---|---|---|---|
| Cargo.toml grammar pins (tree-sitter 0.26 + 12 grammars, markdown fork) | dependency set | 0 | as-is |
| `src/parser/tree_sitter_parser.rs` | parse layer | 0 | strip veil-specific metadata |
| workspace scanning (walkdir + ignore usage) | scanner | 0 | as-is |
| test infra patterns (rstest, assert_cmd, cucumber harness, coverage/mutation CI) | harness | 0 | selective |
| `src/parser/languages/*` (12 per-language extractors) | symbol extraction | 1 | biggest win; extend symbol kinds for refactor targets (esp. Tier C) |
| `src/analysis/cache.rs` | index cache | 1 | rekey by content hash |
| `src/analysis/entrypoints.rs` | entrypoint detection | 3 | keep the 5-category enum + API; heuristics become seed data for YAML catalogs (D7) |
| `src/analysis/call_graph.rs` | call graph | 3 | keep API shape (callers/callees/trace/format_tree/to_dot, petgraph core); **replace** string-name resolution with REF/IMPORTS-based resolution + confidence tags |

Not reused: veil/CAS/patch/profile/token-budget machinery (funveil's product core,
irrelevant here).

## Stages

Each stage lands as one PR (squash-merged), updates PLAN.md/BUGS.md, and must pass all prior
stages' corpora. Feature work within a stage rolls out Tier A → B → C unless noted.

### Stage 0. Substrate: parse + edit engine, **DONE**

**Goal**: parse all 12 languages; make and validate lossless multi-file edits; CLI skeleton.

Landed: `src/span.rs` (byte-native `Span` + `LineIndex`), `src/lang.rs` (language
variants. TS/TSX, CSS/SCSS and YAML/Helm split apart because they need different
grammars or handling), `src/parse.rs` (all grammars + Helm `{{ }}` masking that
preserves byte offsets), `src/scan.rs`, `src/edit.rs` (byte-splice engine, overlap
detection, reparse validation, atomic multi-file commit, unified diff),
`src/cli.rs` (`fr scan`, `fr parse --stats`, `--json`). 48 tests.

- Cargo project, clap CLI, `--json` global flag, tracing setup.
- Parse layer: 12 pinned grammars, `Language` enum, extension/filename mapping, parse
  diagnostics (`fr parse --stats`).
- Workspace scanner: walkdir + ignore (gitignore-respecting), language routing.
- Edit engine: `Edit { byte_range, text }` → per-file descending-offset application →
  workspace-level edit set → dry-run unified diff / `--write` → post-edit reparse with
  ERROR-node check (abort on regression) → atomic write (temp file + rename).
- Test harness: per-language fixture corpora, snapshot tests, property test
  "any applied edit set reparses without new ERROR nodes".

**Exit**: all fixture corpora parse; synthetic edit round-trips are byte-exact outside edited
ranges; diff/apply UX works end to end.

### Stage 1. Graph tier-0: symbols, scopes, references, imports, **DONE**

**Goal**: the resolution layer everything else stands on.

- Symbol extraction per language (kinds per language: fn/method/struct/trait/impl; class;
  interface/type; tf resource/variable/local/module/output; css rule/`$var`/custom property;
  helm values key/named template; yaml anchor; md heading/link-def; xml id; html id/class use).
- Scope resolution: own locals-style queries per imperative language; scope trees;
  shadowing-aware use→def (`REF` edges) within file.
- `IMPORTS` layer: Rust `mod`/`use` (in-crate), Go packages (in-module), TS relative imports +
  basic tsconfig `paths`, Python module paths, Zig `@import`, Bash literal `source` paths,
  SCSS `@use`/`@import`, HCL module sources, Helm chart structure, HTML `script`/`link` srcs,
  Markdown links.
- Cross-file resolution for import-qualified top-level symbols; confidence tags per D4.
- Persistent index cache (postcard) keyed by content hash. **Landed later, in `src/cache.rs`.**
- Commands: `fr symbols`, `fr def <file:line:col>`, `fr refs <pos|name>`.

**Exit**: refs/def corpora pass including adversarial shadowing fixtures; cache invalidation
correct; every ref answer carries a confidence tag.

### Stage 2. Rename (first mutation), **DONE**

**Goal**: the table-stakes refactor, all 12 languages, each meaning the right thing.

- Tier A/B: scope-checked rename; cross-file via import-qualified resolution for top-level
  symbols; conflict detection (existing name in any affected scope → hard error, D8);
  method renames limited to `exact`-confidence resolution, otherwise refuse with candidates.
- Tier C binders (each a small string-keyed resolver):
  - Terraform: rename resource/var/local/module/output address; update every interpolation.
  - Helm: rename `.Values` key path → values.yaml + all template refs.
  - CSS/SCSS ↔ HTML/TSX: rename class/id/custom-property/`$var`/mixin; update `class=` /
    `className` string literals and `var()` uses.
  - XML: id/idref pairs; namespace prefix rename.
  - YAML: anchor/alias rename.
  - Markdown: heading rename → regenerate anchor → update in-repo anchor links, reference
    links, footnotes.
- Post-rename textual sweep (all languages): old name in strings/comments → report only,
  never auto-edit.
- Command: `fr rename <pos|name> <new-name>`.

**Exit**: rename corpus per language incl. shadowing traps; differential spot-checks vs
gopls / rust-analyzer / rope on shared fixtures documented; sweep report emitted.

### Stage 3. Call graph + entrypoints, **DONE**

**Goal**: beat funveil's string-matching baseline with resolved, confidence-tagged graphs.

- `CALLS` edges per strategy (RESEARCH.md §6.4): Go package-qualified + CHA-style interface
  edges; Rust direct + impl-block receiver tracking, `dyn`/fn-pointer → multi-candidate;
  TS/TSX field-based (ACG) + annotation narrowing; Python PyCG-style assignment-graph (no
  bare name-matching); Zig name + `@import`, comptime sites flagged unresolved; Bash
  command-position + static `source` closure.
- Entrypoint catalogs (YAML, D7): schema `kind` × `threat_model` × per-language `match` ×
  `yields` × `provenance`. Starter catalogs: Rust (main/tokio/test/clap/axum/actix),
  Go (main/init/tests/net-http/cobra/gin), TS (Next.js/Express/React roots), Python
  (`__main__`/click/argparse/flask/fastapi/django/pytest), Zig (main/test/build.zig),
  Bash (script mains), Terraform (root variables = infra-input; SG/Service exposure =
  infra-exposure), Helm (values keys; Service/Ingress templates), Markdown/HTML mains
  (funveil parity).
- Commands: `fr callers <fn>`, `fr callees <fn>`, `fr trace <fn> --up|--down --depth N`,
  `fr graph --dot|--json`, `fr entrypoints [--kind K] [--reachable-from|--reaching <sym>]`.

**Exit**: zero same-name cross-file conflation on fixtures (the funveil failure mode);
precision/recall measured on fixture apps per language; catalog-driven entrypoints ≥ funveil's
detection on its own categories.

### Stage 4. Flow analysis, **DONE**

**Goal**: "where does this value come from / where is it used" for all 12, with the right
semantics per tier.

- Tier A/B (`DFLOW`): statement-level CFG per language; reaching definitions (GEN/KILL
  fixpoint); def-use chains; `fr flow back <pos>` / `fr flow fwd <pos>` intra-procedural;
  inter-procedural = query-time traversal across `CALLS` with param/return binding,
  confidence downgrade at every non-`exact` edge, `--depth` bound, D5 at unresolved edges.
- Tier C (`PROVENANCE`): Terraform value DAG (var/local/module/output substitution,
  tfvars/default precedence, multi-pass expansion, hop chains kept); Helm values precedence
  chain (values.yaml < parent < user file < `--set`) → template use sites; CSS cascade
  answer for a selector/property (origin → layer → specificity → order, losers listed) +
  `var()` chains; YAML anchor expansion provenance; Markdown/HTML link + id/`for` graphs.
- Unified hop-chain output: file:line per hop, edge kind, confidence.

**Exit**: slicing corpora per imperative language; Terraform/Helm provenance validated
against `terraform console` / `helm template` outputs on fixtures; CSS answers match browser
devtools on fixtures.

### Stage 5. Extract & inline, **DONE**

**Goal**: the extract/inline family, powered by Stage 4 dataflow.

- Extract variable: Tier A + Zig (expression boundary, insertion point, side-effect warning,
  name suggestion). Config analogues: SCSS `$var`/custom property from repeated value; HCL
  `locals` entry; YAML anchor from repeated node; Markdown reference-link def.
- Inline variable: single-assignment check via `DFLOW`; shadowing check at each use site.
  Config analogues: inline `local.x` / anchor / `$var` / reference link.
- Extract function: Rust, Go, TS, Python (ins→params, outs→returns, control-flow exit
  analysis; comments inside the region move intact, explicitly beat gopls here). Zig/Bash:
  decide per open-decisions.
- Inline call: strict preconditions (single return, no shadowing collisions, effect-order
  preserved); refuse loudly otherwise per D8. Helm: extract named template to `_helpers.tpl`.
- Commands: `fr extract var|fn <range>`, `fr inline <pos>`.

**Exit**: property tests, result reparses clean, extract→inline round-trips to semantic
no-op on fixtures; behavior deltas vs rust-analyzer/gopls documented.

### Stage 6. Move, change signature, safe delete, organize imports, **DONE**

- Move symbol/section to file with reference updates: Rust (module → file), Go (same-package
  split), TS (move-to-file + import rewrite), Python (symbol move + import updates),
  Terraform (resources between `.tf` files, flat namespace), CSS (rules between partials),
  Markdown (section → new file + link updates).
- Change signature (CLI-native; LSP has no equivalent): add/remove/reorder/rename params with
  all call sites updated. Tier A + Zig; Terraform module variables (add-with-default /
  remove / rename propagated to every call site); SCSS mixin params.
- Safe delete: refuse if references exist (list them); flag-only mode for reports. Dead CSS
  selectors (vs HTML/TSX usage), unused tf variables/outputs/locals, unused values.yaml
  keys, orphaned Markdown link defs, unused XML ids, unused functions (via `CALLS` +
  entrypoint reachability).
- Organize imports: Rust `use` merge/sort/unused, Go (goimports-lite), TS, Python, Zig
  `@import` consts, SCSS `@use` ordering.
- Commands: `fr move <sym> <dest>`, `fr sig <fn> <spec>`, `fr delete <sym>`, `fr imports`.

**Exit**: per-feature cross-language corpora; safety-refusal tests (delete/move/sig with live
refs must fail with the ref list).

### Stage 7. Cross-language intelligence, **DONE**

**Goal**: the queries nothing else can answer; mostly composition of existing layers.

- Stitched flows across file-type boundaries: Helm value → container env → `os.environ` /
  `process.env` read; Terraform output → consumed values; CSS custom property chains into
  computed use.
- `fr impact <target>`: blast radius across all edge layers (refs + calls + provenance +
  textual-sweep hits), grouped by confidence.
- HTML `id`/`for`/`aria-*` reference resolution (the gap vscode-html-languageservice leaves).
- TSX className richness (clsx/template literals) per open decision.

**Exit**: cross-language fixtures (mini app: Terraform + Helm + Python service + TSX front
end) with stitched-flow snapshot tests.

### Stage 8. Advanced & ecosystem, **DONE**: pattern restructuring, micro-rewrites and cascading cleanup are complete. The delegation backend is decided against, with the measurement below; the daemon is deferred with a reason

- Micro-rewrite tail (per-language `refactor.rewrite.*` equivalents: invert-if, guard
  clauses, de Morgan, fill-struct where syntax allows).
- Pattern restructure: user-supplied before/after patterns with scope-aware constraints
  (rope-restructure / ast-grep-style), plus Piranha-style cascading cleanup chains.
- Optional LSP delegation backend (`--engine lsp`) for Rust/Go/TS/Python. **Decided
  against.** A language server settles one shape: a member read from a value whose type
  this tool does not know. Measured on this repository, that shape is 400 of the 1,249
  renames that are incomplete, and 18,313 symbols have uses. Delegation would complete
  2.2% of renames and a third of the incomplete ones. It would cost server lifecycle,
  per-language project discovery, version skew, and D1's self-contained binary. It would
  do nothing for the 540 cross-language edges, which are what this tool has and an editor
  does not. Nothing for the 61 of 285 files here whose languages have no server.

  What makes the trade acceptable is that the 400 are refused and named, not rewritten
  wrongly. That is D4 and D8 working. The refusal now says why the site was left, so a
  reader knows what to check.

  What would reopen this: the shape becoming silently wrong instead of refused, or a
  language server that runs without a configured project.
- Daemon/watch mode with incremental reindexing; editor integration surface. **Deferred
  with a reason.** It changes speed and integration, not correctness. The fact cache
  already makes a second run cheap, and there is no editor integration for a daemon to
  serve. It is written down here so that its absence is a decision and not an oversight.

**Exit**: scoped when reached; each item ships behind its own corpus.

## End-state feature × language matrix

`fr capabilities` prints it, and `README.md` carries the generated copy. Each cell is
computed from the predicate the command itself asks, so the table and the code cannot
disagree.

A hand-written copy stood here and drifted, as a hand-written copy does. It had twelve
language columns and the tool has sixteen, so Java never appeared in it at all. Two of
its cells still read `tbd` for questions settled long ago. The stage each capability
landed in is in the stage sections above.

## Testing & quality strategy

Four layers, each answering a question the one below it cannot:

| Layer | Where | What it can catch |
|---|---|---|
| Unit | `#[cfg(test)]` beside the code | Local correctness: span arithmetic, negation, subtree hashing |
| Integration | `tests/*.rs` against the library | A refactoring's resulting bytes, per language |
| End-to-end | `tests/cli.rs`, `tests/test_pyramid.rs` | Argument parsing, path resolution, exit codes, the text a person reads |
| Real repositories | helm/helm, grafana/grafana, by hand | What people write, which is not what fixtures imagine |

The end-to-end layer exists because two bugs were found living in it, both of the
kind that answers wrongly while looking like it worked: `--path` filters built by
joining the default root `.` matched nothing and reported that as nothing found, and
target paths were read from the shell's directory and not from the workspace `-C`
names. Neither was visible from the library API.

`tests/test_pyramid.rs` enforces the layer. It reads
the subcommand list out of `fr --help` and fails if any command has no end-to-end
test. That guard is verified to bite, removing a command's entry fails the build
with its name. It also asserts that no command writes to the workspace without
`--write`, which is the promise the whole CLI rests on.

The fourth layer is deliberately not automated. Pinning a 500 MB clone into CI buys
less than the measurements already recorded in BUGS.md. The bugs it found were found by *reading* output. A silent guard-clause moved code out from
under its condition. A dead-code report was 84% false positives. No assertion written in
advance would have looked for either.


- **Fixture corpora**: per language × per feature, including adversarial cases (shadowing,
  aliased imports, same-name symbols across files, dynamic dispatch).
- **Property tests**: every applied edit set reparses with no new ERROR nodes; bytes outside
  edited ranges unchanged; rename A→B→A round-trips byte-exact; extract→inline round-trips.
- **Differential oracles** (test-time only, not runtime deps): gopls `rename`,
  rust-analyzer, rope, tsserver on shared fixtures; `terraform console` / `helm template`
  for provenance answers.
- **Honesty gates**: no edge without a confidence tag; no command silently succeeding with
  partial coverage, partial results must say what was skipped and why (D5/D8).
- CI from Stage 0; coverage tracked; mutation testing once the edit engine stabilizes
  (funveil precedent).

## Risks

- **Grammar quality variance**: shipped tree-sitter grammars differ in fidelity (esp. Zig,
  HCL forks, markdown). Mitigation: pin versions, fixture-corpus gate per grammar upgrade.
- **Scope-query authoring cost**: own locals-style queries per language is real per-language
  work (stack-graphs' lesson). Mitigation: only imperative languages need true scope trees;
  Tier C binders are string-keyed.
- **Method/dispatch rename correctness ceiling** without types: bounded by D4/D8 (refuse or
  present candidates and do not guess).
- **Helm templating is text-level YAML**: `{{ }}` breaks YAML parsing. Mitigation: parse
  templates with the funveil approach (tree-sitter YAML + template-token layer); treat
  render-dependent structures as unresolved, loudly.
- **Scope creep across 12 × 17 features**: the matrix's, cells are commitments to refuse,
  not gaps to fill. Tbd cells resolve via open decisions, not silent drift.

## Where this stands

Stages 0 to 8 are complete, and so are the five pull requests this plan sequenced. The
figures below were measured on this branch.

| | |
|---|---|
| Commits | 149 |
| Merged pull requests | 99 |
| Rust source | 55,839 lines |
| Tests | 1,914 in 98 files |
| Query sets | 14 |
| Entry-point catalogs | 10 |
| Capabilities × languages | 24 × 16 |
| Supported pairs | 269 of 384, every other one carrying its reason |
| Defects fixed | 311 |
| Defects open | 12 |

Every cell that `fr capabilities` marks `n/a` carries the reason the tool refuses, which
keeps that column a commitment and not a gap.

### The 12 open defects

They are not 12 pieces of pending work. Each was re-triaged against this branch, still
reproduces, and is pinned by a test that fails when it stops being true.

- **Eight are limits of a published grammar,** each naming the construct the grammar has
  no rule for at the version this build pins. B11 and B283 cover SCSS forms and the
  indented Sass syntax. B15 covers Go `new`. B231 and B232 cover TypeScript. B233 and B234
  cover Python. B133 covers Zig. `tests/known_grammar_gaps.rs` pins every one from both
  sides, the failing form and the neighbouring forms that work. So a grammar upgrade
  that starts reading one retires the entry on purpose and not by accident.
- **Three are incomplete answers that the tool reports.** B5 states what dispatch can be
  known without types. B13 states what a partial set of values inputs can decide. B14
  covers a CSS class assembled inside a helper call. Each stands on the report, not on the
  gap. `tests/open_defects.rs` asserts both halves: a rename that quietly skipped the
  helper call would satisfy the first half of B14 and fail the second.
- **A dispatch family renames, re-signs and deletes as a unit (B382, B383).**
  One `Hierarchy`, four commands. What spares implementations from `fr unused`
  carries `fr rename`, `fr signature` and `fr delete` now. The declaration,
  the implementations, and the unresolved dispatch sites move together. The
  last of those is reported for review.
- **B286 was a decision, and then a fix.** Inlining bracketed by the value; it
  brackets by the use site now. A use held by its own delimiters, a declaration, an
  argument list, a return, takes the value bare. A use under a tighter operator keeps
  the pair. The failure modes stay asymmetric, and the unrecognised parent still errs
  toward the bracket.

**No open defect is both this project's own and fixable here.** B263, the last one that
was, closed in #105; B300, the re-export barrel, closed on this branch. B286, B364 and
B365 closed on the branch after it. Inline brackets by use site now. The Zig
file-as-struct idiom reads as the record it is. The IR has a sum, so closed choices
cross all six languages.

### What the work became

The first eighty pull requests built the surface. The last fifteen found that parts of the
surface were untrue. None of them was a missing feature.

| Defect | What was claimed | What was true |
|---|---|---|
| B281 | Resolution strips the `#` from a fragment link | The code that strips it could never run |
| B282 | The tool reads the languages it lists | A `.js` file was not a source file at all |
| B287 | Sorting imports preserves meaning | It separated `#[cfg]` from the import it guarded |
| B288 | A `#[path]` attribute blocks a move | A doc comment blocked every move in the workspace |
| B291 | A rename rewrites what the name refers to | It rewrote method calls inside `assert_eq!` |

Each was found by the same method. Run the tool over a real repository, or over this one,
then ask whether the result still means what it meant. The test suite passed throughout.

### The gap that matters

Four of the last eight defects produced output that parses and does not compile.

| Defect | The output |
|---|---|
| B287 | An attribute guards the wrong import |
| B289 | An integration test imports the library as `crate::` |
| B290 | A signature changed and no call site was updated |
| B291 | A method call names a method that does not exist |

The edit engine has one automatic guard. It parses the file before the edit and after it,
and rejects an edit that introduces a syntax error. That guard cannot see any of the four.
Nothing in this repository compiles what the tool wrote. A person found all four by
reading the output.

## Sequenced work

Five pull requests in dependency order. PR 1 buys a gate that raises the value of PR 2 and
PR 3. PR 4 is independent. PR 5 is a product decision and not a debt.

**All five are delivered and merged.** What followed them was the work this plan had left
unnamed: the two commands PR 2 never swept, the two languages PR 1's gate never drove, the
re-export barrel `fr move` declined at. A re-triage of every open defect. That is finished too, and it found eleven more defects. The largest was a Go call into
another package resolving to nothing at all. That made `fr rename` and `fr signature` write
trees `go build` rejects.

### PR 1. Compile what the tool wrote

**Problem.** A refactoring can produce a file that parses and does not compile. The edit
engine reparses and accepts it. Four known defects reached the repository this way.

**Change.** Make a successful compile a condition of merging.

- Add a harness that copies a workspace, applies a planned refactoring, and runs the
  compiler for that language over the result: `cargo check` for Rust, `tsc --noEmit` for
  TypeScript, `go build` for Go, `python -m compileall` for Python.
- Supply one small workspace for each language, and use this repository for Rust. All four
  known defects appeared here.
- Drive every command that writes: rename, delete, inline, move, signature, imports,
  extract, restructure, rewrite and remove-flag.
- Name any language whose compiler is absent in the output of the run. A green result must
  never mean that nothing was checked.
- Call the harness from `tools/check.sh`, which is the one definition of passing.
- Fix every defect the harness reports, in this same pull request.

**Exit.** Revert each of the four fixes in turn and confirm the harness fails. Restore
them and confirm the sweep passes.

**Delivered.** `tests/output_compiles.rs` drives rename, move, signature, imports and
inline over two fixture crates and runs `cargo check --all-targets` on each result. The
first fixture has nothing awkward in it and every command has to produce a plan that
compiles. The second holds a free function and a method of the same name, called from
inside `assert_eq!` in an integration test. There a refusal is a result and the only
forbidden outcome is a plan that does not compile. The gate names the languages it does
not drive. It found four defects on its first run: B292, B293, B294 and B295.

### PR 2. Sweep the commands that write and have never been swept

**Problem.** `extract`, `restructure`, `rewrite`, `remove-flag` and `translate` have never
been run across a corpus with their results checked. Every command that has been swept has
had defects fixed against it.

**Change.** Run each of the five over this repository and the vendored corpora, and check
the results.

- Count panics, refusals and wrong output separately.
- Read every refusal. B288 was a refusal that named the wrong file for the wrong reason.
- Check the invariants that apply: idempotence for a command that normalises, an inverse
  where one exists, no new parse errors. The compile gate from PR 1.
- Fix what the sweep reports.

**Exit.** Each command has a recorded sweep with counts. Every invariant that holds is a
test.

**Delivered so far.** `rewrite` and `extract` are swept, and each produced a defect that
the compile gate then proved: B296 and B297. Both are fixture cases in
`tests/output_compiles.rs` now.

The compile gate drives TypeScript as well as Rust now, over a fixture with a re-export
barrel in it. That found B300 on its first run: a use reached through a barrel resolved by
name alone. So `fr rename` and `fr move` both wrote code that does not compile. Resolution follows the chain now. `fr move` declines when a barrel exports the symbol.
Repointing an export is a different operation from repointing an import. A test also breaks each fixture on purpose and checks the
compiler complains, because a gate that cannot fail is worse than none.

`restructure` is swept, by asking it for a rewrite that changes nothing. Eight identity
patterns over `src/` changed files eight times out of eight, for three separate reasons,
and none of the three broke a build (B301). An identity is a good sweep for a command that
takes its instruction from the user. It needs no invented pattern and the correct answer is
known in advance.

Both remaining sweeps exist now. `tests/remove_flag_sweep.rs` drives `fr remove-flag`
end to end in seven languages over synthesized flag fixtures, which are the corpus this
repository did not have. `tests/translate_corpus_sweep.rs` translates every corpus file
to every target in process and ratchets the carried-construct ledger in both
directions. The ledger has since kept paying. It recorded the day Zig call arguments
started carrying (B376). It shrank as error propagation, the optional payload `if`
and `while`, literal-armed switches and named tests each gained a crossing. It was
the witness when the Go and Java readers were caught reading `+=` as `=` (B378).
It watched again when a call to a declared record crossed as a call (B379).
`defer` crosses now too, native in Go and Zig and said with `try`/`finally`
everywhere that has one. So do counted loops, keyword arguments against a callee
declared in the same file, and the statement-shaped ternary that Go unfolds into
its `if`/`else`. The paragraph below records why they were last. `remove-flag` has no boolean constant to
target in this repository, so it needs a corpus that has one. `translate` has
`tests/round_trip.rs`, which asks more of it than a sweep would. The first two take a
pattern from the user, so a sweep has to invent the patterns and a poor choice measures
nothing. `translate` has `tests/round_trip.rs`, which is a stronger check than a sweep
would be. They are the remainder of this pull request's scope.

### PR 3. Make the commands that read agree with each other

**Problem.** `refs`, `usages`, `callers`, `callees`, `graph`, `impact`, `flow`, `stitch`
and `duplicates` answer overlapping questions from one index. When two of them disagree,
one is wrong. Nothing checks this today.

**Change.** Write the agreements down as tests over real repositories.

- `callers(X)` is a subset of `refs(X)`.
- Every edge in `graph` corresponds to a call reference in the index.
- `impact(X)` includes `refs(X)`.
- `usages(X)` is `refs(X)` grouped by file.
- Every span that `duplicates` reports parses.
- Where a command stops early, its output says so. `callers` reports its depth limit
  today. The others are unchecked.

**Exit.** The agreements are tests. Every disagreement is fixed, or recorded with the
reason it is correct.

**Delivered.** `tests/commands_agree.rs` asks all six agreements of this repository, which
is the largest workspace the tests have. Every one held: 12,170 resolved call edges each
with the reference that produced it, no call site outside the function it is attributed
to, callers and callees symmetric, `usages` equal to the references that resolved,
`impact` covering every reference it could rewrite. Every span `duplicates` reports
inside its file.

The disagreement the sweep found was between a report and itself. Four lists stopped early
without saying so, one of them beside a list in the same report that did (B298).

### PR 4. Namespaces, with B263 as one instance

**Problem.** A Terraform `var.x` and a `local.x` are different declarations. The index
records them as one symbol, so `fr refs` on either returns both. This is a shape that
other languages also have.

**Change.** Record the namespace that a declaration was written in.

- Fix B263 through that record, and not by naming the two Terraform prefixes at
  resolution.
- Look for the same shape in every other language: an inherent method beside a trait
  method in Rust, a package function beside a method in Go, a class and an element id and
  a custom property that spell the same name in CSS, an anchor beside a key in YAML.
- Write a test for each instance found, then fix it.

**Exit.** `fr refs` on one of two declarations that share a name in different namespaces
returns only its own uses, in every language where the shape exists.

**Delivered.** Two instances, both fixed. Terraform's `var.thing` and `local.thing` are
told apart by the block each is written in, which the index already recorded (B263). A CSS
class and an element id are told apart by the attribute that names them, which the query
now says and `Reference::expects` carries (B299). The second was the worse of the two: a
rename of the id rewrote `class="thing"` at `exact` confidence.

Three languages were checked and need nothing. Go already refuses to read a bare call as a
method. Rust's inherent method beside a trait method needs types, and the answer is
reported `field-based`. YAML's anchor and key of one name resolve separately.

### PR 5. Stage 8: build the delegation backend, or record the decision not to

**Problem.** Stage 8 lists a delegation backend and a daemon. Neither exists. The plan
calls both optional, so the stage cannot close while their status is unstated.

**Change.** Either implement the backend or record the decision.

- `--engine lsp` for Rust, Go, TypeScript and Python. Probe the server for the capability,
  call `prepareRename` and then `rename`, apply the returned `WorkspaceEdit`, and refuse
  when the server declines.
- The diagnostics that a language server returns after an edit are a second form of the
  gate from PR 1. They suit a language whose compiler is too slow to run for each
  refactoring.
- Keep the daemon and the watch mode separate. That work changes performance and
  integration. It does not change correctness.

**Exit.** Scoped when reached. A written decision not to delegate, with the reason, closes
the stage as well as an implementation does.

**Delivered.** The decision is recorded above with the measurement behind it, and Stage 8
is closed. The daemon is deferred with its reason. What follows from deciding against
types is that a refusal has to explain itself. So `fr rename` now names the cause of each
site it leaves: read from a value of unknown type, written inside a macro, or matched by
name alone.

## Progress log

Every stage is complete except the optional LSP delegation backend. Every
capability a language can meaningfully support is built: **273 of 384 capability ×
language pairs supported, 115 not applicable, none refused.**

The matrix is no longer maintained by hand. `src/capabilities.rs` computes it by
asking each refactoring's own predicate, `fr capabilities` prints it with the reason
attached to every non-supported cell. A test asserts the README matches. That
exists because the hand-written version drifted twice, once hiding 27 unbuilt cells,
once publishing six working ones as refused.

The compile gate drives six of the sixteen languages. Rust, TypeScript, Go, Python, Zig
and Java, and names the ten it does not on every run. The ten have no compiler to run: a
stylesheet, a manifest and a document are checked by parsing them, which the edit engine
already does.

It drives every command that writes, across those six languages. `output_compiles.rs` puts
the ones that move a declaration through it, rename, signature, move, inline,
`rewrites_compile.rs` the ones that rewrite one in place, extract, rewrite, restructure,
and `removals_compile.rs` the ones that take code away: delete, imports, remove-flag, and
recipe, which composes them.

The second sweep found nothing, which is worth recording as a result and not an
absence. The third found three, and all three are one shape. The last use of an import lives in the
code being removed, and the statement stays behind. Every one of them parses,
which is why the parse sweeps missed them and a compiler caught them.

A fourth file, `validators_accept.rs`, drives five more languages by the tool that owns each
one. Not by a compiler: `bash -n` with shellcheck and then the script itself, `terraform
validate`, `helm lint`, and `xmllint` for XML and HTML. "Has a compiler" was the
wrong bar, `terraform validate` resolves references, `helm lint` renders the chart against
Kubernetes' schemas. Each rejects things tree-sitter reads happily.

That sweep found no defects either. It found two mistakes of mine, both worth recording. In bash, `$NAME` in a restructure
pattern is a metavariable and not a shell expansion, and the tool documents that. The bash arm of the gate was too weak until it ran the script. `bash -n` cannot see a call
to a function that moved to another file.

CI installs `terraform`, `helm`, `xmllint` and `zig` so those sweeps run there and not only
on a laptop; only `shellcheck` is already on the runner. Zig had been absent since the gate
started driving it, and the rule below is what found that. A validator the gate cannot find makes its cases skip themselves and say so. That is honest
on a laptop and useless on CI. There `cargo test` captures the line, and a hole looks like a
pass.
Each gate file therefore fails on CI when a tool it names is absent, and says which.

Not driven, and why. **scss** has no `sass` on the machine this was built on. **markdown**
has nothing to validate. **yaml** is checked as part of the chart `helm lint` renders.

`fr translate` is the one writing command not driven here. Its output is a draft that
carries unresolved constructs by design. So compiling it would fail correctly and prove
nothing; `tests/round_trip.rs` and `tests/translate_sweep.rs` cover it instead. The IR
carries distinct types since B358. `tests/translate_newtypes.rs` holds each writer to its
language's spelling of one. `tests/cli_translate_flags.rs` holds the blocked-destination
listing, `--out` and `--force`.

### What the matrix claims, and what the suite drove

`fr capabilities` computes the matrix from each refactoring's own predicate. So a `✓` means
"this command would accept this language" and not "this has ever worked". Nothing had ever
asked which of the 269 supported cells the tests reach.

Measured, and not argued about: every capability records the language it ran against
when `FR_CAPABILITY_LOG` is set. The first run answered **205 of 270, 75%**. It is
**269 of 269** now, and `tools/check.sh` measures it on the test run it already does, so
the figure is defended and not checkable. `tools/capability-audit.sh` asks the same
question on its own, through the same reporter, so the two cannot drift apart.

`tests/capability_claims.rs` is what closed it, and it asks the sharper question and not
the easier one. Every claimed cell is driven against a fixture in that language, and one
thing is asserted: it must not answer that the language is unsupported. That is the exact
contradiction a wrong `✓` produces. It found one, `fr move` telling a Rust user that
Rust was unsupported when the fault was the destination path.

What that file deliberately does not assert is that each answer is *good*. The four gates do
that. This checks the claims are true.

### The other half: what the empty cells promise

`n/a` is a claim too, the command does not do this here, and nothing drove those 112
cells, so it was unfalsifiable. `fr remove-flag` was breaking it on XML: an entity flag was
substituted, `&use_new;` became `&true;`, and the prolog went with the declaration.

`every_unsupported_capability_refuses_the_language_it_disclaims` drives every disclaimed
cell and fails when one proceeds. It found 35 at first, and separating the two kinds of
promise is what made the number mean anything. A whole-workspace analysis takes no language
argument. So `n/a` there says the language contributes nothing, and not that the command refuses.
`capabilities::is_whole_workspace` names that distinction. It had lived only in which of two
recording functions a call site happened to use. Of the 95 that remain,
seventeen were real: XML flag removal, and `fr type` and `fr flow` answering emptily for
nine languages each. Two more went the other way. `fr extract --function` writes an SCSS `@mixin` and a shell
function, and the table said it could not. The matrix grew to 272.

The driver could not tell "the command proceeded" from "the fixture had nothing to offer".
Eleven arms folded a missing symbol or span into `Ok(())`. Both tests now count
those apart and report them, and both currently reach every cell they claim to.

### What a writing rule can and cannot catch

The five rules in `tools/check-prose.py` were each read against what they caught, and
three of them were flagging good writing:

* **`exactly`** is filler in "exactly what the branch above did" and precise in "rewrites
  exactly the bytes of a name span". The rule asks for the emphatic form now: `exactly`
  in front of a demonstrative or a wh-word.
* **A negation** is often the most precise sentence available. "the guard was
  file-scoped, not scope-scoped" and "Structure is compared, not text" name the thing a
  reader would otherwise assume. The rule asks for the shape where the negation carries
  the weight and the positive claim arrives late or never.
* **"which is what X"** identifies a thing, and the clause is the shortest way to say it.
  The rule keeps "that is why" and "is what makes", which point at the text instead of
  carrying it.

A Markdown table row is data, and the rules were reading one as prose. 60 cells holding
`—` for "not applicable" were counted as 60 defects.

Both numbers are worth keeping apart, so both are recorded. Measured with the rules
unchanged, rewriting halved every one of them:

| | before | after |
| --- | ---: | ---: |
| em-dash | 220 | 72 |
| false-comparison | 235 | 103 |
| filler | 338 | 123 |
| self-reference | 211 | 88 |
| sentence over 25 words | 2,261 | 1,130 |

Most of the last row came from `BUGS.md`. Its fixed section held 333 entries and 31,000
words, and an entry for a defect closed months ago needs the symptom and the fix. It holds
9,000 words now. The entries below B300 keep their symptom line, and git keeps the rest.

Sentences were split at a connective, and only where the tail stands on its own: it has to
hold a finite verb and must not open with one. Three earlier attempts without that guard
each produced fragments. "Including comments". "Replaced". "which is ordinary Zig. Is the
only parse failure". Punctuation can be moved by machine. A sentence needs the guard.

### The type-safety tutorial

`docs/type-safety.html` teaches typed thinking in eight steps. It starts at simple
types. It walks through aliases and units, domain types, parsing at the edges,
functions as values and purity, and it ends at composition and monads. Five conversion exercises
close it. Every example appears in Python 3.14 and TypeScript 5.9 side by side.

The examples are files under `tests/typesafety/`, one pair per example. Each file
declares the verdict the checker must give it: `expect: passes` or `expect: fails`.
`tests/typesafety.rs` runs mypy 1.19 strict and tsc 5.9 strict over all of them. It
executes the files tagged `run: yes` and regenerates `docs/typesafety-data.js`. It
also holds the page's example slots and the file set in agreement. A statement on
that page about what a checker accepts is a statement CI put to the checker.

### What the browser can show

The playground runs the same library as the terminal program, compiled to WebAssembly.
Two things it could not do:

* **Draw the call graph.** The only graph it could ask for was `graph`, which answers
  with three counts. `CallGraph::neighbourhood` returns the functions within a few hops
  of one symbol and the edges between them. `graph_around` serialises that for the
  browser. The editor window has tabs now, so the drawing sits beside the source. A
  click on a node opens the file at that name. The walk lives in the analysis and not in
  the binding, so a test reaches it without a browser.

  Opening the published site and clicking a node found the next one: a node carried its
  line and no column, so the cursor landed on the indentation and the status bar answered
  "nothing the index knows at this position". Recorded as B352.

* **Trace a value in a config language.** `fr flow` picks between dataflow and
  provenance for the caller. The browser bindings called dataflow whichever the language
  was. Recorded as B340.

### How this project writes

The comments, the messages and the documents follow one controlled style. It comes
from ASD-STE100, Simplified Technical English. Aerospace maintenance manuals use that
standard, so that a reader with limited English can follow a procedure safely.
`docs/style.md` holds the rules that apply here. `docs/terminology.md` holds the terms,
one meaning for each.

The prose here had a voice, and the voice was a machine's. Counted across the source comments, the messages and the documents: 2,050 em-dashes and 350
filler words. 222 sentences pointed at their own text, and 2,339 ran over 25 words.

`tools/check-prose.py` counts those habits and `tools/check.sh` runs it. The numbers
live in `tools/PROSE-DEBT`, and the check fails in both directions. A count that rises
fails, and a count that falls fails until the number is lowered to match. Neither
direction can happen quietly.

Two things this measurement taught, both against the first guess:

* **The comments did not repeat the code.** A scan for a comment whose words all appear
  in the line below it found three, and all three were section dividers. The problem was
  length and voice.
* **A negation is often real.** "The use of `x` binds to the inner `let x`, not the
  outer one" is precise. A rule against every negation would have removed it. The
  rule asks for the rhetorical shape instead: assert a thing, then deny an alternative
  that was never a candidate.

### The advice in a refusal is a claim too

A refusal that stops at "no" is worth less than one that says what to do instead, and
nearly every one of them does: `fr delete` removes a declaration nothing uses, rename the
file to `.scss`, invert it instead of guarding, move it to a package neither imports. Of
366 distinct refusal messages, **21 name a route the reader can take**, and nothing drove
any of them. The sentence was the one part of the message no test read.

Driving them found **five wrong**, and the two worst were the two that named a command:

* `fr flow` refused eight languages and sent the reader to `fr provenance`, **which is
  not a command**, the command is `fr flow`, which chooses between dataflow and
  provenance itself. It also promised an answer for HTML, XML and Markdown, where
  provenance has no arm and stops at the first hop.
* Provenance's own refusal named `analysis::flow (backward/forward)`, a library module,
  to readers holding a CLI or a browser.

Both were written in the same week as the code they describe, which is the point: advice
is prose, and prose is not compiled. Behind them was a matrix cell claiming provenance for eight languages when the dispatch has
five arms. A unit test asserted that every language gets one of the two analyses. The matrix had been shaped to
satisfy the rule and not to describe the code. Three languages get neither, and the
rule that holds is that no language gets both.

Chasing "is the route reachable?" also found the browser had no route at all. `fr flow` picks
the model on the caller's behalf, and the wasm bindings never did. So a YAML anchor
the CLI traced came back empty in the playground.

The other three were advice that led somewhere that also refuses, or nowhere. "Move it
somewhere under `src/`": Rust reaches a file through a `mod` declaration, so the obvious
destination is refused too. The second refusal named no route at all. And `fr remove-flag`
said "say which one with a position", for a command that took a bare name. That one was fixed by giving it the position form `fr delete` and `fr rename` have always
had. The advice was better than the command.

`tests/refusal_advice.rs` drives each route: it provokes the refusal, then does what the
sentence says and checks that it works. A refusal that names a way out now fails the
build when the way out is shut.

### Tests that passed without checking anything

A test that cannot fail is worse than a missing one, because it is counted. Sweeping for the shapes turned up 53. A loop over a collection that may be empty. A `let
Some(x) else { return }` that skips in silence. A skip path that always fires. An assertion
behind an early exit. They are listed with their fixes in BUGS.md as B331–B336.

The largest group was the compile gate: twenty-six sites called `gate` and discarded its
answer. So `…_compiles_or_refuses` passed either way and two of them had never reached a
compiler. Each now names the outcome it expects. The worst single one was a cascade test whose fixture never referenced the flag it was about.
`remove_flag` bailed, and the body sat behind `if let Ok(plan)`. It had asserted nothing since the day it was written. Fixing
it is what found the XML corruption above.

The pattern worth keeping: every one of these was found by making the test say how much it
had checked, and then reading the number.

Open limitations are in BUGS.md. All twelve are described in writing, pinned by a test,
and none is a missing feature: reachability under dynamic dispatch (inherent), Helm values
passed on a command line (invisible to a workspace scan), CSS classes named inside TSX
helper calls (a per-library convention, measured), how `fr inline` brackets a value (a
decision, with the asymmetry stated). Eight constructs a published grammar has no rule
for.

### How the defects were found

Fixtures test what somebody thought to write down, and they passed. Five other methods
produced the findings below:

- **Run it on its own source.** 30,000 lines of Rust, in languages the tool handles. One
  round-trip translation pass found nine defects.
- **Run it on somebody else's.** Five repositories, chosen to differ: a Go tool, a
  TypeScript framework, a Python formatter, a Next.js application, a Spring application.
- **Sweep one operation across every language it claims.** Six writers doing the same
  thing six ways exposes a rule true only of the language it was written against.
- **Feed the output back in.** Anything the tool emits, it should be able to read.
- **Ask whether a test checks what its name claims.** Several did not.
- **Ask what a test would still pass on.** A loop over an empty collection, a skip that
  always fires, a refusal counted as success — 53 tests could pass while checking nothing.
- **Do what the error message says.** 21 refusals name a way out. Five named a command
  that does not exist, a module, or a destination that also refuses.

Five recurring shapes, each of which has caught more than one defect:

1. *A rule true of the languages it was written against, applied to one that arrived
   later*. Java constructors, Go interfaces, Zig's six spellings of a receiver.
2. *Where does the search stop, and does the output say so?*, `fr impact`'s depth bound,
   `fr duplicates`' threshold, `fr unused`'s composition.
3. *Does the test check what its name claims?*, one asserted a cache fingerprint was
   steady, and not that it was correct. Several counted results without inspecting them.
4. *The tool's own output is not valid input*, enum-variant struct literals it could not
   re-read, FastAPI handlers it emitted and then reported dead, `SymbolKind` JSON it
   could not deserialize.
5. *A framework calls it and the source never does*. Python's `__main__` guard, pytest
   fixtures, Next.js server actions, eleven Spring annotations, JUnit test classes.

### Real repositories

Baselines measured before anything was changed, then again after:

| Repository | What it is | What it surfaced |
|---|---|---|
| helm/helm, 1,406 Go files | a tool | qualified names unusable as targets; interfaces matched on arity alone |
| vuejs/core, 547 TS files | a framework | parse failures reported without a position |
| psf/black, 342 Python files | a formatter | two grammar gaps, all four pinned upstream |
| vercel/commerce | an application | server actions read as dead code |

The application mattered most, because the first three are libraries and tools whose
entry points are conventional. An application is reached through a framework.

Two measurements worth keeping. helm resolved 27% of call-graph edges and dispatched the
other 73%, which sent the next probe at class hierarchies. A fix that looked right by edge count was caught by measuring dead code instead. Comparing Go
signatures *as written* refused seven `PrintingKubeClient` methods. `ResourceList` inside the
package and `kube.ResourceList` outside are the same type spelled differently.

### The log

Build-out, in order: sixteen languages; six transpiler readers and writers, thirty ordered
pairs; the recipe language; the entry-point catalogues; the published site and its
WebAssembly playground; the refactoring catalogue page; the API-contract invariant; the
types tutorial. Each is recorded in BUGS.md with what it broke on the way.

What the sweeps found, grouped by what went wrong:

**An expression moved into a context it was not written for.** Caught four times, in
`fr inline`, `fr restructure`, `fr extract` and `translate`. Each time the fix was
bracketing driven by one shared predicate, replacing four local ones. The operators the
six languages spell alike and mean differently, division, remainder, string equality,
account for most of it.

**A refactoring that left the program not compiling.** A move that left an import pointing
at nothing. A move that left its dependencies behind; a signature change that skipped
every `new`; a flag removal that took a class with it; a rewrite that negated half a
condition; a method that could not change its own object.

**A reader that dropped what it did not recognise.** Record members, constructors, type
annotations, Rust's `Counter { value: 0, step }`. Silently, until the round trip started
comparing what came back.

**An answer that was true and not usable.** Qualified names the tool printed and would not
accept back. A parse failure that said how many and never where. A trace that went one hop
and printed four; a threshold mentioned only when it found nothing.

**Documentation that had stopped being true.** Three separate passes. The first was a
sweep, find the stale number, replace it. The second found five defects the sweep
had walked past, all of them the tool saying something untrue about the tool. The lesson
stuck: the capability matrix is now computed from each refactoring's own predicate, the
site's command names are checked against the binary. So is the list of commands below.

**The site.** Driven in a browser and not read, which found dead links and a page that
was three commits behind and did not say so. Every page now stamps what it was built from.

### The last four findings

These are recent enough that the reasoning is still worth having in full.

**A framework calling it makes it an entry point.** Asked of every framework the
catalogues claim, without waiting for a repository to surface the next one. FastAPI handlers were dead code. This project has a page devoted to porting Next.js routes to
FastAPI, and its own `fr translate <route> fastapi` emits handlers it then called unused.
Flask and actix were covered only by coincidence: `@app.route("/health")` above
`def health` spells the symbol's name in a string literal, which `fr unused` skips. Three defects underneath the rules. A dot in an annotation's arguments captured the name, so
`@app.route("/v1.0/status")` matched nothing while `/status` matched. `export` between a
decorator and its class ended the search. So `annotated_with` did not work on exported
TypeScript classes at all. And a decorator's name is not unique across libraries,
`@app.patch` tagged twenty-two of black's test methods as remotely reachable, because
`@patch` is `unittest.mock`'s. What separates them is what the decorator *names*, a path
or a module, so route rules ask for `/`.

**One definition of passing.** A branch was pushed green and rejected. `cargo fmt --all
--check` was one of CI's steps and not one of the commands run locally. Neither set was
wrong; there being two sets was. `tools/check.sh` holds them and the workflow calls it.

**A queue that never cancels needs something that does.** The Pages deploy job held its
concurrency group with `cancel-in-progress: false`. So that a publish in flight could not
be interrupted. A job then stopped between being created and running its first step, and stayed `queued` for
fifty-three hours. Nothing evicts the holder of a group that never cancels. So twenty-four later runs queued behind it and were cancelled one at a time as
the next push arrived. Two days without a publish, reported as twenty-four cancellations and no failure anywhere.
The shape that hides longest is the one where every part reports something other than
"wrong". The guard bought against interruption cost far more
than interruption would have. The interruption it feared is not a real outcome: a
Pages deployment swaps its artifact in atomically. So a superseded one leaves the previous
version serving.

**Being the only method of that name is not knowing the receiver.** A single definition of
a name in a file resolved any use of that name at `Exact`. The rule did not exclude
member accesses, so `fr rename total sum` rewrote `client.total()` on a boto3 client
because a class in that file declared `total`. Only the top two tiers are rewritten, so
this was an unasked edit. It was not a misleading report. `FieldBased` is defined as this exact
case: the tier existed and was not being used.

**The tier is decided once.** Asking whether stronger typing would have made that
unrepresentable found the fix incomplete: the branch above it held the same belief and
still rewrote the call when it was written inside the declaring class. `resolve_one`
returned `(Option<SymbolId>, Confidence)`, which lets any label sit beside any answer
across twenty-eight branches. The rule now lives in one place, resolve, then cap what the
answer may claim. `EdgeOrigin::Hierarchy(basis)` one module over is the shape that
never had the problem, because it carries its justification inside the variant.

Costs, measured across three repositories: black's exact edges 881 → 795, vuejs/core's
2384 → 2240, helm's 4727 → 4133. Those are reported for review now instead of rewritten.

**Where else a type could have said it.** Asking that question of the rest of the codebase
found four more, all the same family. A value that is *checked* somewhere instead of being
*unrepresentable*.

A catalogue's `symbol_kind` was a `String` and its `languages` a `Vec<String>`, compared
against the real enums by name. `deny_unknown_fields` rejects a misspelled key; nothing
rejected a misspelled value, so `symbol_kind: functoin` and `languages: [pyhton]` parsed,
loaded and never fired, a rule that is present and never true, which reads like a
framework that is covered and absent. Parsing them into the types they denote turns both
into a message at load with the line, the column and the values that would have worked.
`Rule.provenance` went too: a field defaulting to `"manual"`, written by no catalogue and
read by nothing.

Underneath that was a real defect. `SymbolKind` has a serde derive *and* a hand-written
`as_str`. Three of twenty-one variants disagreed, `as_str` said `type`, `link-def`,
`element-id` where serde wanted `type_alias`, `link_def`, `element_id`. The output uses
`as_str`, so `fr symbols --json` emitted `"kind": "type"` and the tool could not read its
own JSON back. Shape number four again, in a place nothing had thought to look.

It hid because `as_str` meant two different things. On `SymbolKind`, `Confidence` and
`EntryKind` it is an identifier. It goes into JSON, into a catalogue, into a person's
fingers, and has to match the serde spelling exactly. On `Capability`, `Basis` and
`DefinitionRole` it is prose for a reader: "call graph", "from the literal", "also
declared here". Those three are `label()` and `describe()` now, and the identifier ones
have a round-trip test that reads its cases out of the exhaustive `as_str` match instead
of a list, the compiler already forces a new variant into that match. So a new variant is
covered the day it is added, and not the day somebody remembers.

And `fr type --json` was answering with `"symbol". 1` and `"defined_at": 0`, `SymbolId`s,
positions in one run's index, unstable and useless to a reader, with `defined_at` looking
like a line number. The text rendering resolved them all along; only the machine-readable
half did not.

### A real Java application

Java is Tier A with twelve catalogue rules and had only met fixtures; this repository
contains no Java. `spring-petclinic`, 49 files, answered with 3,554 findings, 35 of them
code. Five defects sat in front of the three that remained.

**Package clauses.** Java classes in one package never write its name and nothing imports
Go's `main`, so no package declaration has a reference. Petclinic reported all 49, one per
file. Removing one is a syntax error. Rust's `mod helper;` shares the symbol kind and
differs, a child module nothing references is a finding, so the exclusion tests the
language, not the kind.

**Containers of entry points.** JUnit constructs a test class to run its `@Test` methods;
nothing names the class. The check walks the containment chain instead of testing the
language, so it also covers Rust `mod tests` and Python classes of pytest cases.

**JavaBean accessors.** `getAddress` reported dead while the template writes
`${owner.address}` and the tests write `param("address", …)`. Java templates, JSON mappers
and Spring's binder reach a getter by the property name.

**HTML attribute values.** `is_string_kind` matched node kinds containing "string", and
the HTML grammar names an attribute value `attribute_value`. So `th:text="${owner.address}"`,
`v-on:click="submitOrder"` and `class="table-striped"` were invisible to the correction
that spares names spelled in strings, and 80 of petclinic's CSS classes reported dead
while its templates used them.

**Three Spring annotations**, `@InitBinder`, `@ModelAttribute`, `@Configuration`,
joining the eight from the earlier sweep. That sweep enumerated what Spring calls; these
came from running the tool at an application.

Code findings: 35 → 3, a constructor Spring calls, a testcontainers field, a nested
`@TestConfiguration`.

`fr unused` also printed 3,554 with no breakdown, 3,439 of them in one vendored
stylesheet. An answer of 50 or more now lists its top five kinds, plus the file holding
them when one file holds over half. vuejs/core: 1,640 keys in `pnpm-lock.yaml`. Nothing
is excluded from the analysis.

### Terraform at scale

`terraform-aws-vpc`, 77 `.tf` files, parsed without an error. `fr unused` answered 369,
of which 46 were HCL blocks and every one was `terraform {}`, `required_providers {}`,
`lifecycle {}` or a `dynamic` block's `content {}`. None of those carries a label, so
Terraform gives none of them an address and nothing can reference one. A labelled block
takes its name from a string label, so the quote before the name settles it. 369 → 323,
the remainder Markdown headings.

The run also found B263, which is not fixed. `var.x` and `local.x` are separate
namespaces; the index records both declarations as `SymbolKind::Variable` with no
qualifier. Where a variable and a local share a name — 18 of 81 in that repository,
`fr refs` on the variable returns the local's reference as well as its own. `fr refs`
on the local returns none. Both drop to `field-based`, so nothing is rewritten. The
reference half is a one-line query change. The symbol half is not, because `var` and
`local` appear in no declaration and a query cannot synthesise a name. So the qualifier
would have to come from `extract.rs` and would change every HCL qualified name and the
cache schema with it.

### Zig at scale

29 files of Zig's own standard library, `http`, `json`, `fmt`. One parse failure, and it
is B133: `const T = struct {};`, which `tree-sitter-zig` cannot read. The gap was already
recorded from a fixture; the standard library uses it.

`fr entrypoints` found 12 tests where the corpus has 495. Zig writes a test as
`test "any prose you like" { … }` and the query makes the description the symbol's name,
so `name_prefix: test` matched the twelve whose description begins with "test". The other
483 read as dead code, and so did everything only they called. Matchers gained
`declaration_keyword`, the third predicate that is not a property of a name after
Python's `__main__` guard and Next.js's `"use server"`. Entry points 12 → 472, dead-code
findings 643 → 204, and 538 → 99 with `--internal`.

Checked and not a defect: 240 `pub fn` declarations reported as unused. Zig `pub` sets
`exported`, so `--internal` already separates them — 105 of the 643.

### Properties over real code

Two invariants asked of `psf/black` and `helm/helm`, and not of fixtures.

`fr imports` is idempotent: 18 of 40 files changed on the first run and none on the
second. It also removed only genuinely unused imports, one name across 40 files, and
`ast` confirms nothing in the file referenced it. A first pass at checking this compared
diff lines and produced eleven suspects; all eleven were the sort step moving a line, not
a removal. Comparing the imported-name sets before and after is the check that answers the
question asked.

The round-trip attempt found something else on the way: `fr symbols` takes `--lang` and
`fr unused` takes `--language`, for the same filter. Five commands to two, with nothing to
say which is which. `--lang` is the name now, `--language` an alias so nothing already
written breaks.

The property itself holds. Fourteen uniquely-named Go callables in `helm/helm`, renamed to
a placeholder and back. All fourteen left the tree byte-identical, including the files the
rename decided not to touch. A larger run was cut off by a time limit and not by a
failure, so fourteen is what was checked. `tests/rename_inverse.rs` pins it on a workspace
that spans languages, where a CSS class named from HTML and TSX gives the inverse more to
get wrong. The test is verified to fail when the reverse rename is given a different
name.

### Helm charts at scale

Three `bitnami/charts` charts, 92 YAML files: 48 failed to parse. The masking replaced
every `{{ … }}` with same-length `x` bytes, which is a scalar everywhere, including the
positions where YAML needs whitespace, a comment, or nothing at all. Five distinct cases,
fixed as B278: an action supplying the block indented under its key, the continuation
lines of a multi-line action, the first line of a block scalar, an action at column zero
inside an indented block scalar. A `{{/* … */}}` template comment containing `}}`.
After the fix, 4 fail. All four put an action in key position. So do 3 files that
parse cleanly, which made the parse error useless as the signal for it. The key
has no name before the template renders, so the entry is absent from the index either
way. B279 reports that as a `FactGap` carried with the facts, alongside syntax errors,
and every refactoring that reads an incomplete file now says which of the two it was.

Also swept the CLI surface after the `--lang` finding, and the other two candidates are
defensible, and are not defects. `impact` calls its walk `--caller-depth` where `callers`
calls it `--depth`, because `impact` also reports references that the depth does not
bound. `--path` exists on `unused` and `duplicates` and nowhere else, which is where it
is needed: those answer whole-workspace questions. Narrowing with `-C` instead gives
a different answer of 30 dead symbols instead of 28, because references from outside the
narrowed root are gone.

### An inverse that did not close

`fr signature` moving a parameter and moving it back should return the file to what it
was. Over 159 sampled functions here, 37 round-tripped, 121 refused. One did not
close: `model::scope_at`, which is a free function with a method of the same name beside
it.

Neither name resolved to itself. The method's four call sites were attributed to the free
function. The free function's one call site to the method, exactly swapped, both
reported `Exact`. Two separate causes. A bare call was allowed to mean a method, because
Rust was missing from the list of languages where a member always has a receiver, on a
stated ground that had stopped being true (B290). And the four `f.scope_at(30)` sit inside
`assert_eq!`, where a macro body is tokens and the receiver is not recorded at all
(B291).

The second fix was wrong the first time in an instructive way: distrusting every token in
every macro fixed the four references and made 12,989 others unrewritable. What
distinguishes them is written in the source even where the syntax is not, the dot.

### What a refusal is hiding

`fr move` over a sample of this repository refused all 64 candidates, which is the kind
of result that looks like caution and is worth reading anyway. Two of the reasons were
about the symbol; the rest named `src/analysis/entrypoints.rs` and a `#[path]` attribute
it does not have. The file documents `#[path::name]` in a doc comment, the check searched
the text. One match anywhere under `src/` refuses every cross-file move in the
workspace (B288).

Reading the attribute from the tree turned 0 possible moves into 11, and the eleven then
exposed the second defect, which no refusal could have. Applying each move to a copy of
the workspace and counting resolved references showed every consumer outside `src/`
losing a few. The import written into `tests/` and `examples/` was `use crate::…`, and
those files are each their own crate (B289).

A refusal is not a safe default when it is wrong about why.

### The output has to be valid input

`fr imports --write` over a clean copy of this repository, then asking what changed.
Idempotence held — 44 files changed on the first pass, none on the second. But three
files came back with an attribute guarding a different import than before. Sorting moves
whole lines, an attribute sits on its own line. Nothing tied the two together, so
`#[cfg(feature = "cli")]` kept its position while the `use` beneath it sorted away
(B287).

Nothing catches this downstream. The edit engine rejects an edit that introduces a parse
error. This one introduces none: the file still parses, it just no longer compiles
under either setting of the feature. The check that would have caught it is the one this
sweep ran, apply the tool to a real tree and ask whether the result still means what it
meant.

### Sweeping a command over its own repository

`fr inline` on every local in this workspace, 9,147 of them, and not on an example.
Two things fell out that no single case would have shown.

It refused 4,940 of them as rebindings. The check asked whether the name appeared again
later in the same file and never asked in which scope. So two functions that each declare
`let s` read as one variable assigned twice. 6,166 of the 9,147 locals share a name
with another local in their file. Scoped, the answer goes to 487 refusals, all of them
real (B284).

And one panicked. `tight_removal_span` read the line before the construct and the line
after it from the same offset, which is only the same line when the construct fits on
one; an HCL local holding a multi-line object asked for `source[end..start]`. The file
was `web/sample/infra/main.tf`, shipped in this repository (B285).

### A language nobody had named

`fr unused` reported a CSS class as dead while a `.js` file two directories away named
it in a string. Not a resolution bug: `.js`, `.mjs`, `.cjs` and `.jsx` mapped to no
language, so those files were never scanned. An unmapped extension looks like a
PNG, so nothing said so.

The grammar was already there. TypeScript is a superset of JavaScript, and the 19
`.js`/`.mjs` files in this repository parse with no errors. Naming the extensions took
one line; the choice worth recording is not adding `Language::JavaScript` beside it.
Twelve `matches!(lang, TypeScript | Tsx)` arms exist across eight files, and each would
have become a place to forget the new variant (B282).

The same sweep found the inverse: `.sass` is named by the table and cannot be parsed,
because Sass's indented syntax is not SCSS (B283). That one stays as it is, the failure
is visible in `fr parse`. Removing the mapping would make those files disappear the
way the `.js` ones did.

### Fragments nobody could resolve

`fr unused` on this repository lists dozens of Markdown headings, which a
workspace looks like when no link resolves to a heading at all. Both query files said
the engine strips the `#` when it resolves a fragment; resolution opens with a verbatim
lookup of the reference name and returns on a miss. `#beta` is nobody's name, so the
branch that strips it had never run. A documented design, written in two places, dead in
the one place that mattered (B281).

The rename was the expensive half: `# Beta` became `# Zeta` and `[jump](#beta)` stayed,
reported as one site changed with no warning. Fixing resolution alone would not have
fixed that, a heading is referenced by its slug. So the rename has to write
`three-big-words` where the heading became `Three Big Words`. The span it writes over
must exclude the `#`.

### SCSS at scale

`twbs/bootstrap`'s stylesheets, the canonical SCSS codebase: **73 of 99 files fail to
parse**. B11 already recorded SCSS grammar gaps from `grafana/grafana`, where they cost 5
of 8 stylesheets. So this is the same limitation measured somewhere it can be measured
properly.

One form is worth masking, and not for the reason the counts suggested. Interpolation in
a declaration value (`color: #{$v}`) co-occurs with 51 of the 73 failures, but masking it
alone fixes 14 files, most of those 51 hit other forms too. So the count measured
co-occurrence and not cost. What makes it the one worth handling is where its error
node goes: not the declaration but the rest of the file, so `_accordion.scss` reported one
error span of 0..5050. Masking it, with the variables and calls inside the braces read
back afterwards, took symbols from 1916 to 2826 and references from 3839 to 6277 with no
file losing a reference (B280).

Masking the other forms was measured and rejected in the same run. They fix 23 more files'
error counts and recover no facts at all, since their errors stay inside the construct.
The sweep also turned up a form the entry never had, a nested rule opening with a
combinator, `.a { > .b { … } }`, 10 files.

The entry also claimed `@content` inside a mixin was among the gaps. It parses, bare,
nested, and with arguments, so the claim was either wrong when written or fixed upstream
since, and nothing re-checked it in between. `tests/known_grammar_gaps.rs` had no SCSS
cases at all, which is how it rotted. It has nine failing forms and nine working ones now,
so a grammar upgrade that fixes one is a test failure pointing at the entry to retire.

### Two commands that have to agree

`fr unused` names candidates and `fr delete` acts on them, so feeding the first to the
second is a check on both. Over `helm/helm`: no refusals, which is the invariant holding,
and 34 of the first 40 candidates could not be passed to `fr delete` at all, because the
name is defined twice and the list had no way to say which one it meant. `--json` carried
no position either, so a script could not construct one. Both renderings say
`file:line:col` now, and 12 of 12 sampled candidates go straight through. `fr entrypoints`
had the same shape.

### Running it on itself again

Two findings. The workspace had one parse error and it was in the published site:
`docs/demo.html` ships two raw `&&` in text, an unterminated entity reference that
browsers recover from. `site_integrity` follows links and checks command names, both of
which pass on a file that does not parse. So it now parses every page with the tool's own
parser.

The second is larger. Rust's container patterns matched `type: (type_identifier)`, and
`impl Ctx<'_>` and `impl<T> Generic<T>` put a `generic_type` there, so the methods inside
had no container. It was recorded as `run` and not `Ctx::run`, with kind `function` and not `method`. A
`self.hcl_backward(…)` then had no member to resolve to, and 43 of `provenance.rs`'s own
methods read as dead code. Internal dead-code findings for this repository go from 92 to
49, and what is left is fields and parameters, with no phantom functions.

### Sweeping the refusals

The Bash run found three defects in what refusals say. None in what they refuse, so
the next pass took that as the question and asked it of every `Refusal::TooWeak`. The
sites divide by what they put in the confidence field: one reporting a real reference
writes `reference.confidence`. Five wrote `Confidence::NameOnly` because there was no
reference to ask. All five say "cannot be known" or "cannot be shown" in their own text
and were then prefixed with "resolution is only 'name-only'".

`TooWeak` now takes a `ResolvedConfidence`, whose field is private to `model` and which
only `Reference::resolved_confidence` produces. The variant cannot be built without a
reference to take a confidence from, checked by trying, which the compiler refuses as a
private constructor. `signature.rs` stopped naming `Confidence` at all.

The same question of `Refusal::Unsupported`, whose shape is
`{operation} is not supported for {language}`, found the reverse problem. With nowhere to
say why, ten of its fifteen sites wrote the reason into the `language` field. One
wrote "a variable is not a flag", which names no language. Adding `because` and typing
`language` as `Language` makes a sentence there a compile error.

### Bash at scale

`nvm`, 5,655 lines across five scripts, parses clean. `fr signature` moved a positional
parameter of `nvm_tree_contains_path` and renumbered the body and all three call sites
correctly, which is the operation with the most shell-specific machinery behind it.

Three defects, all in what the refusals say and none in what they refuse. A signature
change on a function with a twin in another file refused by raising the refusal `rename`
and `extract` use. So it said "renaming would shadow or collide with it" to somebody who
had asked to move a parameter. An argument whose word count the shell decides at run time
refused as "resolution is only 'name-only'", `Refusal::Unknowable` exists for that and
its doc comment names the symptom. So the fix had been written down and this site had not
been changed. And the remedy "quote it to make it one argument" was appended to every one
of those refusals, including `$@`, where quoting gives one word per parameter and the
same problem again.

Commands: `scan`, `parse`, `symbols`, `def`, `refs`, `usages`, `implementations`,
`rename`, `extract`, `inline`, `signature`, `move`, `delete`, `unused`, `duplicates`,
`imports`, `restructure`, `rewrite`, `remove-flag`, `recipe`, `translate`, `callers`,
`callees`, `graph`, `flow`, `impact`, `stitch`, `entrypoints`, `capabilities`, `cache`,
`openapi`, `type`, `completions`.

### The JSON surface an agent scripts against

A probe drove every command as an agent would, `--json` and nothing else. The
gaps it found were of one kind: the machine half of an answer said less than the
human half. B384, B385 and B386 record the three that were defects. The rest
were missing fields, now present and pinned in `tests/json_surface.rs`:

* `fr symbols --json` carries `line` and `col` beside the byte spans.
* `fr callers` and `fr callees` carry `file`, `line` and a `parent` per node, so
  the tree can be rebuilt from the rows.
* `fr flow --json` steps carry `line` and `col`, and the value-flow answer names
  its `model` the way the provenance answer already did.
* `fr unused --json` carries the dynamic-dispatch caveat the human output had.
* `fr openapi --json` carries the "does not settle" notes in the payload.

### The pass where the drafts started to run

Three probes drove the tool the way its users do: one over the mutating
commands, one over `translate` with compilers waiting on the other side, one
over the query surface. Twenty-three findings survived verification. The
refactoring side gained the refusals it owed (B387 through B391), the index
stopped conflating a field with a method (B392) and learned Python's instance
attributes (B398), and the translator crossed the gap between "parses" and
"runs": tuples (B393), class fields and constructors (B394), entrypoints,
field defaults and record returns (B395), the builtin table (B396), properties
(B397), and the everyday Zig forms, with a failed initializer keeping its
binding instead of poisoning the lines after it.

The measure that matters: a Python module of ordinary classes now compiles
under `tsc --strict` after translation and prints the same output under Node
that it printed under CPython, and the same file translated the other way runs
under Python. The corpus ledger fell by roughly 260 carried constructs, and
what still cannot cross says so in the file, at the line where it stops.

### The pass where the CLI grew a spine

Two probes drove the binary the way scripts and agents do, and eight findings
survived verification (B399 through B406). A racing `--write` no longer loses
an edit, and a closed pipe ends a listing quietly. Diffs now apply under
`git apply -p1`, the counts of `fr usages` and `fr rename` reconcile, and the
exit code names the failure. `fr scan` accounts for symlinks, an empty
`restructure` pattern stops a recipe, and `fr remove-flag` names the mentions
it cannot rewrite.

### The pass where the two probes met in the middle

Three more probes: one adversarial against the fixes of the pass before, one
into translation with compilers and runtimes waiting, one for scale and the
agent loop. The refactoring side closed the holes the attribute work had
opened (B407, B408) and the ones dispatch still had (B409, B410), and `fr
move` and `fr inline --call` stopped writing Python that raises on first use
(B411, B412). The edit engine grew the two guarantees concurrent use needs:
a commit verifies its basis and holds a lock, so the race that silently
dropped a rename now refuses with the reason (B399). The rest of the
robustness ledger, B400 through B406, is exit codes an agent can branch on,
diff headers `git apply` accepts, a scan that says what it skipped, and the
figures that disagreed brought into one truth.

On the translation side the corpus ledger kept falling: identifier-named Zig
tests cross with a collision-proof name, `errdefer` cleans up on the failure
path in the languages whose failures are exceptions, and a base class in the
same module lays flat into its extenders where nothing inherits, so the
supertype marker stands only for what is truly out of reach.

### The pass where the drafts stayed running

A second probe drove `translate` with compilers and runtimes on the far side,
and five findings survived (B413 through B417). Markers compile now: Go's
stand-in binds, Rust's `todo!` doubles its braces, and an untranslatable
constant is a comment instead of a build that stops. The entry call crosses
every pairing once. The self-running readers synthesize it, Python guards it,
TypeScript writes it bare, and the targets that run `main` themselves drop it
with a note. Exceptions cross under the target's own names, and a caught
error read as text is its message everywhere. Rust's `Result<T, E>` and Zig's
`E!T` read as one shared name. Go writes it as its `(T, error)` pair; the
exception languages raise the `Err`. A value-position Zig switch lowers to
declare-then-assign, and the Rust writer folds the pair back into a `match`.

The measure again: the Result fixture builds under `go build`, and its
functions answer byte-for-byte what the Rust binary answers. The exception
fixtures run identically in both directions across Python, TypeScript and
Java. The Zig ledger fixture fell from ten rustc errors to five, and every
survivor names a foreign API out loud. Carried error propagation fell from 46
to 20 across the corpus. The Zig reader stopped dropping one-statement
branches in silence, and the ledger now states them.

### The pass where the analyses told the whole truth

A chart value with two values files is one entity now: every command acts on
every layer, and a template read blocks delete. `fr stitch` reads
docker-compose `environment` blocks in both spellings, so compose variables
join chains and orphan detection. Python packaging declares entry points, and
declared console scripts stopped reading as dead code. A call through an
import alias reaches the call graph, and an aliased re-export chain resolves
to its declaration. `fr impact` carries a route's weakest confidence, so a
caller past a dispatch edge lands under needs-review. And `fr flow back`
stopped claiming a `-f` that nobody passed. (B424 through B429.)

### The pass where the values crossed

Twelve passes in, the types of a closed choice crossed in every direction
while every value of one carried. Now the IR holds the variant. Rust paths
and struct expressions, Zig's anonymous `.{ .one = n }`, Python calls of a
consumed class, Go composite literals and TypeScript kind-literal objects
all settle against the module's own sums, and each writer builds the value
the way its language does. The inline TypeScript union became the same sum
as the named form. A path naming anything else, `Vec::new`, an enum from
another crate, goes back to being carried whole. A demoted callee takes its
whole call with it, so no marker ever runs. (B418.)

The receivers learned to carry their evidence. A Python property's getter,
setter, decorator and use sites rename as one attribute. Ambiguity is
counted in entities, so the two doors stopped blocking their own class. A
declared receiver reaches its family through every declared subtype. `var b
= new B()` takes its type from the construction, and `self.count` follows
the class chain across an import. `fr inline --call` refuses a callee that
reads its own file's imports where the destination lacks them, and inlines
when both sides import alike. (B419 through B423.) The UX probe's fifteen
findings landed too. Refusals exit as promised, listings print
workspace-relative paths, and an inverted range is refused with both ends
named. Indexing shows progress on a terminal, and the docs stopped
promising commands that did not exist. (B430 through B434.)

A directory sweep now translates a package instead of a pile of files. Each
file crosses against the merged context of the whole set. One naming table
spells every declaration and use, and imports of siblings become real
imports of their translations. The seam gate holds it. Python to TypeScript
must pass `tsc --strict` and print byte-for-byte what the source printed,
and the reverse must do the same under python3.

### The pass where the tool stopped breaking what worked

A fifth probe asked what the others had not. Does a refactoring hold up when
it is composed, repeated, or reversed? Does it leave working code working? It
built projects whose own tests pass, ran an operation, and ran the tests
again. Seven of its findings wrote broken code to disk and exited
zero.

The worst were not in the refactorings at all. Every write staged a file
beside its target and renamed it over, so the target took the private mode a
temporary file is given. An executable script stopped being executable, and a
repository-wide rename re-permissioned the repository. A first import went in
at byte zero, which is above everything. A shebang moved to line two, and a
module docstring became an expression nobody reads. Both were invisible to
the syntax gate, because both files still parse.

The same shape ran through the rest. A repeated `signature add:` wrote a
parameter list naming one thing twice. The grammar accepts that and the
language refuses it, and every other operation here declines a repeat. A
parameter's name was read from the wrong end of its text, so Go's `price
float64` came back called `float64`.

The reports stopped disagreeing with the facts underneath them. A Kubernetes
`configMapKeyRef` is a reference now, so renaming a ConfigMap key rewrites
the Deployment that reads it. `fr flow` names that consumer instead of
declaring there is none. A Terraform module's outputs and arguments are
references every command reads. So `fr impact` stopped missing what `fr flow`
reports, and `fr delete` refuses an output something still uses.

And the capability matrix stopped disclaiming what the binary does. It denied
`fr openapi` for Python while reading FastAPI routers. It told a reader that a
Terraform variable cannot be traced "because this language has no functions".
The matrix's own claims test then refused the overcorrection, which was the
useful part. Dataflow really does not apply where values are substituted
rather than executed. The row says that now, and points at the provenance row
that answers. (B570 through B610.)

### The pass where a name meant the same thing everywhere

A fourth probe drove the ground the others had left. The markup and config
languages as refactoring targets, the capability matrix against the binary,
and the analysis commands as one story. Its finding was a pattern, not a
list. Edges the fact base already holds were not reaching the commands that
act, and the reports described the gaps in words that read as completeness.

A shell function reached through `source` was the sharpest case. Sourcing a
file runs it, so its functions are callable by their bare names, and nothing
modelled that. `fr usages` said none, `fr unused` listed the function, and
`fr delete` removed it while `bash` still called it. The same honesty went
into the listing that had called a broken Kubernetes reference "a mention in
a comment or a string". It drops what the search already counted, and says
what the rest are.

Names now mean the same thing at both ends of a translation. A sweep renames
what two files both declare, where the target keeps a directory in one
namespace, and says so in the header. An import written inside a function is
lifted to the file's own imports, since every target here hoists them. An
aliased base class joins its family. A leading underscore stopped inverting
its own meaning. The case converter read Python's mark for "not outside this
module" as a word break, and handed Go its mark for exported. A round trip
published a package's internals.

The bodies grew the things a body needs. A field read bare goes through the
receiver, so a translated class compiles. Go's `for` crosses in all three of
its spellings. A field keeps the value it starts at, and a concatenation chain knows it is
a string. A function that returns something names what, even where the source
annotated nothing. Integer division truncates for a field
as it already did for a local.

The refactorings stopped answering with the wrong thing. A Go extract
compiles, with the several values Go returns. A Terraform rename reaches the
module call that names the variable. A signature change refuses where a macro
hides a dispatch site. A restructure matches across comments and reports what
it will not rewrite. A move handles a class that names itself. And removing a
parameter takes the argument that names it, instead of whatever sat in that
position. (B531 through B565.)

### The pass where the answers stopped lying

Four probes drove the tool as a stranger would. One went adversarial against
the last pass, one through compilers and runtimes, one over the operations
least exercised, one as an unattended agent. Two hazards topped the list, and
both produced clean success over a broken workspace. A file skipped for its
size was invisible to every command it could falsify. And `fr imports`
deleted a Python package's public API as unused. Both are answered now.

The machine surface grew the shapes a script needs. Refusals carry their
blocking positions as data, and a recipe that fails its expectation restores
the bytes it started from. The exit codes match the taxonomy the help
documents, and one warning has one shape wherever it comes from. `fr symbols`
emits positions `fr extract` accepts, so a tool can drive the pair without
reading the file itself.

Translation stopped answering confidently in the wrong arithmetic. Python's
`//` and Rust's `div_euclid` disagree for negative divisors, and the draft
ran and printed the wrong number. A class with two bases kept neither while
its body still called `super()`. A default reading another parameter reached
Python verbatim and raised before the module finished importing. Each now
crosses correctly or says what it could not do.

The refactorings learned two refusals they owed. A selection crossing a
loop's body cannot be extracted as a call. It is refused with the boundary
named, instead of writing a file that does not parse. A receiver assigned
twice is not declared by its first initializer, so the call stays for review
and the reason says which binding is unsettled. (B505 through B529.)

### The pass where the sums closed the loop

Construction crossed in pass twelve; consumption crosses now. The IR holds
the variant match, payloads bound to plain locals. TypeScript's kind chains
and switches read into it, and Java's `instanceof` with its cast collapses
into it. Rust's own `match` reads its unit and struct patterns in. Every
writer spells the narrowing natively. Rust matches, Python asks
`isinstance`, Go switches on the type, Zig on the union. Java's sealed
interface finally forms the sum it declares, and its constructions and
narrowings ride the same rails. Around the crossing, the edges hardened.
Two sums sharing a tag settle by the position's declared type, and the
discriminator literal is read instead of derived. A collision dodge is
spelled once and consulted everywhere. A concretely-used struct keeps its
identity beside its variant, and a shadowed member holds its calls back.
Integer literals gain their point where a float signature needs one. (B505
through B512.)

### The pass where the constructs crossed

A third compiler-backed probe found eight gaps, B435 through B442. Asserts
cross now: Python's statement, Rust's `assert!` family and Zig's
`std.debug.assert` read as one check. The targets without an assert test the
condition and throw or panic, so a translated test file can fail again.

One-expression lambdas cross between Python, TypeScript, Rust and Java, and
Go and Zig carry them visibly. Floor division reaches every target through
its own flooring call. An optional TypeScript parameter defaults to `None`
in Python, so its callers stay valid. `super` and the exception bases speak
the target in both directions, and a constructor whose body was the super
call stopped gaining a `raise NotImplementedError`. An annotated instance
field keeps its field and its type. A Go declaration whose initializer
cannot cross still declares its name. A Java record's `implements` clause
carries, and a spelled-out accessor no longer collides with its field.

Two bug classes fell on the way. A field or index access on a compound
receiver takes brackets in every writer, so `(a == b).then(x)` stays one
expression. A property read spells its name from the method namespace, so a
two-word property survives the crossing. The measure: the inventory fixture
crosses to TypeScript, compiles under `tsc --strict`, runs under node, and a
violated assert stops it with a nonzero exit.

### The pass where the seams stopped swallowing things

A probe over the joints between languages found eleven, B600 through B610.
The theme is a seam: a place where one model hands to another and something
fell in the gap without a word.

A Markdown section carried the document's link definitions off with it, so
the links left behind resolved to nothing. A YAML anchor was written with no
alias to spend it, and counted as a replacement. `fr remove-flag` refused the
qualified name `fr symbols` prints, and, once it took it, wrote `Flags.true`
over a use read through its owner. Three `fr signature` refusals printed
under exit 1, the code for a crash.

The rest are answers that read as facts and were not. `fr callers` on SCSS
printed the name and exited 0, which a reader takes for "nothing calls this".
A resolved call at file scope counted among the unresolved. A `data-*` hook
shared by markup and its component was no symbol at all. A link into an id
nothing declares had no report anywhere. Markdown was invisible to the
mention sweep, having neither a string node nor a comment node. So a style
guide naming a CSS class went unlisted through a rename. A chart with no
`Chart.yaml` was read as plain YAML, and `fr stitch` began its chain one hop
in.

### The pass where the tool answered about the project

A shell stands in a subdirectory far more often than at a repository root, and
an agent's shell almost always does. The root defaulted to `.`, so every
command asked from `pkg/deep` answered about `pkg/deep`. `fr usages` reported
no uses of a function `main.py` calls. `fr delete` offered to remove it.
`fr rename` renamed the definition and left the caller reading a name nothing
declares. All three exited zero and reported success, which is the shape of
wrong answer this project exists to remove. The root is now the nearest
enclosing project, and a path typed from where you stand is read from there.

The rest of the pass is the same question asked of the other surfaces. What
did the scan pass over, and did it say so. Which floor is a stylesheet judged
against, when eleven copied declarations come to fewer tokens than one copied
function. And where does a reader go when `.gitignore` excludes the file they
want to work on. Nowhere: no flag reached an ignored file at all.

Translation was checked by compiling what it produced, not by reading it. Go
refused every translated library, because a file with no `func main` is a
program with no entry point. Rust refused every method that wrote a field,
because `&self` cannot be assigned through. Both refused an empty list that
came out `[]any` under a signature promising something else. Java took its
file and answered 5 where the source answered 5.34. Python's `/` and C's `/`
are two operations that share a spelling. Reading both as one made every true
division a truncating one. Java's silence was the worst of the three.

Two things a person needs that were not there. `__init__` is how Python spells
a public constructor. Its underscores were read as the mark for internal, so
no translated class could be built from outside its own file. And nothing
completed anything: thirty-three subcommands, and no shell knew one of them.
### The pass where the edits landed where they belong

A probe over extract and move found six, B660 through B665. The theme is
placement: an edit computed correctly and written into the wrong scope, the
wrong file, or beside the thing it should have replaced.

`fr extract --function` wrote its definition straight after the function it
came from, at column zero. Inside a Python class that puts a `def` in the
middle of the class body. Python parses that, so the reparse guard passed.
The methods below became closures of the new function. Placement is one
choke point now. Hoist out of every enclosing class, stop at the first
enclosing function, and take the indentation of whatever it lands beside.
TypeScript reached the same code with a receiver nobody could see, `this`
being named in no signature. It travels as a parameter now, the way Go's
named receiver already did.

`fr move` in Go left the imports where they were. The destination named an
undefined qualifier, and the source imported a package it no longer used. Both
had been reported and neither done. A Go import path is absolute and a
qualified use is a reference under the package binding, so neither half was
ever a guess. In TypeScript a specifier crossing a directory resolved to
nothing at all, one path join short of normalised. The old import stayed
beside the new one.

The last two are about what a refactoring leaves behind. A move erased a
declaration's lines and left both blank lines that had separated it. A symbol
moved out and back came home to that scar. And `fr inline` was documented as
the reverse of `fr extract` while sharing no case with half of it. The docs,
the help and the refusal say so now.

### The pass where the commands were held to what they promise

A probe drove the CLI the way an agent would and reported what it saw. The
theme is a promise the tool makes and then keeps only in part.

`fr remove-flag` could not run on the commonest Python layout, a flag in its
own module and an import where it is read. The literal went into the import
statement, and the parse gate threw the cascade away. TypeScript wrote the
same nonsense there and survived by accident, because a later round deleted
the statement. An import binds a name and reads nothing, so the choke point
that decides where a literal can stand now says so for every language.

The same command refused a flag it could see being read. `from app import flags`
binds a submodule, and the index read the import path as the whole answer, so the
receiver named the package file. `flags.USE_NEW_TAX` resolved to nothing, and the
refusal said nothing read the flag and pointed at `fr delete`. A receiver bound by
an import can now name the submodule too, and relative module paths resolve. A
refusal with no firm use to work from lists what `fr rename` would show instead.

`fr restructure` called a pattern that matched nothing a success. It printed a line
and exited 0, while `fr rename` exits 3 for a target it cannot find. A caller looping
over rewrites read a typo as "nothing left to do". The command reports not-found now,
in the exit code and in the `--json` error. Its skipped matches were prose on stdout
under `--json` too, in front of the report, so the output was not JSON.

`fr impact` is the reconnaissance this tool suggests before a change, and it left
out what the change itself reports. The name written as text resolves nowhere: an
`__all__` entry, a line of documentation. `fr rename` sweeps for those and lists
them. `fr impact` ran no sweep, so it answered one site where the rename showed
three. It asks `crate::mentions` now, the same sweep the other commands ask.

`fr imports` worked out why it kept each import and printed none of the reasons. A
package `__init__.py` re-export, a `__future__` import, a submodule imported for its
side effects: each one was built as a warning and dropped. The user read "removed 0
import(s)" and had nowhere to go. The single-file report lists them, and `--json`
carries them as `kept_imports`. The workspace sweep prints the count.

A recipe run and its `--explain` gave the same file two lengths. `--explain` counted
the steps in the recipe and the run counted the steps it reached. A run stopped at
the second of three called itself a two-step recipe. The header describes the file
now. How far the run got is a line of its own, and `steps_in_recipe` in the JSON.

### The pass where the tool was turned on itself

Every probe of this pass was an `fr` command run over this repository, and
every fix was retried with the command that had misbehaved. The edits used
`fr` itself where an operation exists for them. The four genuinely dead
functions left this codebase through `fr delete`.

`fr unused` opened the pass by answering 445 lines, 202 of them Markdown
headings. Working the rest of that report down uncovered a chain of
resolution defects, each hiding behind the last. An enum variant matched
seventeen times read as dead, because variants had no qualifier. A field
consumed only by destructuring read as dead, because a pattern was no
reference. A serde-constructed variant read as dead, because a catalog spells
`Remote` as `remote`. A call to `fn stmt` resolved to a sibling function's
`stmt` parameter, because scopes covered only the body block and a parameter
sits before it. When the report finally told the truth, it had shrunk from
445 lines to the handful this pass deleted or wired up.

The worst find was a write. Renaming the local that feeds `Facts { count }`
produced `Facts { total }`, a field the struct does not have. Rust and
TypeScript both had it. The shorthand expands now, in whichever direction the
rename runs. The companion fixes let a field rename reach `f.count` through a
receiver declared `&Facts`. A struct now owns its fields, and a declared type
sheds its sigils.

Two ergonomic gaps a dogfooding agent hits in the first minute. `fr symbols
<file>` was a usage error, and `fr delete` said nothing about the import it
deliberately kept. Both answer properly now. The wasm surface's one dead
method, `declared_type`, turned out to be a playground action nobody wired;
the playground offers "What type is this?" now.

### The pass where the write commands went to work

The previous pass turned the analyses on this repository; this one turned the
writers. Each command ran over real code, the result went to the compiler,
and what the compiler refused became the finding.

`fr signature` refused at the first target it was given, twice over. A path
written inside `assert_eq!` is tokens to the grammar, so
`fun_refactor::model::anchor_slug` resolved at the weakest tier. Even
resolved, the call could not be rewritten. Both halves read the tokens now.
The tokens spell a path and an argument list, and the top-level commas of the
token tree split the arguments exactly.

`fr extract --function` compiled cleanly on its second target and not its
first. `println!("{total} file(s)")` reads `total` through a format capture
no reference records, so the parameter never travelled. A capture is a
read now. `fr move`, sent on a round trip between two modules, failed one way per
direction. Out, a written `crate::…` path kept naming the module the symbol
had left. Back, the move carried a `use` the destination already bound in a
brace group. The round trip compiles both ways.

`fr rewrite invert-if`, applied twice to a real branch, returned the file
byte-for-byte, which is the property a rewrite pair owes.

### The pass where the tool got fast enough to use

`fr recipe` over this repository was the probe that mattered: a two-step
recipe never finished. Under it sat two compounding costs. The engine rebuilt
the whole index from scratch after every step. One such build took two and a
half minutes, most of it in resolution walking the workspace per candidate. `definition_group` scanned every symbol; the dotted-import rule
scanned every file key; `names_a_type` scanned every symbol again. All of it
goes through by-name buckets now, and extraction is cached by content within
a run. The same recipe finishes in the time its steps take.

The rest of the pass was the writers again, smaller. `fr rewrite
guard-clause`, pointed at a real branch of `values_paths`, negated the
first atom of `!a && !b` alone. That silent wrong
answer let duplicates through; both directions are pinned now. An
inlined multi-line binding left its indentation behind as a line of trailing
whitespace. A TypeScript rename round-tripped byte-clean under `tsc`, and
`fr restructure` matched nineteen real occurrences across eleven files.

### The pass where a warm command stopped costing seventeen seconds

Two lies about cost, one in a comment and one in an architecture. The
parallel build's comment said query compilation was paid once per thread.
The code compiled the whole set once per file, hundreds of times a build. A
thread-local made the comment true and took a cold index from fifty seconds
to twenty-four, on top of the last pass's three-fold gain.

The architectural one: every warm command re-resolved the workspace, because
resolution ran on every index build however fresh the facts were. Resolution
is a pure function of the merged facts, so it is a cache entry now, keyed by
every file's path, language and content hash. An agent running ten commands
against an untouched workspace paid seventeen seconds ten times. It pays a
fifth of a second now; the first command after an edit resolves afresh.

### The pass where the tutorial practiced what it preached

The type-safety page called hand-rolled monads friction and said only
`Result` earns its keep. It then walked readers through a Writer and an IO
anyway. Those went. So did a literal-flag block that repeated the status
lesson, and an alias block that repeated its own section. Twelve examples
went, twenty-four files, each with its before and its misuse. The gate
in `tests/typesafety.rs` chased out every orphan the removals left.

The opening example now carries no annotations at all. It runs, and it
gives wrong answers. Both checkers refuse to reason about it until someone
annotates, which becomes the tutorial's first step. The
prose swapped its passive constructions for sentences with subjects, and the
concepts table lost the row for a lesson the page no longer teaches.
