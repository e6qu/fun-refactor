# Cross-language analysis and translation

`fr` follows declared relationships across supported language boundaries.
Reference confidence decides which occurrences a mutation may rewrite.
A configuration trace, a resolved reference and a translated function provide different evidence.

## Current sample

The bundled corpus exercises multiple languages in one workspace:

    web/sample (27 files, 17 languages, 596 resolved references)
            html -> css          18   selector
             tsx -> css           2   selector
             tsx -> typescript    8   function 6, interface 2

`tests/docs_census.rs` measures these figures.
Use `fr symbols --stats --json` to inspect index coverage in another workspace.
Use `fr refs`, `fr usages` and `fr stitch` for individual relationships.

Helm values and templates both use the Helm language label.
Their relationship crosses file roles even when the language census records no crossing.

## Permitted reference boundaries

`lang::may_resolve_across` limits the candidate languages before resolution.
A matching name in an unrelated language supplies no reference evidence.

| From | To | For | Why it is real |
| --- | --- | --- | --- |
| any | itself | everything | the ordinary case |
| TypeScript | TSX, and back | everything | TSX *is* TypeScript with JSX; a `.tsx` imports from a `.ts` constantly |
| CSS | SCSS, Sass, and back | everything | both compile to CSS and the three share one selector namespace |
| SCSS | Sass, and back | everything | one language, two syntaxes: the braced one and the indented one |
| HTML, XML, TSX, TypeScript, Markdown | CSS, SCSS, Sass | selectors, custom properties | markup names a style rule by class or id |
| Helm | YAML, and back | keys | a template names a key in its values file |
| HTML, XML, TSX, TypeScript | HTML, XML | element ids | a template names an element the markup declares |
| HTML, TSX, TypeScript | HTML, TSX | `data-*` hooks | a test and a component agree on `data-testid="submit-btn"` by string |

Rust-to-Zig and Go-to-Python FFI bindings need build and ABI facts that the current resolver does not read.
The index keeps these boundaries unresolved.

## Current relationships and limits

| Relationship | Current behavior | Remaining limit |
|---|---|---|
| CSS modules to TSX | Resolve an imported selector in its stylesheet; keep module selector identities separate | Dynamic member names require review |
| DOM accessors to markup | Recognize selected literal ids and selectors; report name-only evidence | Compound selectors and runtime strings do not establish rewrite permission |
| Manifest environment to code | `fr stitch` traces values and their readers | Traces do not authorize renaming external runtime names |
| Shell flags to declarations | `fr stitch --flags` recognizes supported flag frameworks | Arbitrary shell argument parsers remain outside its scope |
| CI steps to scripts | `fr stitch --files` resolves declared workspace paths | Commands such as `make` need build-system knowledge |
| Terraform to files | Follow `file`, `filebase64` and `templatefile` paths | Template substitution needs target-specific semantics |
| Markdown to source | Follow file links and report textual symbol occurrences | Prose mentions supply no safe rewrite evidence |

`exact` and `import-qualified` references permit structural rewrites.
Weaker references and textual occurrences remain visible for review.
A framework relationship can add useful context without increasing its rewrite confidence.

## Translation through the code IR

`fr translate` converts supported programming constructs through [the IR](IR.md).
Rust, Go, Java, Python, TypeScript, Zig, Bash and Lean have readers and writers.
TSX uses the TypeScript reader where its constructs have an IR representation.
Separate conversions handle supported configuration and markup formats.

Each translation reports its fidelity and constructs that require manual work.
A supported target does not imply that every dependency or runtime behavior can migrate.
The suite checks syntax, compilation and execution across its fixtures and pinned corpora.
Those checks establish evidence for their cases rather than general semantic equivalence.

## Framework and project transformations

Next.js API routes and server functions can become FastAPI handlers.
FastAPI endpoints can become a Next.js route tree.
OpenAPI documents can generate skeletons for either target.
[API_CONTRACTS.md](API_CONTRACTS.md) describes the current contract surface and limits.

The [active roadmap](PLAN.md) adds a project and framework model alongside the code IR.
It will connect packages, services, route groups and frontend component trees to source symbols.
An agent will select a feature, inspect a feasibility report and apply a checked migration plan.
Authentication, middleware, state, effects and dependency boundaries need explicit adapter semantics.
Whole-project and frontend-framework migration remain planned work.

## Related documents

- [CLI.md](CLI.md): command options and write guarantees.
- [RECIPES.md](RECIPES.md): composing existing operations.
- [IR.md](IR.md): supported representation and fidelity reports.
- [docs/lean-specs.md](docs/lean-specs.md): formal models and implementation correspondence.
