# fun-refactor. Research: what "standard refactors" means for the funveil language suite

Target languages (inherited from [funveil](https://github.com/e6qu/funveil), see its
[docs/LANGUAGE_FEATURES.md](https://github.com/e6qu/funveil/blob/main/docs/LANGUAGE_FEATURES.md)):
**Rust, Go, Zig, TypeScript/TSX, Python, Bash, HTML, CSS/SCSS, Terraform/HCL, Helm/YAML, XML, Markdown**,
all parsed with tree-sitter (funveil pins tree-sitter 0.26 + per-language grammar crates).

## 1. The canonical catalog

Fowler's catalog ([refactoring.com/catalog](https://refactoring.com/catalog/)) has ~66 named
refactorings, but mainstream engines automate only about a dozen. Cross-referencing
[rust-analyzer's assists](https://rust-analyzer.github.io/book/assists.html),
[gopls transformations](https://go.dev/gopls/features/transformation),
[VS Code / TS language service](https://code.visualstudio.com/docs/typescript/typescript-refactoring),
[rope](https://rope.readthedocs.io/en/latest/overview.html) /
[PyCharm](https://www.jetbrains.com/help/pycharm/refactoring-source-code.html), and
[IntelliJ's common set](https://www.jetbrains.com/help/idea/refactoring-source-code.html),
the **table stakes** are:

| Refactoring | LSP kind | Knowledge required |
|---|---|---|
| Rename symbol | `textDocument/rename` (+ `prepareRename`) | scope resolution + cross-file reference index + conflict/shadowing detection |
| Extract variable/constant | `refactor.extract.variable` | mostly syntax: expression boundaries, side-effect awareness, insertion point |
| Extract function/method | `refactor.extract.function` | single-file data flow (ins → params, outs → returns, control-flow exits); type info only to print signatures |
| Inline variable | `refactor.inline.variable` | scope + shadowing check at each use site |
| Inline function/call | `refactor.inline.call` | hardest: reference index + semantics preservation (evaluation order, side effects) |
| Move symbol / move to file | `refactor.move` (formalized only in LSP 3.18 proposed) | cross-file references + module/import-system knowledge |
| Change signature | **no native LSP support** (gopls fakes it with `refactor.rewrite.moveParam*`) | call-site index + type info for defaults |
| Organize imports | `source.organizeImports` | module resolution to detect unused |
| Safe delete | (JetBrains concept) | inverse reference index ("who still uses this?") |

Advanced tier (differentiators): the `refactor.rewrite.*` micro-transform tail (invert-if,
guarded return, de Morgan, fill-struct/switch, loop↔iterator), class-level ops
(extract class/interface, pull up/push down), async conversions, and rope-style
**pattern restructure** (user-supplied before/after patterns, the only precedent for
automating catalog entries like Replace Loop with Pipeline).

Notable gaps in the ecosystem we can exploit:
- LSP has **no change-signature flow**, a CLI-native one is an advantage. It is not a workaround.
- gopls drops comments in extract/inline ([golang/go#20744](https://github.com/golang/go/issues/20744));
  comment preservation is a known weak spot everywhere.

## 2. The language suite splits in two

**Big-4 code languages (Rust, Go, TS/TSX, Python)**, mature LSPs exist
(rust-analyzer, gopls, typescript-language-server, pyright). Correct project-wide rename here
requires imports/types/dynamic dispatch; only a compiler-grade frontend gets it right.

**The other eight (Zig, Bash, HTML, CSS/SCSS, HCL, Helm/YAML, XML, Markdown)**. LSP support is
weak to nonexistent (terraform-ls rename: open request
[terraform-ls#1155](https://github.com/hashicorp/terraform-ls/issues/1155). Bash-language-server
rename: [bash-lsp#161](https://github.com/bash-lsp/bash-language-server/issues/161); zls rename
returns empty edits in practice, [serena#799](https://github.com/oraios/serena/issues/799);
nothing rename-grade for HTML/CSS/XML/Markdown/Helm). But their name semantics are simple and
**string-keyed** (Terraform addresses, Helm `.Values` paths, CSS selectors, XML ids, Markdown
anchors), small hand-written binders on tree-sitter are feasible and *nobody else has built
this*. Cross-language refs (CSS class ↔ HTML/TSX `className`, `values.yaml` ↔ template
`{{ .Values.x }}`, Terraform `var.x` ↔ `variables.tf`) are invisible to every LSP, the unique
value of a multi-language tool.

## 3. Architecture options

| | Pure tree-sitter syntactic (ast-grep/GritQL/comby) | tree-sitter + own name resolution | LSP delegation |
|---|---|---|---|
| Pattern rewrite / migrations | excellent | same + scope filters | poor (only rust-analyzer SSR) |
| Shadowing-correct local rename | wrong | good via `locals.scm` scope trees | overkill |
| Project-wide rename (imports/types) | unsafe | must rebuild per-language import resolution | correct where servers exist (big 4) |
| Type-dependent ops | impossible ([ast-grep FAQ](https://ast-grep.github.io/advanced/faq.html)) | impossible without a type checker | only option |
| Config/markup languages | works but unscoped | **sweet spot** | servers weak/absent |
| Cross-language references | only explicit rules | **feasible. We own the model** | invisible to every LSP |
| Ops cost | zero | per-language authoring | daemons, config discovery, version skew |

Key facts:
- **`github/stack-graphs` is archived** (2025-09-09, read-only; only 4 language definitions ever
  shipped: Java/JS/Python/TS), do not build on it.
  [`tree-sitter/tree-sitter-graph`](https://github.com/tree-sitter/tree-sitter-graph) is alive
  but you'd own all binding rules.
- tree-sitter `locals.scm` captures (`@local.scope/definition/reference`) give per-file
  lexical scoping, enough for shadowing-safe renames, but no imports/types, and shipped
  query quality varies per grammar.
- **Lossless editing**: tree-sitter CSTs keep every byte (comments included). So edits are
  byte-range splices on original source, applied descending by offset, followed by incremental
  reparse + assert-no-ERROR-nodes
  ([docs](https://tree-sitter.github.io/tree-sitter/using-parsers/3-advanced-parsing.html)).
  This beats pretty-printing (recast's fallback reformats; OpenRewrite's whitespace-carrying
  LST needs a compiler frontend per language). ast-grep's indentation-aware fixes prove the
  approach yields hand-written-looking output.
- **Piranha lesson** ([uber/piranha](https://github.com/uber/piranha)): cascading rule graphs
  (delete flag → inline constant → delete dead branch → delete unused var/file) get deep
  cleanup without a full semantic model.
- **Headless LSP prior art**: `gopls rename -w` is the model CLI; Serena/SolidLSP
  ([oraios/serena](https://github.com/oraios/serena)) is the best-documented multi-server
  headless orchestration. Real costs: server startup (everyone ends up daemonizing), per-language
  project-config discovery, applying `WorkspaceEdit` yourself, capability checks before offering
  an op.

## 4. Recommended hybrid

1. **Substrate (all 12)**: Rust CLI, funveil's grammar set, byte-splice edit engine with
   descending-offset application, post-edit reparse validation, dry-run diff output.
2. **Own resolution where LSPs are weak** (the eight): `locals.scm` scope trees for
   shadowing-safe renames; weekend-sized string-keyed binders per config/markup language
   (Terraform address graph, Helm values paths, CSS↔HTML selectors, XML id/idref, Markdown
   anchors/links); cross-language reference index on top.
3. **Optional LSP delegation for the big 4**: `prepareRename` → `rename` → apply WorkspaceEdit,
   with capability checks and LSP diagnostics as post-edit verification. Without it, big-4
   renames are offered in "syntactic + scope-checked" mode with explicit safety caveats.
4. **Safety net for what nothing catches**: after any rename, textual sweep for the old name in
   strings/comments/templates across *all* languages; surface hits for review (string refs
   defeat both syntax and LSP).

## 5. Refactor × language-class matrix (what "all the standard refactors" concretely means)

| Refactoring | Big-4 code langs | Zig / Bash | Terraform/HCL | Helm/YAML | CSS/SCSS ↔ HTML/TSX | XML | Markdown |
|---|---|---|---|---|---|---|---|
| Rename | LSP-grade (delegated) or scope-checked | scope-checked (locals) | resource/var/local/module/output address, all refs | `.Values` path ↔ values.yaml; anchor/alias | class/id/custom-property/mixin across files | id/idref, namespace prefix | heading + all anchor links, link refs, footnotes |
| Extract variable | local/const | local/var | `locals {}` entry | YAML anchor / values entry | SCSS `$var` / CSS custom property from repeated value | — | link reference def from inline URL |
| Extract function | full data-flow extract | Zig fn; bash function | module from resource set | named template (`_helpers.tpl`) | mixin from repeated declarations | — | section → new file with link updates |
| Inline | variable + call | variable | `local.x` → uses | anchor → uses | `$var`/custom-prop → uses | — | ref link → inline |
| Move to file | with import updates | — | resources between `.tf` files (flat namespace = easy) | values restructure | rules between partials | — | section extraction |
| Change signature | CLI-native (LSP can't) | Zig fn params | **module variables add/remove/rename, propagated to call sites** | — | mixin params | — | — |
| Safe delete | via reference index | — | unused variables/outputs/locals | **unused values.yaml keys** | **dead selectors** (vs HTML/TSX usage) | unused ids | orphaned link defs |
| Organize imports | full | Zig `@import` | — | — | `@use`/`@import` ordering | — | — |

Bold entries are things no existing tool does; they fall out naturally from the cross-language
reference index.

## 6. Beyond refactors: entrypoints, flow analysis, call graphs

The analysis features and the refactorings share one substrate: the reference index that makes
rename safe is the Tier-0 layer of the flow graph.

### 6.1 What funveil already has (and its limits)

- **Call graph** (`src/analysis/call_graph.rs`): petgraph `DiGraph<FunctionNode, CallEdge>` with
  BFS `trace()`, DOT export, std-function filtering. Resolution is **pure string-name matching**:
  no scoping, no import resolution, same-named functions in different files are conflated into
  one node. Dynamic calls carry an `is_dynamic` flag but are never resolved. Good API shape
  (callers/callees/trace/format_tree), floor-level precision.
- **Entrypoint detection** (`src/analysis/entrypoints.rs`): five categories (Main, Test, Cli,
  Handler, Export) via naming conventions, attributes (`#[test]`, `#[tokio::main]`,
  `#[derive(Parser)]`), and file conventions (`main.tf`, `page.tsx`, `Chart.yaml`). Binary
  classification, no confidence score, heuristics hardcoded in Rust instead of a data catalog.

### 6.2 Data-flow precision tiers (what "where does this value come from/go" can mean)

| Tier | What it is | Existence proof |
|---|---|---|
| 0 | Name/symbol resolution only | SCIP/LSIF, stack-graphs (archived) |
| 1 | Syntactic def-use / on-demand slicing, no PDG | [srcSlice](https://github.com/srcML/srcSlice) (sliced the Linux kernel in ~20 min on an XML CST, [JSEP'14](https://www.cs.kent.edu/~jmaletic/papers/JSEP14.pdf)); [tree-climber](https://github.com/bstee615/tree-climber) (CFG + def-use **directly on tree-sitter**) |
| 2 | Intra-procedural CFG dataflow on a language-agnostic IL | **Semgrep CE**: constant propagation + taint over an IL from tree-sitter parses ([data-flow docs](https://docs.semgrep.dev/writing-rules/data-flow/data-flow-overview)); intraprocedural, no path sensitivity |
| 3 | Reaching-defs + query-time inter-procedural traversal with call summaries | **Joern**: GEN/KILL fixpoint → `REACHING_DEF` edges, `reachableBy` walks CALL edges at query time; unresolved calls over-approximated unless a `FlowSemantic` summary narrows them ([docs](https://docs.joern.io/dataflow-semantics/)) |
| 4 | Global, field/context-sensitive dataflow on compiler-grade extraction | CodeQL, Semgrep Pro, needs a compiler frontend; CodeQL's engine is **proprietary/unembeddable** ([license](https://github.com/github/codeql-cli-binaries/blob/main/LICENSE.md)) |

**Achievable target:** Tier 2–3 for the imperative languages (Semgrep proves the parser stack;
Joern proves the architecture works without types for dynamic languages). The known cost of no
compiler is **call boundaries**, so: intra-procedural reaching-defs everywhere, inter-procedural
tracing as query-time traversal that **downgrades confidence loudly at unresolved call edges**,
plus Joern-style summaries for stdlib/framework functions.

**Config languages get different, and fully solvable, flow semantics. Substitution/override
provenance**, since each has a deterministic evaluation model:
- Terraform: `var`/`local`/`module.out` substitution is a true value DAG. Checkov implements it
  completely with attribute-labeled edges and multi-pass rendering
  ([local_graph.py](https://github.com/bridgecrewio/checkov/blob/main/checkov/terraform/graph_builder/local_graph.py));
  its one flaw: it substitutes in place, destroying the hop chain. We must keep it.
- Helm: 4-level override precedence + coalescing ([docs](https://helm.sh/docs/chart_template_guide/values_files/));
  [helm-ls](https://github.com/mrjosh/helm-ls) already resolves `.Values.x` → values files.
- CSS: the cascade **is** a spec'd provenance algorithm (origin → layer → specificity → order);
  DevTools' struck-through-losers view is the reference UX.
- YAML: anchors are discarded post-composition per spec, provenance must be captured
  pre-composition, which a CST-based tool does naturally (advantage over yq-style tools).
- Markdown: [marksman](https://github.com/artempyanykh/marksman) does link resolution; HTML
  id/`for` reference resolution is a genuine gap (vscode-html-languageservice doesn't do it).

### 6.3 Unified graph model (steal from Joern + Checkov)

One directed, edge-labeled property multigraph; shared nodes, **separate edge layers** built
incrementally per language, queries degrading gracefully when a layer is absent (Joern CPG's key
design, [cpg.joern.io](https://cpg.joern.io/)):

- **Nodes**: `File`, `Symbol` (kind: function/type/var/param/css-rule/tf-block/helm-key/
  yaml-anchor/md-heading…, byte range, language), `CallSite`, `Reference`, `Value`.
- **Edge layers**: `DECLARES`/`REF` (scope resolution, also powers rename) · `IMPORTS` ·
  `CALLS` (with `resolution: exact | import-qualified | field-based | name-only` + candidate
  count) · `DFLOW` (intra-procedural reaching-defs; inter-procedural at query time) ·
  `PROVENANCE` (config langs; `kind: substitution|override|expansion|default`, precedence
  metadata on competing edges, **hop chains preserved**).

Backward flow = walk DFLOW/PROVENANCE to sources surfacing override decisions; forward flow =
reverse. The layers stitch across the code/config boundary (Helm value → env var →
`os.environ` read).

### 6.4 Call-graph resolution per language

The literature's headline: **precision is cheap, recall dies on dynamic features**; the unsound
field-based heuristic (bucket call targets by method/property name, [Feldthaus et al.
ICSE'13](https://www.franktip.org/pubs/icse2013approximate.pdf)) gets ~66–80% precision / ≥85%
recall on JS with no types, the single most effective technique.

| Language | Strategy | Notes |
|---|---|---|
| Go | package-qualified names + CHA-style all-implementors for interface calls | gopls call hierarchy *omits dynamic calls by design*, own analysis can beat LSP recall |
| Rust | direct calls + impl-block tracking; `dyn`/fn-pointer sites → multi-candidate | static-only misses ~29% of edges ([Rupta, CC'24](https://dl.acm.org/doi/10.1145/3640537.3641574)) |
| TS/TSX | field-based (ACG) + explicit type annotations to narrow | module-graph tools (madge) are not function-level |
| Python | **skip pure name-matching** (over-links badly); PyCG-style assignment graph | PyCG: 99.2% precision / 69.9% recall ([ICSE'21](https://arxiv.org/abs/2103.00587)), archived; successor: Jarvis |
| Zig | name + `@import` resolution; comptime sites flagged unresolved | zls has no callHierarchy, trivially best-in-class opportunity |
| Bash | function-name-in-command-position + static `source` resolution | bash-language-server has no callHierarchy either |

Every `CALLS` edge carries its resolution-confidence tag (compare Sourcegraph's
precise vs search-based split). LSP callHierarchy stays an optional precision backend, not the
core: whole-program extraction is O(2 requests/function), so Sourcegraph abandoned
LSP for compiler-based SCIP indexers.

### 6.5 Entrypoint catalog

Funveil's five categories survive, but detection moves from hardcoded Rust heuristics to
**flat declarative per-framework YAML catalogs**, following CodeQL Models-as-Data
(`sourceModel(package, type, name, kind, provenance)` rows,
[docs](https://codeql.github.com/docs/codeql-language-guides/customizing-library-models-for-java-and-kotlin/))
and [OWASP noir](https://github.com/owasp-noir/noir) (~193 frameworks incl. Rust/Zig/Go/Python/TS,
proving syntax-only endpoint extraction scales). Schema: `kind` (http-route, cli-main,
env-read, queue-consumer, scheduled-job, exported-api, test, infra-exposure, infra-input, …) ×
orthogonal `threat_model` (remote/local) × per-language `match` block × `provenance:
manual|generated`. For infra languages, entrypoints are externally-settable inputs (root-module
tfvars, values.yaml keys) and declared network exposure (Service/Ingress, 0.0.0.0/0 ingress).
Entrypoints become tagged Symbol nodes, seeds for reachability and forward-flow queries.
Semgrep's per-rule duplication of source definitions is the anti-pattern to avoid.

### 6.6 Licensing constraints

CodeQL engine: proprietary, unembeddable. Safe design references: Semgrep/Opengrep (LGPL-2.1),
Joern (Apache-2.0), Checkov (Apache-2.0). Fits an AGPL-3.0 tool like funveil.

## 7. Prior art shortlist

[ast-grep](https://github.com/ast-grep/ast-grep) · [GritQL](https://github.com/getgrit/gritql)
([Biome fork](https://github.com/biomejs/gritql)) · [comby](https://comby.dev) +
[Sourcegraph batch changes](https://sourcegraph.com/docs/batch-changes/faq) ·
[Polyglot Piranha](https://github.com/uber/piranha)
([PLDI'24 paper](https://danieltrt.github.io/papers/pldi24.pdf)) ·
[OpenRewrite LST docs](https://docs.openrewrite.org/concepts-and-explanations/lossless-semantic-trees) ·
[gopls CLI](https://pkg.go.dev/golang.org/x/tools/gopls/internal/lsp/cmd) ·
[Serena/SolidLSP](https://github.com/oraios/serena) · [rope](https://rope.readthedocs.io) ·
[rust-analyzer SSR / ra_ap_ssr](https://docs.rs/crate/ra_ap_ssr/latest) ·
[LibCST](https://github.com/Instagram/LibCST) · jscodeshift/[recast](https://github.com/benjamn/recast)
