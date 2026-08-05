# fun-refactor — Plan

Multi-language refactoring + code-intelligence CLI on tree-sitter, covering the funveil
language suite. Research and provenance for every design choice: see [RESEARCH.md](RESEARCH.md).

- **Crate**: `fun-refactor`, binary `fr` (provisional). Rust 2021, **AGPL-3.0-or-later**.
- **Repo**: `github.com/e6qu/fun-refactor`. Commits authored as `e6qu
  <2966430+e6qu@users.noreply.github.com>`; remote uses the `github.com-e6qu` SSH alias.
- **Languages** (12): Rust, Go, Zig, TypeScript/TSX, Python, Bash, HTML, CSS/SCSS,
  Terraform/HCL, Helm/YAML, XML, Markdown. Grammar pins inherited from funveil
  (tree-sitter 0.26 line).
- **Feature families**: standard refactors (rename, extract/inline, move, change signature,
  safe delete, organize imports) + analysis (symbols/refs, call graphs, entrypoints,
  forward/backward flow, config-value provenance).

## Design decisions (baked in)

| # | Decision | Rationale (RESEARCH.md ref) |
|---|---|---|
| D1 | Self-contained binary; no LSP dependency in the core. LSP delegation is a late optional backend. | §3, §6.4 — the unique value is where LSPs are weak; LSP drags in daemons/config discovery |
| D2 | Edits are byte-range splices on original source, applied descending by offset, validated by reparse + no-ERROR-node assertion. Never pretty-print. | §3 — formatting/comment preservation for free; beats gopls's known comment loss |
| D3 | One unified property graph, shared nodes, independent edge layers (`REF`, `IMPORTS`, `CALLS`, `DFLOW`, `PROVENANCE`), built incrementally per language. | §6.3 — Joern CPG model; queries degrade gracefully |
| D4 | Every resolved edge carries a confidence tag: `exact` / `import-qualified` / `field-based` / `name-only`, plus candidate counts on multi-candidate edges. | §6.4 — characterized imprecision is what makes heuristic systems trustworthy |
| D5 | At unresolved call edges, flow queries stop and downgrade loudly — no silent over-approximation. Summaries (stdlib/framework) can extend reach explicitly. | §6.2 — dev-tool honesty over scanner-style over-tainting |
| D6 | Config languages get provenance semantics (substitution/override chains, hop chains preserved immutably), not imperative dataflow. | §6.2 — deterministic evaluation models; fix Checkov's substitute-in-place flaw |
| D7 | Entrypoint detection is data (per-framework YAML catalogs, MaD-style schema), not hardcoded heuristics. | §6.5 — CodeQL MaD + OWASP noir precedent |
| D8 | Unsupported operation × language combinations are refused with an explicit error naming the gap. No silent no-ops, no silent fallbacks. | engineering principle; also user convention |
| D9 | Every command has `--json` output; mutations default to dry-run unified diff, `--write` to apply, multi-file apply is atomic (all-or-nothing). | CLI-native + agent-friendly |
| D10 | Do not build on stack-graphs (archived 2025-09). Scope resolution via our own locals-style queries; graph construction may use tree-sitter-graph if the DSL earns its keep. | §3 |

**Open decisions.** One remains: whether to add the optional LSP delegation backend
(Stage 8). It is the only route to type-correct method resolution in Rust, Go,
TypeScript and Python, and it costs server lifecycle management, per-language project
discovery, version skew, and the self-contained-binary property the tool has today.
Recommendation on file: skip it.

Resolved since: the tool is `fun-refactor`, binary `fr`; extract-function landed for
both Zig and Bash without needing a CFG; and TSX `className` handles plain attribute
values but not helper calls or template literals — recorded as BUGS.md B14 rather than
left as an open question, because it is a gap with known behaviour, not a decision.

## Language tiers

- **Tier A** — imperative, ecosystem-rich: Rust, Go, TypeScript/TSX, Python. Full refactor +
  flow surface. LSPs exist (future differential-test oracles and optional backend).
- **Tier B** — imperative, tooling-desert: Zig, Bash. Same feature shape as Tier A where
  syntax allows; instant best-in-class (no zls/bash-ls call hierarchy exists at all).
- **Tier C** — config/markup, string-keyed semantics: Terraform/HCL, Helm/YAML, CSS/SCSS,
  HTML, XML, Markdown. Rename/provenance/safe-delete are the stars; several features are
  structurally n/a and refused per D8.

## Reuse from funveil

Same author, compatible licensing — copy liberally, adapt aggressively. Every copied module
records provenance (source repo + pinned commit) in the importing commit message. Mechanism:
at Stage 0, clone `github.com/e6qu/funveil` at a pinned commit into a scratch checkout and
copy modules in; never depend on funveil as a crate (it's a binary crate and we diverge
immediately).

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

### Stage 0 — Substrate: parse + edit engine — **DONE**

**Goal**: parse all 12 languages; make and validate lossless multi-file edits; CLI skeleton.

