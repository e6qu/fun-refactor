# fun-refactor — Plan

Multi-language refactoring + code-intelligence CLI on tree-sitter, covering the funveil
language suite. Research and provenance for every design choice: see [RESEARCH.md](RESEARCH.md).

- **Crate**: `fun-refactor`, binary `fr` (provisional). Rust 2021, **AGPL-3.0-or-later**.
- **Repo**: `github.com/e6qu/fun-refactor`. Commits authored as `e6qu
  <2966430+e6qu@users.noreply.github.com>`; remote uses the `github.com-e6qu` SSH alias.
- **Languages** (16 variants): Rust, Go, Zig, Java, TypeScript/TSX, Python, Bash, HTML,
  CSS/SCSS, Terraform/HCL, Helm/YAML, XML, Markdown. Grammar pins inherited from funveil
  (tree-sitter 0.26 line); Java was added afterwards and priced at one query file, five
  lines of enum and three transpiler cases.
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

Landed: `src/span.rs` (byte-native `Span` + `LineIndex`), `src/lang.rs` (language
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
capability a language can meaningfully support is built: **270 of 384 capability ×
language pairs supported, 114 not applicable, none refused.**

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

Five recurring shapes, each of which has caught more than one defect:

1. *A rule true of the languages it was written against, applied to one that arrived
   later* — Java constructors, Go interfaces, Zig's six spellings of a receiver.
2. *Where does the search stop, and does the output say so?* — `fr impact`'s depth bound,
   `fr duplicates`' threshold, `fr unused`'s composition.
3. *Does the test check what its name claims?* — one asserted a cache fingerprint was
   steady rather than correct; several counted results without inspecting them.
4. *The tool's own output is not valid input* — enum-variant struct literals it could not
   re-read, FastAPI handlers it emitted and then reported dead, `SymbolKind` JSON it
   could not deserialize.
5. *A framework calls it and the source never does* — Python's `__main__` guard, pytest
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
other 73%, which is what sent the next probe at class hierarchies. And a fix that looked
right by edge count was caught by measuring dead code instead: comparing Go signatures
*as written* refused seven `PrintingKubeClient` methods, because `ResourceList` inside the
package and `kube.ResourceList` outside are the same type spelled differently.

### The log

Build-out, in order: sixteen languages; six transpiler readers and writers, thirty ordered
pairs; the recipe language; the entry-point catalogues; the published site and its
WebAssembly playground; the refactoring catalogue page; the API-contract invariant; the
types tutorial. Each is recorded in BUGS.md with what it broke on the way.

What the sweeps found, grouped by what went wrong rather than by when:

**An expression moved into a context it was not written for.** Caught four times, in
`fr inline`, `fr restructure`, `fr extract` and `translate`, and each time the fix was
bracketing driven by one shared predicate rather than four local ones. The operators the
six languages spell alike and mean differently — division, remainder, string equality —
account for most of it.

**A refactoring that left the program not compiling.** A move that left an import pointing
at nothing; a move that left its dependencies behind; a signature change that skipped
every `new`; a flag removal that took a class with it; a rewrite that negated half a
condition; a method that could not change its own object.

**A reader that dropped what it did not recognise.** Record members, constructors, type
annotations, Rust's `Counter { value: 0, step }`. Silently, until the round trip started
comparing what came back.

**An answer that was true and not usable.** Qualified names the tool printed and would not
accept back; a parse failure that said how many and never where; a trace that went one hop
and printed four; a threshold mentioned only when it found nothing.

**Documentation that had stopped being true.** Three separate passes. The first was a
sweep — find the stale number, replace it — and the second found five defects the sweep
had walked past, all of them the tool saying something untrue about the tool. The lesson
stuck: the capability matrix is now computed from each refactoring's own predicate, the
site's command names are checked against the binary, and so is the list of commands below.

**The site.** Driven in a browser rather than read, which found dead links and a page that
was three commits behind and did not say so. Every page now stamps what it was built from.

### The last four findings

These are recent enough that the reasoning is still worth having in full.

**A framework calling it is what makes it an entry point.** Asked of every framework the
catalogues claim, rather than waiting for a repository to surface the next one. FastAPI
handlers were dead code — on a project with a page devoted to porting Next.js routes to
FastAPI, whose own `fr translate <route> fastapi` emits handlers it then called unused.
Flask and actix were covered only by coincidence: `@app.route("/health")` above
`def health` spells the symbol's name in a string literal, which `fr unused` skips. Three
defects underneath the rules: a dot in an annotation's arguments captured the name, so
`@app.route("/v1.0/status")` matched nothing while `/status` matched; `export` between a
decorator and its class ended the search, so `annotated_with` did not work on exported
TypeScript classes at all; and a decorator's name is not unique across libraries —
`@app.patch` tagged twenty-two of black's test methods as remotely reachable, because
`@patch` is `unittest.mock`'s. What separates them is what the decorator *names*, a path
or a module, so route rules ask for `/`.

**One definition of passing.** A branch was pushed green and rejected: `cargo fmt --all
--check` was one of CI's steps and not one of the commands run locally. Neither set was
wrong; there being two sets was. `tools/check.sh` holds them and the workflow calls it.

**Being the only method of that name is not knowing the receiver.** A single definition of
a name in a file resolved any use of that name at `Exact`, and the rule did not exclude
member accesses — so `fr rename total sum` rewrote `client.total()` on a boto3 client
because a class in that file declared `total`. Only the top two tiers are rewritten, so
this was an unasked edit, not a misleading report. `FieldBased` is defined as this exact
case: the tier existed and was not being used.

**The tier is decided once.** Asking whether stronger typing would have made that
unrepresentable found the fix incomplete: the branch above it held the same belief and
still rewrote the call when it was written inside the declaring class. `resolve_one`
returned `(Option<SymbolId>, Confidence)`, which lets any label sit beside any answer
across twenty-eight branches. The rule now lives in one place — resolve, then cap what the
answer may claim — and `EdgeOrigin::Hierarchy(basis)` one module over is the shape that
never had the problem, because it carries its justification inside the variant.

Costs, measured across three repositories: black's exact edges 881 → 795, vuejs/core's
2384 → 2240, helm's 4727 → 4133. Those are reported for review now instead of rewritten.

**Where else a type could have said it.** Asking that question of the rest of the codebase
found four more, all the same family: a value that is *checked* somewhere instead of being
*unrepresentable*.

A catalogue's `symbol_kind` was a `String` and its `languages` a `Vec<String>`, compared
against the real enums by name. `deny_unknown_fields` rejects a misspelled key; nothing
rejected a misspelled value, so `symbol_kind: functoin` and `languages: [pyhton]` parsed,
loaded and never fired — a rule that is present and never true, which reads exactly like a
framework that is covered and absent. Parsing them into the types they denote turns both
into a message at load with the line, the column and the values that would have worked.
`Rule.provenance` went too: a field defaulting to `"manual"`, written by no catalogue and
read by nothing.

Underneath that was a real defect. `SymbolKind` has a serde derive *and* a hand-written
`as_str`, and three of twenty-one variants disagreed — `as_str` said `type`, `link-def`,
`element-id` where serde wanted `type_alias`, `link_def`, `element_id`. The output uses
`as_str`, so `fr symbols --json` emitted `"kind": "type"` and the tool could not read its
own JSON back. Shape number four again, in a place nothing had thought to look.

It hid because `as_str` meant two different things. On `SymbolKind`, `Confidence` and
`EntryKind` it is an identifier — it goes into JSON, into a catalogue, into a person's
fingers, and has to match the serde spelling exactly. On `Capability`, `Basis` and
`DefinitionRole` it is prose for a reader: "call graph", "from the literal", "also
declared here". Those three are `label()` and `describe()` now, and the identifier ones
have a round-trip test that reads its cases out of the exhaustive `as_str` match instead
of a list — the compiler already forces a new variant into that match, so a new variant is
covered the day it is added rather than the day somebody remembers.

And `fr type --json` was answering with `"symbol": 1` and `"defined_at": 0` — `SymbolId`s,
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
differs — a child module nothing references is a finding — so the exclusion tests the
language, not the kind.

**Containers of entry points.** JUnit constructs a test class to run its `@Test` methods;
nothing names the class. The check walks the containment chain rather than testing the
language, so it also covers Rust `mod tests` and Python classes of pytest cases.

**JavaBean accessors.** `getAddress` reported dead while the template writes
`${owner.address}` and the tests write `param("address", …)`. Java templates, JSON mappers
and Spring's binder reach a getter by the property name.

**HTML attribute values.** `is_string_kind` matched node kinds containing "string", and
the HTML grammar names an attribute value `attribute_value`. So `th:text="${owner.address}"`,
`v-on:click="submitOrder"` and `class="table-striped"` were invisible to the correction
that spares names spelled in strings, and 80 of petclinic's CSS classes reported dead
while its templates used them.

**Three Spring annotations** — `@InitBinder`, `@ModelAttribute`, `@Configuration` —
joining the eight from the earlier sweep. That sweep enumerated what Spring calls; these
came from running the tool at an application.

Code findings: 35 → 3 — a constructor Spring calls, a testcontainers field, a nested
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
qualifier. Where a variable and a local share a name — 18 of 81 in that repository —
`fr refs` on the variable returns the local's reference as well as its own, and `fr refs`
on the local returns none. Both drop to `field-based`, so nothing is rewritten. The
reference half is a one-line query change; the symbol half is not, because `var` and
`local` appear in no declaration and a query cannot synthesise a name, so the qualifier
would have to come from `extract.rs` and would change every HCL qualified name and the
cache schema with it.

### Zig at scale

29 files of Zig's own standard library — `http`, `json`, `fmt`. One parse failure, and it
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

### Sweeping the refusals

The Bash run found three defects in what refusals say rather than in what they refuse, so
the next pass took that as the question and asked it of every `Refusal::TooWeak`. The
sites divide by what they put in the confidence field: one reporting a real reference
writes `reference.confidence`, and five wrote `Confidence::NameOnly` because there was no
reference to ask. All five say "cannot be known" or "cannot be shown" in their own text
and were then prefixed with "resolution is only 'name-only'".

`TooWeak` now takes a `ResolvedConfidence`, whose field is private to `model` and which
only `Reference::resolved_confidence` produces. The variant cannot be built without a
reference to take a confidence from — checked by trying, which the compiler refuses as a
private constructor. `signature.rs` stopped naming `Confidence` at all.

### Bash at scale

`nvm`, 5,655 lines across five scripts, parses clean. `fr signature` moved a positional
parameter of `nvm_tree_contains_path` and renumbered the body and all three call sites
correctly, which is the operation with the most shell-specific machinery behind it.

Three defects, all in what the refusals say rather than in what they refuse. A signature
change on a function with a twin in another file refused by raising the refusal `rename`
and `extract` use, so it said "renaming would shadow or collide with it" to somebody who
had asked to move a parameter. An argument whose word count the shell decides at run time
refused as "resolution is only 'name-only'" — `Refusal::Unknowable` exists for that and
its doc comment names the symptom, so the fix had been written down and this site had not
been changed. And the remedy "quote it to make it one argument" was appended to every one
of those refusals, including `$@`, where quoting gives one word per parameter and the
same problem again.

Commands: `scan`, `parse`, `symbols`, `def`, `refs`, `usages`, `implementations`,
`rename`, `extract`, `inline`, `signature`, `move`, `delete`, `unused`, `duplicates`,
`imports`, `restructure`, `rewrite`, `remove-flag`, `recipe`, `translate`, `callers`,
`callees`, `graph`, `flow`, `impact`, `stitch`, `entrypoints`, `capabilities`, `cache`,
`openapi`, `type`.
