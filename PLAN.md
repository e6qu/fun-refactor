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
capability a language can meaningfully support is built: **231 of 315 capability ×
language pairs supported, 84 not applicable, none refused.**

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

Commands: `scan`, `parse`, `symbols`, `def`, `refs`, `rename`, `extract`, `inline`,
`signature`, `move`, `delete`, `unused`, `duplicates`, `imports`, `restructure`,
`rewrite`, `remove-flag`, `callers`, `callees`, `graph`, `flow`, `impact`, `stitch`,
`entrypoints`, `capabilities`, `cache`.