Landed: `src/span.rs` (byte-native `Span` + `LineIndex`), `src/lang.rs` (15 language
variants — TS/TSX, CSS/SCSS and YAML/Helm split apart because they need different
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

### Stage 1 — Graph tier-0: symbols, scopes, references, imports — **DONE**

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

### Stage 2 — Rename (first mutation) — **DONE**

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

### Stage 3 — Call graph + entrypoints — **DONE**

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

### Stage 4 — Flow analysis — **DONE**

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

### Stage 5 — Extract & inline — **DONE**

**Goal**: the extract/inline family, powered by Stage 4 dataflow.

- Extract variable: Tier A + Zig (expression boundary, insertion point, side-effect warning,
  name suggestion). Config analogues: SCSS `$var`/custom property from repeated value; HCL
  `locals` entry; YAML anchor from repeated node; Markdown reference-link def.
- Inline variable: single-assignment check via `DFLOW`; shadowing check at each use site.
  Config analogues: inline `local.x` / anchor / `$var` / reference link.
- Extract function: Rust, Go, TS, Python (ins→params, outs→returns, control-flow exit
  analysis; comments inside the region move intact — explicitly beat gopls here). Zig/Bash:
  decide per open-decisions.
- Inline call: strict preconditions (single return, no shadowing collisions, effect-order
  preserved); refuse loudly otherwise per D8. Helm: extract named template to `_helpers.tpl`.
- Commands: `fr extract var|fn <range>`, `fr inline <pos>`.

**Exit**: property tests — result reparses clean, extract→inline round-trips to semantic
no-op on fixtures; behavior deltas vs rust-analyzer/gopls documented.

### Stage 6 — Move, change signature, safe delete, organize imports — **DONE**

- Move symbol/section to file with reference updates: Rust (module → file), Go (same-package
  split), TS (move-to-file + import rewrite), Python (symbol move + import updates),
  Terraform (resources between `.tf` files — flat namespace), CSS (rules between partials),
  Markdown (section → new file + link updates).
- Change signature (CLI-native; LSP has no equivalent): add/remove/reorder/rename params with
  all call sites updated — Tier A + Zig; Terraform module variables (add-with-default /
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

### Stage 7 — Cross-language intelligence — **DONE**

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

### Stage 8 — Advanced & ecosystem — **PARTIAL**: pattern restructuring, micro-rewrites and cascading cleanup all complete; the optional LSP delegation backend is the only item left

- Micro-rewrite tail (per-language `refactor.rewrite.*` equivalents: invert-if, guard
  clauses, de Morgan, fill-struct where syntax allows).
- Pattern restructure: user-supplied before/after patterns with scope-aware constraints
  (rope-restructure / ast-grep-style), plus Piranha-style cascading cleanup chains.
- Optional LSP delegation backend (`--engine lsp`) for Rust/Go/TS/Python: prepareRename →
  rename → WorkspaceEdit apply, capability probing, LSP diagnostics as post-edit check.
- Daemon/watch mode with incremental reindexing; editor integration surface.

**Exit**: scoped when reached; each item ships behind its own corpus.

## End-state feature × language matrix

Cell = stage where the feature lands for that language; — = structurally n/a (refused per D8).

| Feature | Rust | Go | TS/TSX | Py | Zig | Bash | HCL | Helm/YAML | CSS/SCSS | HTML | XML | MD |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Symbols/refs/def | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| Rename | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| Call graph | 3 | 3 | 3 | 3 | 3 | 3 | — | — | — | — | — | — |
| Entrypoints | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | — | 3 | 3 | 3 |
| Flow back/fwd | 4 | 4 | 4 | 4 | 4 | 4 | — | — | — | — | — | — |
| Provenance | — | — | — | — | — | — | 4 | 4 | 4 | 4 | 4 | 4 |
| Extract variable | 5 | 5 | 5 | 5 | 5 | — | 5 | 5 | 5 | — | — | 5 |
| Inline variable | 5 | 5 | 5 | 5 | 5 | — | 5 | 5 | 5 | — | — | 5 |
| Extract function | 5 | 5 | 5 | 5 | tbd | tbd | — | 5* | — | — | — | — |
| Inline call | 5 | 5 | 5 | 5 | tbd | — | — | — | — | — | — | — |
| Move to file | 6 | 6 | 6 | 6 | — | — | 6 | — | 6 | — | — | 6 |
| Change signature | 6 | 6 | 6 | 6 | 6 | — | 6† | — | 6‡ | — | — | — |
| Safe delete | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | — | 6 | 6 |
| Organize imports | 6 | 6 | 6 | 6 | 6 | — | — | — | 6 | — | — | — |
| Cross-lang impact | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 |
| Micro-rewrites | 8 | 8 | 8 | 8 | 8 | 8 | — | — | — | — | — | — |
| Pattern restructure | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 |

\* Helm: extract named template. † Terraform: module variable add/remove/rename propagated to
call sites. ‡ SCSS: mixin parameters.

## Testing & quality strategy

Four layers, each answering a question the one below it cannot:

| Layer | Where | What it can catch |
|---|---|---|
| Unit | `#[cfg(test)]` beside the code | Local correctness: span arithmetic, negation, subtree hashing |
| Integration | `tests/*.rs` against the library | A refactoring's resulting bytes, per language |
| End-to-end | `tests/cli.rs`, `tests/test_pyramid.rs` | Argument parsing, path resolution, exit codes, the text a person reads |
| Real repositories | helm/helm, grafana/grafana, by hand | What people actually write, which is not what fixtures imagine |

The end-to-end layer exists because two bugs were found living in it, both of the
kind that answers wrongly while looking like it worked: `--path` filters built by
joining the default root `.` matched nothing and reported that as nothing found, and
target paths were read from the shell's directory rather than the workspace `-C`
names. Neither was visible from the library API.

`tests/test_pyramid.rs` enforces the layer rather than merely occupying it: it reads
the subcommand list out of `fr --help` and fails if any command has no end-to-end
test. That guard is verified to bite — removing a command's entry fails the build
with its name. It also asserts that no command writes to the workspace without
`--write`, which is the promise the whole CLI rests on.

The fourth layer is deliberately not automated. Pinning a 500 MB clone into CI buys
less than the measurements already recorded in BUGS.md, and the bugs it found were
found by *reading* output — a silent guard-clause that moved code out from under its
condition, a dead-code report that was 84% false positives — which no assertion
written in advance would have looked for.


- **Fixture corpora**: per language × per feature, including adversarial cases (shadowing,
  aliased imports, same-name symbols across files, dynamic dispatch).
- **Property tests**: every applied edit set reparses with no new ERROR nodes; bytes outside
  edited ranges unchanged; rename A→B→A round-trips byte-exact; extract→inline round-trips.
- **Differential oracles** (test-time only, not runtime deps): gopls `rename`,
  rust-analyzer, rope, tsserver on shared fixtures; `terraform console` / `helm template`
  for provenance answers.
- **Honesty gates**: no edge without a confidence tag; no command silently succeeding with
  partial coverage — partial results must say what was skipped and why (D5/D8).
- CI from Stage 0; coverage tracked; mutation testing once the edit engine stabilizes
  (funveil precedent).

## Risks

- **Grammar quality variance**: shipped tree-sitter grammars differ in fidelity (esp. Zig,
  HCL forks, markdown). Mitigation: pin versions, fixture-corpus gate per grammar upgrade.
- **Scope-query authoring cost**: own locals-style queries per language is real per-language
  work (stack-graphs' lesson). Mitigation: only imperative languages need true scope trees;
  Tier C binders are string-keyed.
- **Method/dispatch rename correctness ceiling** without types: bounded by D4/D8 (refuse or
  present candidates rather than guess).
- **Helm templating is text-level YAML**: `{{ }}` breaks YAML parsing. Mitigation: parse
  templates with the funveil approach (tree-sitter YAML + template-token layer); treat
  render-dependent structures as unresolved, loudly.
- **Scope creep across 12 × 17 features**: the matrix's — cells are commitments to refuse,
  not gaps to fill; tbd cells resolve via open decisions, not silent drift.

## Progress log

Every stage is complete except the optional LSP delegation backend, and every
capability a language can meaningfully support is built: **263 of 368 capability ×
language pairs supported, 105 not applicable, none refused.**

The matrix is no longer maintained by hand. `src/capabilities.rs` computes it by
asking each refactoring's own predicate, `fr capabilities` prints it with the reason
attached to every non-supported cell, and a test asserts the README matches. That
exists because the hand-written version drifted twice — once hiding 27 unbuilt cells,
once publishing six working ones as refused.

Open limitations are in BUGS.md. All five are characterised rather than silent, and
none is a missing feature: reachability under dynamic dispatch (inherent), Helm
values passed on a command line (invisible to a workspace scan), CSS classes named
inside TSX helper calls (a per-library convention, measured), SCSS forms the grammar
does not cover (upstream), and `tree-sitter-go` parsing a user-defined `new` as the
builtin (upstream).

### What running it on real code found

The fixture corpora test what we thought to write down. Running the tool over
grafana/grafana (16,525 files, 10.9s) and helm/helm found ten defects that 1,200
tests did not, and two of them changed program meaning while parsing cleanly — the
class no reparse check can catch:

- `guard-clause` hoisted code out from under the `if` guarding it in Go. **1,258 of
  1,498 applications over 250 files of `pkg/services` were wrong** (84%); the cause
  was a grammar wrapper node counted as a statement, in a predicate that had been
  copied into two modules and drifted.
- de Morgan dropped the brackets its result needed, turning `x && !(a && b)` into
  `x && !a || !b`.

The rest were loud but real: micro-rewrites published for seven languages and working
in three, `extract --function` emitting `x: : number`, and moves producing files that
parse and do not compile. BUGS.md B16–B25 records each with its measurement.

The lesson is recorded here because it should shape what gets tested next: a
capability matrix derived from code predicates proves a refactoring is *reachable*
for a language, not that it *works* there. The rewrites and extract-function now have
per-language tests that apply the operation and reparse the result, which is what
caught these once the shape was known. Extending that pattern to the remaining
operations is the highest-value work left, and running against a second large
codebase in a different language mix is how the next ten will be found.

### What the playground found

Compiling the same analysis to WebAssembly and driving every capability from a
browser found five more, and the shape of them is worth recording: **none was a bug
in an analysis.** Each was a place where the browser and the terminal disagreed
because a second caller had been written and had made a different choice.

- `find_unused` takes its roots as a slice, and the browser passed `&[]` where the
  terminal passed a detected catalog. Nothing runs, so everything unexported is dead:
  twenty extra findings, including every `#[test]`. The roots are a type now, and an
  empty one has to be asked for by name.
- Every *read* went through the virtual filesystem; the six `Path::exists` calls did
  not, and there is no filesystem in a browser, so each quietly answered false.
- The virtual filesystem itself was one global map, so a second workspace silently
  became the first one's bytes.

Two of the three are now checked by a test that reads the source, because neither is
expressible in the type system and both had already happened once. The third is a
newtype. The general lesson: a second frontend is a good bug-finder precisely because
it re-makes every decision the first one made implicitly — and the decisions that
were never written down are the ones it gets wrong.

The browser API has no `cargo test` — `src/wasm.rs` compiles only for `wasm32` — so
`web/test/api.mjs` drives all twenty-nine of its methods against a bundled
fifteen-language workspace, in Node, using the wasm the site ships. CI runs it, and
`web/test/patch.mjs` beside it lays the sample down as a git repository and asks
`git apply` to take what the download button produces.

A second technique found five more, and it generalises: **run two halves of the tool
against each other.** `fr unused` reports what can be deleted and `fr delete` removes
it, so every symbol the first names the second must accept. Over one Rust function
that invariant held; over a nine-language workspace it failed thirteen times out of
fifty-nine. Nine of those were not dead at all — a CSS class declared in two
stylesheets and used by the markup, counted per declaration site — and four were
deletable in principle but not in practice, because the span the index keeps is the
one a rename rewrites, not the one a delete can remove.

Neither is a bug either half could find alone: each was internally consistent. The
pairs worth checking this way are the ones where one command's output is another's
input — `unused`/`delete`, `rewrites_at`/`rewrite` (which found the workspace
isolation bug), `symbols`/`def`, `duplicates`/`extract`.

### Translating into a framework, not only a language

A Next.js API route is the case that proves the point of reading the *path*: the URL a
route serves is where its file sits on disk, and nothing inside the file says so. A
content-only translation cannot produce `@router.get("/users/{id}")` from
`app/api/users/[id]/route.ts` no matter how well it reads TypeScript.

Building it found the same class of defect twice, from opposite directions, and both
were failures of nerve rather than of technique:

- Things treated as untranslatable that **correspond exactly**. `NextRequest` is
  Starlette's `Request`; `await` is `await`; `return NextResponse.json(x)` is
  `return x`. Each had been carried into the output as a comment, which reads as
  modesty and is actually a worse answer than the obvious one.
- Things treated as untranslatable that are **redundant**. `const id =
  context.params.id` is precisely the work FastAPI already did. Carrying it opened
  every handler with a line naming an object Python does not have.

The discipline that surfaced both: **write the target file, then parse it with the
target's own grammar and read it as a person would.** `transpile::plan` already
refused output that would not parse; the FastAPI writer now does the same. What
parsing cannot catch — a comment where working code belonged — showed up the moment
the output was read against the promise the module documents, which is that the result
is a file you finish rather than one you have to diff against the original.

### A fixture is written by whoever writes the assertion

Thirteen hundred tests passed while `def create_user(*, session, user_create)` produced
`export function createUser(*: unknown, …)` — a file TypeScript will not parse. They
passed because every input was written by the same person as the expectation, and that
person did not think of keyword-only parameters.

`tests/corpus.rs` runs the translations over two MIT-licensed projects vendored
unmodified and pinned. What it asserts is deliberately *not* "the output equals this
string", which would freeze today's translation and break on every improvement, but the
three properties any translation must have: it parses as what it claims to be, it
adopts the target's conventions, and nothing goes missing quietly.

Eight defects in one sitting, two of them silent wrong answers — `session?.user.id` as
`None.id`, and `params.postId as string` as `None`. Both were code that compiles, runs,
and does the wrong thing, with the original nowhere in the file. Neither is reachable
from a fixture nobody thought to write.

Two habits generalise from it:

1. **Vendor the corpus, pin it, and check the properties rather than the bytes.** A
   golden-output test over real code would fail on every improvement and be deleted
   within a week.
2. **Make the exhaustive check exhaustive.** `has_unsupported_expr` had a `_ => false`
   arm, so the three expression kinds added after it were quietly exempt — and each one
   produced a silent wrong answer rather than a gap. It has no `_` arm now, and neither
   does the writers' statement match: a new variant has to be decided about.

### Code no build compiles is code no check checks

`src/wasm.rs` is the playground's entire public surface and, until now, nothing
compiled it on a development machine: the in-memory backing it needs was gated on
`target_arch = "wasm32"`, and the grammars are C that wants a clang with the
WebAssembly backend, which Apple's is built without. `cargo test` and
`cargo clippy -D warnings` both passed over a file with a missing struct field, and CI
found it after a push.

Two gates were wrong rather than two things being hard. The in-memory backing has
nothing wasm-specific in it and now follows the `wasm` *feature*; the libc shim is the
only genuinely wasm32 part and is the only thing still gated on the target. With that,
`cargo check --features wasm` works, `Workspace::load` takes plain Rust values so the
API is reachable from `cargo test`, and CI runs both feature sets.

Compiling the two feature sets *together* immediately found a third defect nobody had
seen: `commit` chose filesystem staging with `#[cfg(feature = "cli")]` rather than
asking where the writes were going, so a build with both would stage a temporary file
beside a path that only exists in a browser's memory. A feature flag says which
backings exist. It does not say which one is active, and code that confuses the two is
correct only for as long as nobody builds the combination.

### Writing the demo is a test of the tool

`docs/catalog.html` works the named moves from Fowler's and Beck's catalogues in
Python, and `docs/translate.html` writes typed Python as TypeScript and back. Neither
page contains a hand-written result: `tests/site_data.rs` runs the real binary over the
samples, captures before, after, the diff and what it printed, and fails when the
committed pages stop matching. A hand-typed "after" is a claim about the tool rather
than a demonstration of it, and it is stale the first day the tool improves.

Building the two pages found four defects, and the shape of them is worth noticing:
none was reachable by asking the tool to do something unusual. They were all found by
asking it to do the *most ordinary thing in the catalogue*.

- `fr signature X 'add:1:flag: bool:false'` — the example printed in the tool's own
  error message — failed with that same message.
- Inlining a variable was refused whenever any name in its value appeared anywhere
  else in the file, because the capture check was a two-hundred-byte proximity window
  rather than a question about scope. The scope-aware answer was being computed and
  thrown away on the line above.
- A bare `xs.filter(p)` did not translate at all, and a comprehension that kept what it
  selected wrote out an identity `map`.

The page also has to say where the tool stops, and one entry is a *refusal*: Fowler's
own Guard Clauses example is an `if`/`else` nest, and turning the `else` into an early
return means deciding what the function returns on the path that used to fall through.
The generator asserts that entry still fails, so the page cannot go on claiming a
refusal that no longer happens.

### Java, and what a language costs

Sixteen languages now. Adding Java was one query file, five lines of enum, and three
`match` arms the compiler demanded — which is the architecture working: "adding a
language means writing queries, not Rust" is a claim `queries/README.md` makes and this
was the test of it.

The three arms are the interesting part, because each was a decision rather than a
default. `move` **refuses** for Java and says why: a public type must live in a file
named after it and imports name packages rather than paths, so moving one is a rename
of the file and its package, not a move of a definition. Doing half of that would leave
a tree that does not compile.

Writing the queries surfaced a defect in the *shared* extractor. Java says
"externally visible" with a keyword rather than with capitalisation, and `modifiers` is
a positional child rather than a field — so `!modifiers` is unavailable and the three
cases have to be made mutually exclusive by hand, because nothing downstream
de-duplicates definitions. And `receiver_of` decided the receiver positionally, which
holds for Go and not for Java: every method call in the language had no receiver at all
until it started preferring the field the grammar names.

### The recipe language, and the choke point that was not one

`fr recipe` runs a refactoring written down. The design in RECIPES.md survived contact
almost intact; what it could not have predicted is that the runner immediately failed
with `edit at 1..301 extends past end of file (226 bytes)`.

The cause is worth keeping. The runner holds the workspace in memory and rebuilds the
index between steps — but the *refactorings* read source through `crate::vfs`, which
means the disk. A plan made after one step was measured against the text before any
step ran. `vfs` is documented as the single choke point for reading source and it is;
what it was not was *switchable* on an ordinary build. The in-memory backing was gated
to the browser although nothing in it is wasm-specific. It is compiled everywhere now.

A choke point that only one caller can redirect is a choke point with one user. The
second user found that out in an afternoon.

### Writing the catalogue page down found the citations wrong

Twenty-three entries now, and two of the original fifteen were citing the wrong thing.
The De Morgan entry was titled *Consolidate Conditional Expression* and cited Fowler
§10.2 — which is about combining several conditionals that produce the same result, not
about pushing a negation through a conjunction. Substitute Algorithm was cited to a
first-edition section number rather than §7.9 of the second. Both now say what they
are, and two entries admit to citing **neither** catalogue: renaming a CSS class across
a stylesheet and its markup is a problem the books predate, and dropping unused imports
is housekeeping nobody wrote an entry for. Attaching a plausible section number to
either would have been worse than the admission.

The page also gained a third refusal, and finding it was the point: `fr signature circ
move:0:1` produced `def circ(units="m", r):`, which Python rejects outright. The engine
reparses every edit, and tree-sitter accepts that line — so a grammar-level check was
never going to catch it and the refactoring had to learn the rule. A demo that only
shows successes would not have gone looking.

### The recipe language met a real repository

Four defects in the first run against helm, and none of them was reachable from a
two-file fixture:

- `rewrite` reported a file it had nothing to do in as a *refusal*, and `on-refusal
  stop` is the default — so the run abandoned itself on the first ordinary file. The
  selector chooses files; a file with no wrapping `if` needed no work.
- Applying a micro-rewrite asked at every byte offset, and each ask reparses.
- A step rebuilt the whole index after every subject. Correct, and two minutes forty
  over five files.
- Both workspace analyses ran, twice, for a recipe that asked for neither.

Together: 2m40 to 48 seconds, with the same output. Three of the four are the same
mistake in different clothes — **doing per-item what only needs doing per-change**. A
fixture with two files cannot tell the difference between O(1) and O(n) re-indexes, and
the design document that predicted "each step sees the workspace as the previous step
left it, which means re-indexing between steps" said *between steps* and got
implemented as between subjects.

### API contracts

`API_CONTRACTS.md`, and a section on the docs page. The idea is a third invariant
alongside the two this tool already had: a refactoring preserves behaviour, a
translation preserves a signature, and a rewrite of a service preserves the **contract**
— the URLs, methods, path parameters, schemas and status codes, while the language, the
framework and every function signature are free to change.

Writing it produced one finding worth the document on its own. FastAPI builds its
OpenAPI from the *decorator*, so a status on a returned `Response` changes what an
endpoint does without changing what it says it does. A translated handler returns `204`
and behaves correctly while its published contract says `200` — behaviour-preserving
and contract-shrinking at once, with every test passing. The tool now lists every status
it saw and says what will happen if they stay where they are. It does not hoist them,
because which status is the *success* one is a judgement about the endpoint rather than
a fact about the syntax.

### Closing the three things that were written down as not done

**The four missing recipe predicates.** `calls=` and `called-by=` come from one call
graph, `implements=` from the hierarchy, `matches=` from the pattern matcher. Each was
an existing analysis rather than new machinery, which is what the design promised, and
each is run only when a predicate asks for it — building a call graph over helm for a
selector that says `name="x"` is the mistake the last round was about.

`matches=` needs `lang=` beside it and says so. The same text parses into a different
tree in every language, so there is no language-free answer to where a shape occurs.
Exposing it also needed `restructure::locate` — the find half of `apply`, which could
not be reused as it stood because `apply` skips a rewrite that changes nothing, and a
pattern used as its own template changes nothing every time.

**Reading zod.** The IR already held the whole builder chain, so this was a walk rather
than a parse: a chain is left-nested, and walking to the base call with the modifiers
collected on the way past gives the type. The constraints are deliberately dropped —
`.min(3)` is validation, Pydantic spells it `Field(min_length=3)`, and the two are not
the same rule in every case.

**`fr openapi`.** The baseline half of a contract check. Paths, methods and path
parameters are exact because they come from the tree; schemas are as good as what was
declared; responses are `default` only, because which status an endpoint returns is a
fact about its code rather than its declaration. Writing `200` for everything would put
fiction into the file you are about to diff against — the diff comes out clean and the
contract still shrank, which is the exact failure the document exists to prevent.

### Reading the documentation with fresh eyes

Adding a language broke six statements the *tool itself* makes. The capability table's
fallback reasons were written when every unsupported language was markup or
configuration, so they explain the absence in those terms — and `extract variable` went
on to tell a reader that Java "has no binding form: a reusable value here is a CSS
custom property". The table's whole value is that its empty cells explain themselves,
which makes a false explanation worse than a blank.

Two cells were worse than wrong: they disagreed with the tool. `move to file` said Java
was supported while the operation refused it, because `supports_move` was a blocklist
and the refusal was a match arm elsewhere. `inline --call` was derived from the
language's *class* rather than from the operation's predicate — the one cell in the
table that was, so adding a language to the enum claimed a capability nobody had
written. Both now ask a single authority, which is what the module already claimed of
itself: "every arm either calls the predicate the refactoring itself uses, or states why
the operation is meaningless".

Four of the six turned out to be capabilities Java simply *had* once asked properly, and
two more followed from fixing them: an annotation inside a declaration rather than above
it, and an unqualified call to a method of the enclosing class.

The prose had the ordinary rot — counts that had moved, a command list three commands
behind — and one entry worth naming: the README carried two "known limitations"
paragraphs that contradicted each other, and the second described a bug that had been
fixed. A document that disagrees with itself has stopped being read by its author.

### A sweep is not a review

The documentation pass before this one was a sweep: find the stale number, replace it.
It introduced an error and missed three of the same kind, and re-reading the *live*
pages is what turned them up.

`16,525 Grafana files across 13 languages` counts what Grafana contains, not what this
tool supports. Replacing "13 languages" changed it on two pages; the mistake was caught
on one and not the other, and the two pages then disagreed — which is how it announced
itself. The lesson is not "be careful with sed". It is that a claim about a measurement
and a claim about a capability look identical to a text search, and only the second is
safe to update.

The sweep also had a blind spot with an edge: it covered `docs/*.html` and not
`web/src/`, which is the half of the site that is *compiled* rather than served. The
playground told every visitor it had fifteen languages while sitting on sixteen.

The one worth keeping is `web/test/scale.mjs`. It claimed to probe every language in
the bundled sample and did not include `.java` in the extensions it walks, so the
newest language was invisible to the sweep that exists to catch exactly that. The claim
lived in a comment, and a comment cannot fail.

Turning it into an assertion immediately found that the gap was older and larger than
the one it was written for: `html` and `scss` had never been probed either. They hold
one definition each, and a stride across five hundred targets steps straight over them
— so the sweep had been covering fourteen of sixteen languages for as long as there had
been sixteen. Raising the probe count until they appeared would have fixed today and
broken on the next language, so the sampling takes one probe per language first and
strides for the rest. The coverage is a property of the algorithm now, and the
assertion is what would say so if it ever stopped being one.

### The tool's claims about itself are documentation too

Five defects, and every one was the tool saying something untrue about the tool.
`fr translate` refused a Java file with "nothing here can do it, so nothing here
pretends to" — written before the transpiler existed and false from the day it landed.
`fr imports` said Bash has no import statements while `queries/bash/facts.scm` extracts
every `source`. The capability matrix omitted `fr translate` and `fr openapi` entirely,
which is a claim of completeness that was not true.

The pattern under three of the five is the same one that produced the last round:
**two places deciding one thing.** The operation kept its reason and the table kept its
own, and they drifted apart in the dark. `why_not_move`, `why_not_organizable` and
`Change::parse` are all the same correction — the operation is the authority and the
table asks it.

Two tests now hold the line, and the second one earned its place immediately by
finding a sixth: a reason must not name a language other than the one it is given for,
and `entry points` was telling YAML it was a stylesheet.

Fixing the last "not yet" cell was worth more than the cell. `fr remove-flag` refused
every Java flag because `SymbolKind::Field` is a struct member in Go and Rust and never
a flag — but Java has no top level below the type, so a `static final` field *is* the
idiomatic flag. Then `if (true)` would not collapse, because Java names an `if`'s
condition as the parenthesised expression and the literal arrived as `(true)`. Both are
the same shape of mistake: a rule that is true of the languages it was written against,
applied to one that arrived later.

### Java translation, and the language with no top level

The fifth language in the transpiler, and the first that made the *writer* do something
structural. Every other target takes a module's items and writes them out. Java has no
top level below the type: a function must be inside a class, and a public class must be
named after its file — a rule the compiler enforces rather than a convention. So a
module becomes a class, `sensors.py` becomes `Sensors.java`, and when a module has both
loose functions and a record, the record becomes a package-private sibling with a
comment saying why. That is the first time a target's *file naming* has been part of a
translation, and it is why `Module` now carries a name at all.

Five defects on the way, and the shape of them is the usual one — a rule that held for
the four languages already there:

- `generic()`'s fifth parameter is the *path* separator (`::` in Rust, `.` elsewhere)
  and reads as the argument separator when you meet it beside a list of arguments. The
  Java writer was written on that reading and turned `sync.Mutex` into `sync, Mutex`.
  Renamed to `path_separator`.
- `d[k] = v` rendered as `d.get(k) = v`, which is not a statement in Java. It is
  `d.put(k, v)`.
- Java was missing from the reserved-word table, so a Python `defaultdict(float)` wrote
  the keyword `float` into an expression position.
- A catch clause names its parts positionally rather than by field, so both the
  exception type and the binding were lost.
- `new ArrayList<>()` carried the diamond into the name.

The pair test asserted twelve ordered pairs and had four sources for five languages, so
it was quietly testing sixteen of twenty. It computes the count from `SUPPORTED` now,
which means adding a language without adding a source for it fails there rather than
silently testing four fifths of the matrix.

### Zig translation, and the receiver that had six names

The sixth language, and the one with the least in common with the rest. A `struct` in
Zig is not a declaration form but a **value**, so a record is a `const` whose value
happens to be a type and the methods live inside it. The grammar reuses one node for a
declaration and an assignment, so which of the two you are looking at is in the keyword
rather than in the shape.

The reader was written from a grammar dump and it was wrong in five ways, all of them
the same way: `cx.children` returns *named* children, and in this grammar the `:` before
a type, the `=` before a value and every operator are anonymous. Every field and
parameter lost its type, `var sum = 0` declared a variable called `var`, `a * b` put the
right operand where the operator belonged, and every `else` branch vanished without a
word. Running the translation found all five in one go; reading the code had found none
of them.

Four facts about the language shaped the writer, and each is a thing no other target
here needed:

- **No block comment.** `//` runs to the end of the line, so a carried fragment written
  beside an expression swallows the rest of the statement — semicolon included. Carried
  text is queued and flushed above the statement.
- **`var` is an error when nothing writes to it.** Only the Rust reader records
  mutability; every other one says "mutable" for want of anything better. The keyword is
  now worked out from what assigns to the binding.
- **Three naming conventions, not two.** Types `PascalCase`, functions `camelCase`,
  everything else `snake_case` — which is why `Kind` had to grow a `Function` case that
  is identical to `Value` in every other target.
- **`error` is a keyword.** Go's type carried across by name did not parse; it is
  written `@"error"`, which is Zig's own spelling for that collision.

Then the boyscout half, which was larger than the phase:

**The receiver has six names and the IR recorded none of them.** `self`, `this`, or
whatever the Go author called it — and because the receiver is the one binding that is
*not* in the parameter list, it never went through the rename every other name goes
through. Every translated method in the tool's history kept its **source's** word:
`this.cache` inside a Rust `impl`, `self.Cache` inside a Go method whose signature bound
`s`. It parses in neither. The IR records the word the source used and each writer puts
its own back on, through the same naming map every other rename goes through.

Underneath it, Rust refuses to raw-escape `self` — so the escape that makes every other
reserved word writable produced `r#self`, turning a correct body into a file that does
not build. `crate`, `super` and `Self` are the same and take a suffix instead.

**Python's `x = 1` declares once and assigns thereafter**, and every one of them was
read as a declaration. `total = total + x` inside a loop became `let total = total + x;`
in Rust, which shadows rather than accumulates: the value outside the loop never
changed. It parses, it type-checks, and it is the wrong program — the exact failure the
parse self-check cannot see. Python's scope is the function, so one set of bound names
carried through the body in order is precisely its rule.

**A TypeScript class member is public unless it says otherwise**, which is the opposite
of a free function's default. Reading both the same way made every translated method
private in Java and unreachable everywhere else, and every `private` field public.

The stale prose was the usual crop: comments counting "four languages" in a file that
had five, and a translation page pointing at "the third case" that had moved.

### Translating the tool's own source, and the nine things that fell out

Fixtures are written by whoever writes the assertion, and they pass. The repository
itself was not: twenty thousand lines of Rust, thirteen files of TypeScript, three of Go
and somebody else's Python, none of it written with a translator in mind. Running all of
it through every target and asking only "does the output parse" failed **97 of 235
translations** the first time.

Nine defects, and three of them had been changing the meaning of files since the
transpiler landed:

- **A comment inside a parameter list was read as a parameter.** A comment is an *extra*
  in every one of these grammars, so it can appear between any two nodes anywhere, and
  every reader reads a parameter list either positionally or through a catch-all arm.
  Both read a comment as whatever they expected in that position, so a four-line comment
  between two parameters became four parameters named after the sentence — lower-cased
  by the naming convention on the way. Fixed at the choke point: `Cx::children` returns
  the children that are part of the structure, and the one caller that wants comments
  asks for them by name.
- **Every string escape was doubled on every crossing.** The IR held the source's
  spelling rather than the string's value, so each writer escaped the backslash again
  and a newline crossed as a backslash and an `n`. It parsed, so nothing caught it.
- **A method became a free function whose body reached through a receiver nothing
  bound.** The IR's own documentation says methods are kept with their type, which is
  what lets one shape become the other; the Rust reader repeated that in a comment while
  pushing them out as top-level functions.

The other six: a multi-line comment got its marker on the first line only; a doc comment
quoting `app/**/route.ts` ended itself early and the rest of the sentence was parsed as
code; `0usize` was carried into languages that read it as a number glued to an
identifier; a Rust tuple struct lost its payload type without a word; `let _ = f()`
declared something with no name; and a method with no receiver was written as one with a
receiver, because one `bool` was answering both "inside the type?" and "takes a
receiver?".

Two diagnostics were fixed on the way, because a defect report has to carry evidence.
The self-check reported the *outermost* error, which for a whole-file failure is line 1,
column 1 — and printed the banner as context. It reports the innermost now, prefers a
`MISSING` node because that one names what was expected, and falls back to
`error_spans` when its own walk finds nothing, which is what happened for the empty Zig
struct.

The regression test is the sweep itself, in `tests/self_translation.rs`. It costs ten
seconds and it is the only test here whose inputs nobody chose.

### Real Java and real Zig, and the two things the IR did not have

The sweep found nothing more in this repository because there is no Java or Zig in it:
both readers were exercised only by fixtures somebody wrote to pass. Three files of
google/gson and two of zigtools/zls, vendored and pinned, found five defects — every one
of them in the Zig reader, and every one of them a thing no fixture would have thought
to include:

- **`?T` is a `nullable_type`**, which is not what it looks like it should be called.
  The arm written for `optional_type` matched nothing, so every optional in every Zig
  file crossed as a foreign type spelled `?T`. A pointer had no arm at all.
- **`comptime T: type`** is Zig's generics — a parameter that is a type — and reading
  it as a value produced `func Lazy(comptime type, comptime type) type`.
- **`const a, const b = pair;`** kept `a` and dropped `b`.
- **`_` as a parameter name** went through the naming convention, which asked what the
  empty word is called in `camelCase` and got the empty string back.

Then two things the IR simply did not have, both found by reading gson rather than by
running it:

**The conditional expression.** `a > 0 ? 1 : 2` was carried verbatim by all six writers,
including the five languages that have one. `Expr::Ternary` now crosses every ordered
pair among Python, TypeScript, Rust, Java and Zig; Go is the only target without one,
and turning an expression into an `if` statement needs somewhere to put the result,
which does not exist inside an argument list.

**The base class.** `class JsonPrimitive extends JsonElement` became a class that
extends nothing, which is a different type, and nothing in the output said so. Carried
into the three targets that inherit and reported for the three that do not.

The reporting had the matching hole: notes were printed only when `carried_verbatim > 0`,
so a translation that lost a supertype and nothing else gave a clean bill. The tool had
computed the honest answer and then declined to print it.

`tests/corpus/PROVENANCE.md` now describes four projects instead of two, and
`tests/vendor.rs` checks it the same way it checks the grammar manifest — a checksum
nobody verifies is a claim rather than a fact.

### The next bar up: did anything go missing?

"The output parses" is the weakest objective check and it found nine defects. It cannot
see the next class at all: a translation that drops a parameter, or invents one, or
loses a function altogether, produces a file the target's grammar is perfectly happy
with — and a fidelity report that says every signature carried across intact.

The check is a round trip. Read the source into the IR, translate it, read the *result*
back into the IR, and compare — the IR being the only place two files written in
different languages can be compared at all. What is compared is deliberately narrow:
**which functions exist, and what their parameters are called.** Types are where the
legitimate differences live (Go writes `struct{}` for nothing at all; Zig writes a slice
where TypeScript writes an array) and a check that argued about those would spend its
life growing exceptions. A parameter appearing or vanishing is never legitimate.

Four defects on the first run, and one of them was a hole this tool had dug for itself:

- **A `@staticmethod` disappeared from its class.** The Python reader handled decorated
  definitions at module level and not in a class body, so a decorated method fell to the
  member loop's catch-all — including the `@staticmethod` that this tool's own Python
  writer emits. Every associated function in a Zig file came back from Python missing.
- **Every reader's member loop ended with `_ => {}`.** That is the same shape as the
  comment-in-a-parameter-list defect: a catch-all that reads what it does not recognise
  as nothing at all. Java constructors were the largest thing it had been swallowing.
- **Python's `self` was stripped from free functions too**, which is a convention inside
  a class and an ordinary name outside one.
- **`r#where` grew an `r` every time it crossed.** The prefix is how Rust spells a name
  that collides with a keyword, not part of the name.

`transpile::read_file` is public now, because a round trip needs somewhere to stand: two
files in different languages have nothing in common except what it returns.

### The constructor, and what a name is worth

Making the record member loops stop swallowing what they did not recognise made Java
constructors visible for the first time — as carried comments, which is honest and
useless. Three of these six languages have a constructor and three have a habit:
`Thing::new`, `NewThing`, `Thing.init`. So the name is not what carries. What carries is
that the function **makes a value of its type**, and each writer spells that its own way.

The habit is only read as a constructor when the function *also returns the type*. A
`new` that returns something else is an ordinary function with a common name, and moving
it into a constructor's place would be moving it somewhere it does not belong.

Three things followed:

- **A constructor's own name must not claim a spelling.** Java names it after the class,
  so letting it into the naming map meant every Java class came out named after its
  constructor: `class a` where the source said `class A`.
- **A constructor has no receiver**, so the rule that binds an orphaned receiver as a
  first parameter must skip it — otherwise `Handle::new(files)` reads as
  `new(handle, files)`.
- **Its body only travels where a receiver does.** Python, Java and TypeScript act on a
  value that already exists; Rust, Go and Zig build one and return it, so a body that
  assigns through a receiver has nowhere to run. That is said, rather than written as
  `self.n = n` inside a function that binds no `self`.

The round trip needed two things said out loud, and both are true rather than
convenient: a constructor's name is compared as `<constructor>` on both sides, because
comparing `jsonarray` with `new` compares the two targets rather than the translation;
and a constructor may swap names in either direction, because a Rust `new_handle` that
returns a `Handle` is written `NewHandle` in Go — which is exactly how Go spells a
constructor, so it comes back as `Handle::new`. What is never allowed to change is the
parameters.

Underneath all of it, one older defect the round trip finally surfaced: `impl<'a>
Ctx<'a>` was read as an impl on `Ctx<'a>`, which matches no record in the file. **The
methods of every generic Rust type had been coming out as free functions** with a `self`
parameter bolted on.

### Types, and the seven readers that were reading them wrong

The round trip checked which functions came back and what their parameters were called.
It did not look at data at all — a field that vanishes is exactly as bad as a parameter
that vanishes — and it said nothing about types.

Adding fields and constants found the first disagreement immediately: **the Python
reader would not read back what the Python writer writes.** It required SCREAMING_SNAKE
for a module constant, while the writer spells a constant bound to anything but a
literal in lower case — on the grounds, written down in a comment, that shouting the
name of `schema = z.object(...)` would be wrong. Two rules deciding one thing.

Then types, compared as *shapes*: a list stays a list, an optional stays optional, a
named type keeps its name. Not which scalar — TypeScript has one numeric type, so an
`i64` that goes through it comes back a `number` and there is nothing wrong with that.
Not the qualifier either: Go has room for exactly one level of it, so
`crate::model::Reference` is `model.Reference` there and cannot be anything else.

That found something in almost every reader:

- **Rust stripped the reference last.** `&HashMap<K, V>`, `&Vec<T>`, `&Option<T>` — most
  of the types in a Rust file — were checked against the container patterns *before* the
  `&` came off, so none of them matched and all of them were read as names. Alongside
  it: `&'a str` kept its lifetime, `Node<'_>` read the lifetime as a type argument and
  produced a type with an empty name, and generic arguments were split on the first
  comma rather than the first one at depth zero.
- **Go's recursion was not its entry point.** The value of `map[string][]SymbolId`
  resolved one layer and lost the slice. The same lesson was already written down for
  TypeScript, in a comment, one reader away — which is the point of writing it down.
- **Every Zig type read from text was read wrong.** The grammar binds `?` tighter than
  `.`, so `?http.Request` arrives inside out as a field expression over a nullable
  `http`. A generic there is a name applied to its arguments, and reading
  `std.StringHashMap(V)` as one name turned a dictionary into a type with that name.
  Scalars were recognised only on a `builtin_type` node. And a Zig type can span lines
  and hold doc comments, all of which went into the name.
- **TypeScript read `readonly string[]` as an array of `readonly string`.**

Two tolerances had to be written into the check, and both are true rather than
convenient: a type this tool cannot write at all is replaced by a placeholder, which is
a rename and the one exception; and a constructor may change its name in either
direction, because in three of these languages "constructor" *is* a naming convention.

### Rename, and the name that belongs to more than one thing

The translation side has been checked against real code three times over. The
refactorings had not, and the newly vendored Java and Zig had never been renamed at all.
The probe was the same shape as the round trip: rename every symbol in the corpus to
something else and back, and see whether the file comes home.

Thirteen of a hundred and fifty-five did not, and behind them were four defects.

**The collision guard was file-scoped.** A parameter is written outside the body it
belongs to, so the scope it falls in is the one *around* its function — the file. Every
parameter of every function therefore shared a scope, and renaming one of them to a name
used by an unrelated function was refused as a collision. That was most of the renames a
real file offers.

Then the serious one. Renaming `add(int)` in a class that also declares `add(String)`
rewrote the declaration, left both call sites saying `add`, and reported **no warnings at
all**. Three separate things had to be true for that:

- **A member access was resolved through the lexical scope chain.** `c.run(1)` names a
  member of whatever `c` is; the scope chain has nothing to say about it, and answered
  anyway at `Exact` by picking whichever same-named method sat in an enclosing scope. The
  code four steps further down already says the right thing — "for a member access,
  proximity is not evidence" — but never ran, because step one had answered.
- **An overload set was resolved by proximity.** Two methods in one class body are a coin
  flip for a bare call. Proximity is evidence for a *binding*, where it reads as
  shadowing, and not for a callable.
- **A rename reported a same-named reference only when it resolved to nothing.** One that
  resolved *weakly to something else* was skipped in silence, because the winner was not
  the symbol being renamed. A weak resolution is a guess wherever it lands.

Measured over this repository's own source, the change costs seventeen `exact`
resolutions — every one of them a member access with more than one candidate, which is
precisely the case that was being guessed — and gains four hundred honest weaker ones.

### Extract and inline are inverses, and one of them was changing the answer

The rename probe worked, so the next pair got the same treatment: extract a
sub-expression into a binding, inline it straight back, and see whether the file comes
home. Five hundred and eighty-nine extractions across the vendored Java, Zig and Python.

**`inline` refused every Zig binding there has ever been.** tree-sitter-zig names nothing
on a `variable_declaration` — the `=` is an anonymous token with the value after it — and
the lookup asked only for a `value` or `right` field. The capability matrix said inline
variable worked for Zig; it had never worked once.

With that fixed, the round trip ran, and the answer it gave back was wrong:

    b = a + 1
    return b * 2        →        return a + 1 * 2

which is `a + 2`. **Every language with an expression grammar, since the operation was
written.** A refactoring that changes what the code does is the one thing this tool must
never do.

The fix errs toward a parenthesis. What is left bare is the set of things no surrounding
operator can split — a name, a literal, a call, a field, an index — and everything else
is wrapped, including where nothing could have bound tighter. The alternative is a
precedence table per grammar, and a table like that is wrong somewhere, silently, in
exactly the way that was just found. The `extract_then_inline_returns_the_original` test
now says `_expression` instead: what comes back is the original with the substituted
expression in parentheses, and that is worth stating rather than hiding behind a
comparison that strips them.

One more, from the Zig side: extracting an expression that *is* its statement leaves a
statement that only names the binding. `zzx;` is a parse error in Zig, an unused value in
Go, and nothing at all in the other three — and the value is already being computed for
its effect, so there is nothing to hoist.

After the three: 589 extractions, 21 refusals — all of them name capture or a struct —
and 40 files that came back differing only in parentheses.

### The catalogue says what does not change, and shows every file it changed

A refactoring is a change to the text of a program that leaves what the program does
alone. The catalogue was a list of edits; every entry now says, in its own words, what
stayed the same — and where a move reaches past the file it started in, the page shows
**every file**, including the ones that did not have to change. Six entries do. A
signature change is only half a refactoring until the calls change with it, and showing
the declaration alone is showing the half that on its own breaks the program.

Rewriting the Remove Parameter sample so the parameter is genuinely unused turned up a
defect at once: `fr signature f remove:1` on `def f(a, b): return a + b` produced
`def f(a): return a + b`. The rule existed for shell functions and for nothing else,
which is the shape most of the defects in this tool have had. Two SCSS tests were
asserting the broken output.

### A pet store, moved without moving its API

Then the other half of "behaviour preserved": a rewrite that changes the language and
the framework and keeps the only thing anyone outside the repository can see. There is
now a full worked example — `tests/petstore`, eight route files, thirteen operations —
with one of every shape a CRUD API has: a collection, a member, a sub-collection, a
sub-member, a whole-resource replacement, an action that is not CRUD, an aggregate under
a second root, and a catch-all that matches across slashes.

Building it found four things the contract had been quietly leaving out:

- **A zod schema declared in another module was invisible.** A real Next.js application
  keeps its shapes in `lib/schemas.ts` and imports them; only the route file was read,
  so `components` came out empty. Every `.ts` file in the tree is read now.
- **The schemas nothing referred to.** A `components` section with no `requestBody`
  pointing at it says every endpoint takes no body. The link comes from the
  `petCreateSchema.parse(json)` call inside the handler — the only place a Next.js route
  records it.
- **No query parameters at all.** Next.js declares none; a handler reads them out of the
  URL. Where a statement could not be read whole, the document now says a query
  parameter inside it may be missing, because a gap nothing mentions is exactly the
  failure this is about.
- **`context.params.petId` inline in a handler body.** Dropping the *statement* that
  pulls a path parameter off the context was only half of it: read inline, the Next.js
  spelling survived into Python and the endpoint answered every request with a
  `NameError`. The value arrives by a different route in FastAPI and every use of it has
  to arrive with it — which is the behaviour being redistributed while the URL stays the
  same.

`fr openapi --yaml` writes the document the way a contract kept beside the code is
usually written. `docs/contract.html` works the whole example, generated by running the
binary, and `API_CONTRACTS.md` has the method in prose.

Then the loop closed. `fr openapi` reads a **FastAPI router** too — off the decorators
and the signatures, which is where FastAPI itself reads it — so the same command answers
the same question about the code the rewrite produced. Half the check in step 4 needs no
server at all. Over the pet store, thirteen operations go in and thirteen come out with
every URL, method and path parameter identical, and the one thing that does not survive
shows up as a line of diff:

    - GET /pets?species
    + GET /pets

The translated handler still read `species` off the request object rather than declaring
it as a FastAPI parameter, so the router did not say it took a query.

### Closing it

The same move as the path parameters, one level out. A query parameter is declared
`str | None = None` — which is exactly what `searchParams.get()` returns — every read of
it in the body becomes that name, and the binding that was doing the reading is dropped,
because a binding of a name to itself is a statement that does nothing. Leaving the read
in place would have been worse than not declaring the parameter: `req.nextUrl` is a
Next.js object and Starlette's `Request` does not have it, so the line would not run.

Thirteen operations now go in and thirteen come out with **everything** identical, and
the site data test asserts it rather than the page claiming it.

What that does *not* mean is that the contract is complete, and the difference is the
more important half: both sides can be missing the same thing and agree perfectly. So
the baseline says what it could not read, and that number is what to watch.

### The gap that announced itself, closed

It said *two statements* in `app/api/pets/route.ts`, and one of them was
`const limit = Number(req.nextUrl.searchParams.get("limit") ?? "50")`. `??` had no
counterpart in the IR, so it took the whole statement with it and `limit` reached
neither document.

It asks whether a value is absent, which is a question rather than an arithmetic
operator, and the six languages disagree about how to ask it: Zig spells it `orelse`,
Rust reaches for `Option::unwrap_or`, Java for a static method, Python has to name the
value twice, and Go cannot say it at all. `Expr::Coalesce` carries the question; each
writer asks it its own way; and the two that can only ask by naming the value twice
refuse when naming it twice would *call* it twice, because that would make the program
do more than it did.

The note counts one statement now, `limit` is on both sides of the crossing, and the
page says what closing a gap looks like from here.

### The shorthand every TypeScript file is written in

The pet store's baseline still said one statement could not be read, and it was this:

```ts
const pets = await db.pet.findMany({ where: species ? { species } : {}, … })
```

`{ species }` means `{ species: species }`. Reading it as something unrecognised refused
the whole object, and refusing the object refused the statement the object was in — so a
single shorthand property cost the endpoint its body. The count is zero now, and every
route in the pet store translates with nothing carried.

### The palette

Grey scale, one saturated blue that means "you can act on this", square corners
everywhere. Every colour that is not the blue is carrying information — a confidence
tier, a diff line — so there is nothing left over to decorate with, which is the same
argument the rest of this tool makes about output.

Two things had to be said out loud rather than inherited. A terminal is dark whichever
way the page is, so its prompt cannot borrow the page's blue: `#0043ce` on `#161616` is
a shape rather than a colour. And the primary button's label is fixed rather than
following the paper, because the blue is light in one theme and dark in the other and a
label that followed would vanish into one of them.

### The rewrite that negated half a condition

`invert-if` is its own inverse: swap the branches, negate the condition, do it again and
you have what you started with. That is a property, so it can be probed — apply it twice
at every conditional in the corpora and see which files come home. Five of forty-one did
not.

Behind them, one defect and one blind spot.

**The negation was applied to the first comparison in the text.** `if a == 1 and b == 2`
became `if a != 1 and b == 2`, and the branches swapped anyway. That is a different
program: the negation of an `and` is an `or` of the negations, and flipping one operand
cannot say it. The guard that was supposed to prevent this excluded `&&` and `||` — the
C spellings — and knew nothing of the languages that spell them as words. The same rule
also flipped the comparison inside `g(a == 1) == 2`, because it searched the text rather
than the expression. Both are fixed by the same move: simplify only when the comparison
is the whole of the condition *and* sits at the top level, and otherwise put the negation
round the outside, which is what De Morgan is there to distribute afterwards.

**Zig fell into the C arm of the boolean table.** It writes `and` and `or` as words, as
Python does, and negates with a sigil, as C does — so it matched neither and every rule
that looks for a boolean operator was blind to it. `!(a and b)` is also an
`error_union_type` in that grammar, because `!T` is an error union where a type is
expected and a negation where a value is: inside a condition there is no type, so the
node is a negation whatever it is called.

And one refusal that was missing. Zig writes `if (maybe) |value| { … }`, where the
condition is an optional and the payload binds what was inside it; inverting gave
`if (!maybe) |value|`, which is not a program. The *reader* had refused that shape for
that reason since it was written. The rewrites had not — which is the same lesson as
every other entry in this file, arriving from a different direction.

After: forty-one probes, thirty applicable, and every one of them an involution.

The pet store's TypeScript joined the translation and round-trip sweeps at the same
time. It is the most idiomatic TypeScript in the repository — builder chains, shorthand
properties, nullish coalescing, a shared schema module — and it passes both.

### The move that left an import pointing at nothing

Two more operations probed over the corpora, and the two answers were opposite.

**Organising imports is idempotent** — do it twice and the second time changes nothing.
Forty-four files across five languages, and it holds everywhere.

**Moving a symbol was not.** The probe that found it was the wrong probe: a move out and
back is never byte-identical, because the symbol comes back at the end of the file rather
than where it left. But looking at one concrete pair showed something that had nothing to
do with round trips:

```python
# a.py, after moving `area` into it from b.py
from .b import area          # b.py no longer defines it

def label(r): …

import math                  # in the middle of the file
def area(r): …
```

**The destination kept importing what it now defines.** The move adds an import to every
file that references the symbol, deliberately skipping the destination — which is right,
because it does not need one. Nothing removed the import it already had, so the file
failed on the line that used to make it work. An import naming several things is narrowed
rather than deleted; the rest are still over there.

**And what the moved code needs was written above the code rather than where imports
go.** Legal in Python, a syntax error in half the other targets, and wrong-looking in all
of them.

The property that *is* right for a move — every reference still resolves — holds over
seventy moves in three languages, before and after.

### The signature change that skipped every `new`

Swapping two parameters twice should return the file to what it was. Seven swaps over the
corpora, seven restored — and the involution held while the *refusals* said something was
badly wrong. Java refused everywhere, with a message about resolution strength for
references that had resolved exactly.

**A Java call is not spelled "call".** The hunt for a call site matched on
`kind().contains("call")` plus one named exception, under a comment saying SCSS's
`include_statement` was "the one call form whose kind does not say call". True of the
languages it was written against. Java says `method_invocation` for a call and
`object_creation_expression` for a construction, so `fr signature` had never once
rewritten a Java call site. The wrong message made it worse: it named a confidence, so
the reader goes looking for a resolution problem that is not there. That refusal is now
`Unknowable` — for the things the tool cannot establish at all, as against a resolution
that exists and is too weak to act on.

**Then the constructor, which was the serious one.** `new Thing(1, "x")` is recorded as a
reference to the *type* — which it also is — and the loop skipped every reference whose
recorded kind was not `Call`. So this happened, and nothing warned:

```java
-    B(int a, String b) { }
+    B(String b, int a) { }
     static B make() { return new B(1, "x"); }   // untouched
```

A signature change is the refactoring with the least room for a partial result: the
declaration and every call move together, or the code stops compiling. This is the same
silently-wrong-answer class already fixed for `rename`, and the fix has the same shape —
stop trusting a label when the thing itself is right there. The grammar decides whether a
reference is a call. A mention that really is not one, the `C` in `static C make()`, has
no arguments to change and is passed over.

**Which immediately introduced its own bug, caught before it shipped.** The walk goes up
to eight parents, so a type named inside somebody *else's* argument list —
`register(Pet.class, 7)` — now found that enclosing call and would have reordered *its*
arguments as though they belonged to `Pet`. The walk now requires the reference to sit
before the argument list of the call it lands on, which is what "this reference names the
thing being called" actually means.

### The chart whose consumer was invisible

The previous three defects all had one shape — a rule true of the languages it was
written against — so the next sweep went looking for that shape directly rather than
waiting for it to surface: every per-language table in the tool, checked against the
languages that arrived after it was written.

**`fr stitch` could not see a Java or Zig program read its configuration.** The accessor
table knew `os.Getenv`, `os.environ`, `env::var`, `process.env` and `$FOO`. It did not
know `System.getenv`, which is how every Java service on earth reads a variable. So a
Helm chart feeding a Java service reported every variable as orphaned — configuration
with no consumer, which is precisely the finding this command exists to produce, and
here it was produced backwards. The worst kind of wrong answer: not a refusal, an
assertion.

Zig needed more than a prefix. Its accessor is
`std.process.getEnvVarOwned(allocator, "DATABASE_URL")` — the allocator comes first, and
the reader took the name from directly after the paren. So it read `allocator`, which is
lower case, which the "environment variables are upper case" filter dropped. A miss
inside a miss, and neither said anything. Accessors now carry how many arguments stand
between them and the name.

**And the capability row was a transcription.** `support()`'s own doc comment says every
arm asks the refactoring's predicate and that nothing there is a transcription. The
stitch arm listed six languages by hand, so the table said Java and Zig "neither declare
environment variables nor read them" — a sentence that is simply false about both. It
asks the analysis now.

Two more fell out of looking at the table itself:

**A shell function was told it needs a return type and modifiers.** The reason for an
absent capability is picked by language *class*, so every imperative language gets one
sentence, and that sentence was Java's. This exact defect was fixed once already — the
reasons had been written when every unsupported language was markup, and told Java it
was a stylesheet. A second imperative language landing on the same arm brought it
straight back. The guard test now carries a word-to-language table instead of one list
of markup words.

**The number describing the matrix was not checked, and had drifted.** The rows are
regenerated and asserted; the sentence above them counting the rows was prose. It said
260 supported where its own table counted 261, and PLAN.md quoted a total from before
six capabilities and a language existed. It is asserted now, in all four places it is
published, against the computation that produces the table.

### The hierarchy the tool would not read

The sweep continued through the per-language tables, and the next one was the shortest
function with the largest consequence:

```rust
fn of(language: Language) -> Option<Family> {
    match language {
        Language::Rust => Some(Family::Rust),
        Language::Go => Some(Family::Go),
        Language::TypeScript | Language::Tsx => Some(Family::Ts),
        Language::Python => Some(Family::Python),
        _ => None,      // "Zig dispatches through comptime duck typing and Bash
    }                   //  has no methods at all"
}
```

Both of those are true, and Java is neither. It fell into the same `_` — so the one
language here that states its hierarchy in as many words was the one whose hierarchy
went unread. The same twelve lines of code, side by side:

```
$ fr callers shape.ts:2:33          $ fr callers Shape.java:6:19
Circle::area                        Circle::area
  total [field-based]
```

A call through an interface reached no implementation. Not a refusal — an empty answer
to "who calls this?", which is the question `fr delete` is built on. Three vendored gson
files now produce 80 hierarchy edges where they produced none.

**And adding it turned up a defect in shared code.** The heritage reader took every type
name under the clause, so `implements Holder<Pet>` filed the class under `Pet` as well.
A call that reached the method by name alone was then reported as reaching it through a
declared relationship. The edge is identical either way; the evidence for it is not, and
presenting a guess as a declaration is the one thing this layer must not do. The helper's
doc comment had claimed it excluded type arguments since the day it was written.

### The entry point that is not a function

The sweep through per-language tables reached the entry-point catalogs, and the probe
was the simplest one available: write the same program in six languages and ask where it
starts.

```
$ fr entrypoints
cli-main   main   main.go
cli-main   main   main.rs
cli-main   main   main.zig
cli-main   Report::main   Shape.java

3 entry point(s)
No entry-point rules exist for: css, scss, yaml
```

Python is missing, and the last line says it is covered. Every catalog says
`name: main`, because every other language here agrees that a program starts in a
function so called. Python's starts in a *statement*:

```python
if __name__ == "__main__":
    cli()
```

`cli` can be named anything, so no name rule can find it — and the command whose only
job is answering "where does this start?" answered nothing for a script that plainly
starts somewhere. Catalogs gained `called_from_main_guard`, the first predicate here
that is not a property of a name. Direct calls only: what the guard calls is the
starting point, and what *that* calls is reachability, which the call graph already
answers.

Two more came out of the same file. **Nothing calls a pytest fixture by name** — pytest
injects it by matching the parameter — which is exactly the reasoning the Java catalog
already gives for `@Bean` and `@Component`. A fixture in `conftest.py`, which is where
the shared ones live, matched no rule at all: neither the file nor the function is named
`test_*`. The ones inside a `test_*.py` file were found by the file rule, which is what
made the gap look closed. **And `unittest` calls `setUp` and `tearDown` itself**, once
per test, with nothing in the source referring to them.

### The inline that returned a different number

`fr inline --call` was next, and the probe was the same one: the same function in six
languages, inlined at the same call.

```
-    double(x + 1)
+    (x + 1 * 2)
```

Six languages, six wrong answers. `double(x + 1)` returns `(x + 1) * 2`; the expansion
computes `x + 2`. The body binds its parameters at whatever precedence it was written
with, the argument arrives as text, and nothing put it back together.

Inlining a *variable* was fixed for exactly this earlier — the substitution runs the
other way there, from the bound value into its use, and the fix for one is not the fix
for the other. The grouping test existed; the call path did not reach it, and when it
did reach it the test asked the wrong question. It was gated on
`extract::supports_imperative_extract`, which has a mostly overlapping answer — and the
overlap is where the wrong ones live. Java groups with parentheses like every other
C-shaped language here and is missing from that list because it has no inferred
declaration to extract into; Bash is the other way round, supporting the extraction
while `( … )` there opens a subshell. It asks whether the language groups with
parentheses now.

**Then the outer bracket, which was a string heuristic.** "Is this already bracketed?"
was answered by reading the first and last character, and `(p + 1) / (q - 1)` starts
with one and ends with one:

```
-    2 * scale(p + 1, q - 1)
+    2 * (p + 1) / (q - 1)        # p = 1, q = 4: the call returns 0, this is 1
```

Both are the shape this project keeps finding: not a refusal, not a crash, a diff that
applies cleanly and quietly means something else.

Commands: `scan`, `parse`, `symbols`, `def`, `refs`, `rename`, `extract`, `inline`,
`signature`, `move`, `delete`, `unused`, `duplicates`, `imports`, `restructure`,
`rewrite`, `remove-flag`, `callers`, `callees`, `graph`, `flow`, `impact`, `stitch`,
`entrypoints`, `capabilities`, `cache`, `openapi`.
