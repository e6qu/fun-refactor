# fun-refactor

`fr` finds and changes code across 19 languages. One binary carries the whole
program, and it pulls in no dependencies. It runs without a language server, a
background process or a configuration file.

It reads source files with [tree-sitter](https://tree-sitter.github.io), a parser
library that carries a grammar for each language. It shares those grammars with
[funveil](https://github.com/e6qu/funveil).

```
fr rename btn-primary btn-cta
```

That command renames a CSS class. It rewrites the CSS rule that declares the class,
every HTML `class` attribute that uses it, and every TSX `className` property that
names it. One rename crosses three languages and three grammars. A language server
reads one language at a time, so it cannot follow a name across that boundary.

New here? Read [docs/terminology.md](docs/terminology.md) for the words this project
uses. [TUTORIAL.md](TUTORIAL.md) walks through a real repository.

For an agent handoff, use the portable [fr skill](skills/fr/SKILL.md).
[Agent skill validation](docs/agent-skill.md) describes its task-specific references, executable examples and measured limits.
[Function authoring](docs/body-authoring.md) adds bounded Rust, Go, TypeScript and TSX implementation changes through project handles and source-history transactions.
[Project checks](docs/project-checks.md) selects declared validation commands and reports bounded results and coverage claims.
After reviewing the listing, add `--no-declarations` to `checks --run` to omit repeated command metadata while retaining execution outcomes and diagnostics.
[Development continuity](docs/continuity.md) records the active milestone, evidence and remaining work for the next session.
The portable skill starts targeted edits with authoring guidance and loads exploration or interrupted-write recovery when needed.
`project find --source --bytes N` combines name lookup with source slices under one shared page budget.
[Source kernel proofs](docs/lean-specs.md#bounded-source-kernels) cover modeled UTF-8 slicing and shared budgets, with Rust and CLI comparisons.
[Cache measurements](docs/project-context-evaluation.md#query-time-and-the-fact-cache) compare query time while checking identical reports and source invalidation.
[Release profiling](docs/project-context-evaluation.md#release-stage-profiling) identifies project construction as the largest remaining stage in the measured cached lookups.
[Batched revision hashing](docs/project-context-evaluation.md#batched-revision-hashing) reduces allocation and hash-update overhead while checking identical reports and stale-source refusals.
[Revision buffer proofs](docs/lean-specs.md#revision-buffer-kernels) cover ordered bytes, failed writes and flush schedules, with shared Rust/Lean execution checks.
[Real-agent acceptance](docs/agent-acceptance.md) records the first paired trials and reversible patches.
The [context-reduction follow-up](docs/agent-context-followup.md) measures targeted lookup, quiet successful checks and selective skill loading against fresh file-tool trials.
Use `fr project find NAME --signature` to locate a known declaration without requesting a broad map.
After reviewing a source transaction, `history apply`, `undo`, `redo` and `recover` accept `--write --no-diff` for smaller completion reports.
The [workspace evaluation](docs/agent-workspace-evaluation.md) records four passing trials on the larger regex repository, with context comparisons and replayable patches.
Rust function declaration replacement also supports combined signature and implementation changes.
Declaration insertion adds a Rust function through a file or inline module handle while retaining existing code.
Authoring batches coordinate disjoint edits across files through one reviewed source-history transaction.
The [controlled batch comparison](docs/project-context-evaluation.md#coordinated-authoring-measurement) measures command and payload costs while checking behavior and exact reversal.
The [coordinated workspace task](docs/agent-workspace-evaluation.md#coordinated-task-preparation) prepares evaluation of a change spanning regex and regex-syntax.
The [coordinated agent evaluation](docs/agent-coordinated-evaluation.md) records the paired trials of that task.

## Why

Language servers work well for the four largest ecosystems. Elsewhere you find a thin
one, or nothing. `terraform-ls` cannot rename. `bash-language-server` cannot
rename. `zls` returns no edits. Nothing renames HTML, CSS, XML, Markdown or Helm to
that standard.

None of them sees a reference that crosses languages, because each reads a single
language. Three examples: a CSS class named in a JSX property, a Helm values key read
in a template, a Terraform variable passed through modules.

`fr` covers that gap. [RESEARCH.md](RESEARCH.md) holds the survey behind the design,
with sources.

## Languages

`fr capabilities --markdown` generates this table from the code, so the two cannot
disagree. **✓** means the command works for that language. **n/a** means the
operation has no meaning there, and the table carries the reason. Run
`fr capabilities` to read the reason for every cell that is not a ✓.

`fr recipe` has no row. It runs the steps you write, so it answers for a language
whatever those steps answer.

JavaScript has no row. The `typescript` grammar reads `.js`, `.mjs` and `.cjs`, and
the `tsx` grammar reads `.jsx`.

| Capability | rust | go | zig | java | typescript | tsx | python | bash | html | css | scss | sass | hcl | json | yaml | helm | xml | markdown | lean |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| symbols/def/refs | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| rename | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| safe delete | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| impact | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| restructure | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| call graph | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ |
| flow | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ |
| provenance | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| entry points | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | ✓ | n/a | n/a | ✓ | ✓ | ✓ | n/a |
| extract variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a |
| extract function | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | n/a | n/a | n/a | ✓ | n/a | n/a | n/a |
| inline variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a |
| inline call | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| change signature | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | ✓ |
| micro-rewrites | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| organize imports | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| remove flag | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a |
| move to file | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ |
| config→code stitch | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | n/a | n/a | n/a |
| duplicate code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| dead code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| write as another language | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| declared HTTP contract | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| declared type | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |

**No cell is blank for want of work.** Every cell either reports support or carries
the reason the operation has no meaning there. HTML has no way to bind a value. A
stylesheet has no entry point. A document does not import the elements of another.

## Commands

```
fr cache                      # where cached facts live, and how big
fr scan                       # files it can act on
fr parse --stats              # syntax health per language
fr symbols --kind function    # what is defined
fr def <name|path:line:col>   # where is it defined, every one of them
fr implementations <target>   # the concrete implementations of an abstraction
fr usages <target>            # every use, grouped by file, with its context
fr refs <target>              # where is it used, with confidence per site
fr rename <target> <new>      # rename it and everything that points at it
fr extract <path:l:c-l:c> <n> # extract an expression into a binding
fr extract <range> <n> --function   # extract statements into a function
fr inline <target>            # replace a variable's uses with its value
fr inline <path:l:c> --call   # replace a call with a one-expression body
fr signature <target> remove:1  # change parameters, update every call site
fr move <target> <dest-file>  # move a symbol, update imports
fr delete <target>            # delete it, refusing if anything uses it
fr unused                     # symbols nothing appears to use
fr unused --lang go --internal   # ...only what is definitely dead here
fr duplicates                 # code written more than once, by structure
fr duplicates --exact         # ...requiring the names to match as well
fr imports <file>             # drop unused imports, sort the rest
fr restructure 'old($X)' 'new($X)' --lang rust
fr restructure 'A | B' 'A | B | C' --lang rust
                              # a member, an arm, or a run of macro tokens
fr remove-flag USE_NEW --value true  # and everything that only served it
fr rewrite <path:l:c>         # list local transformations that apply here
fr rewrite <path:l:c> guard-clause   # ...and apply one
fr translate <file> [language]  # write it as another language, or `fastapi`
                              #   (routes and "use server" modules both)
fr translate app.py nextjs    # a FastAPI application as a Next.js route tree
fr translate openapi.yaml fastapi  # a service skeleton from a contract
fr recipe <file.recipe>       # a workspace transaction: recipes find, do, expect together
fr recipe fmt recipes --check # format every recipe in a directory, or reject drift
fr spec check                 # Lean models whose source anchors still match
fr spec sync --write          # renew reviewed stale source hashes
fr spec verify                # strict correspondence plus Lake builds
fr openapi [--yaml]           # the contract a Next.js route tree declares
fr callers <fn> --depth 3     # who calls this
fr callees <fn> --depth 3     # what does it call
fr graph --dot                # the call graph
fr flow back <target> [-f values.yaml] [--set a.b=c]
                              # where did this value come from
fr flow fwd <target>          # where does it go
                              #   (def-use for code, substitution/override
                              #    provenance for config languages)
fr impact <target>            # everything a change could affect
fr stitch                     # config values traced into the code reading them
fr entrypoints --kind http-route
```

Every command takes `--json`. Source refactorings preview their diff; `--write` applies it and `--save-plan` records a plan.
`checks --run` executes declared project commands outside source history.
[CLI.md](CLI.md#write-guarantees) states the commit and recovery guarantees and command-specific exceptions.

`fr` indexes files in parallel and caches the facts it extracts by file content and
query set. A repeated command therefore re-reads only what changed, roughly 1.7×
faster cold and 3–5× warm. On the one repository we timed, a warm index cost on the
order of 30 ms per file; take the magnitude, not the number. `--no-cache` bypasses
the cache, and `fr cache --clear` empties it.

## What it will not do

The tool measures how sure it is, and says so. It does not guess and sound certain.

**Every resolved reference carries a confidence tier.** The four tiers are `exact`,
`import-qualified`, `field-based` and `name-only`. A change rewrites the top two
tiers only. The tool reports anything weaker for you to check.

- **Renames report, never rewrite, occurrences in strings and comments.** They
  defeat syntax analysis and language servers alike.
- **`fr` refuses an ambiguous name and lists** the candidates.
- **Change-signature refuses entirely** if any call site is unproven, because
  updating a subset does not compile.
- **Flow analysis stops loudly** at function boundaries, unresolved calls and weak
  resolutions instead of over-approximating through them.
- **`fr` writes no edit that breaks the file.** It reparses each changed file,
  rejecting the whole operation if one gains a syntax error.

The tool never reformats a file. Each edit replaces one range of bytes in the
original source. Comments, spacing and trailing whitespace outside that range stay
as they were, including inside an expression that the tool extracts.

## Install

Every release carries a built binary. Take the one for your machine, check it
against the `.sha256` beside it, and put `fr` on your path.

| Archive | Machine |
|---|---|
| `fr-<tag>-x86_64-unknown-linux-musl.tar.gz` | Linux, amd64 |
| `fr-<tag>-aarch64-unknown-linux-musl.tar.gz` | Linux, arm64 |
| `fr-<tag>-x86_64-apple-darwin.tar.gz` | macOS, Intel |
| `fr-<tag>-aarch64-apple-darwin.tar.gz` | macOS, Apple silicon |
| `fun-refactor-<tag>-wasm.tar.gz` | A browser or Node, as a module |

The Linux binaries link against musl and are static, so they need nothing
installed and no particular distribution. The browser archive holds the
WebAssembly module and the JavaScript that loads it, the same pair the
[playground](https://e6qu.github.io/fun-refactor/playground/) runs.

```
shasum -a 256 -c fr-v0.2.0-aarch64-apple-darwin.tar.gz.sha256
tar -xzf fr-v0.2.0-aarch64-apple-darwin.tar.gz
./fr-v0.2.0-aarch64-apple-darwin/fr --version
```

Releases come from `release-please`. It reads the commits on `main` and keeps the
next version and its changelog in an open pull request. [CHANGELOG.md](CHANGELOG.md)
holds what each release carried. Merging that request tags
the release. The repository has to allow a workflow to open one: Settings,
Actions, General, Workflow permissions, "Allow GitHub Actions to create and
approve pull requests".

To build it yourself instead:

```
cargo install --path .
```

`./tools/check.sh` runs the native and WASM API PR checks, capability coverage, prose checks and Lean kernels.
A separate CI job builds and exercises the browser playground.
`./tools/check.sh deep` runs the repository-scale audits.

## Third-party material

`vendor/` holds the upstream tree-sitter query files behind the rules in
`queries/`, each with its licence and a checksum in `vendor/MANIFEST.toml`. The
build compiles nothing there. Those files serve as reference material, and as
evidence of where the rules came from. `cargo test --test vendor` fails in three
cases:

- A file changed and its manifest entry did not.
- A file arrived with no record of its source.
- A licence arrived that AGPL-3.0-or-later cannot include.

Run `python3 vendor/vendor.py` after you update a grammar, then read the diff. A
grammar that renames a node does not break the build. It makes a query stop matching,
and nothing reports that.

## Adding a language

`queries/<lang>/facts.scm` declares definitions, references, scopes and imports.
`src/extract.rs` documents the captures each query can produce.
YAML catalogs describe recognized entry points.

A new language also needs grammar integration and capability decisions.
Refactoring and translation rules may require Rust changes and language-specific validation.
Framework migration needs adapters for its runtime and project conventions.
See `grammars/README.md` for grammar provenance and local patches.

## Status

The generated matrix records **311 of 456 capability × language pairs supported, 145 not applicable**.
Supported operations still report input-specific limitations and confidence.

[PLAN.md](PLAN.md) is the active roadmap for agents: compact project understanding,
reversible changes, Git patches, reusable Lean verification and hierarchical framework migration.
The original implementation stages are complete. The new milestones remain active.
LSP delegation stays outside the default engine; daemon/watch mode awaits a measured need.

The shared commit path recovers earlier writes after a handled failure and reports recovery problems.
The native CLI now saves plans and supports checked apply, undo, redo and interrupted-write recovery through `fr history`.
`fr history patch ID` exports stored text changes for Git, with reverse export and optional JSON metadata.
Add `--check` to compare the receiving files with the recorded starting state; `--against DIR` selects another workspace.
Use `--git-check` for Git's application verdict, with `--index` to include the index.
Patch-basis and executable-mode helpers have anchored Lean models with 55,358 shared execution comparisons.
See [recorded transaction patches](docs/git-patches.md) for application checks, mode scope and limitations.
`fr file delete` and `fr file executable --set on|off` add explicit file operations with saved plans, undo/redo and patch export.
See [file transactions](docs/file-transactions.md) for owner-execute semantics and validation scope.
`fr git status` pages through repository changes with filters, rename sources and continuation cursors.
See [Git status](docs/git-status.md) for observation limits, omitted submodules and configuration scope.
`fr git changes` discovers changed paths and line counts across the repository, including comparisons since a commit.
See [repository change pages](docs/git-changes.md) for scope and metadata identity.
`fr git diff PATH` pages through hunks and capped source excerpts, with staged and commit-based comparisons.
Add `--symbols` for changed declarations and their containing hierarchy, with snapshot checks and no source bodies.
Add `--calls` for incoming and outgoing candidates within each file snapshot, retaining confidence and unresolved targets.
Repeat `--include FILE` to add explicit caller and target context, with selected blob bases and checked raw working snapshots.
See [Git diff details](docs/git-diff.md) for cursor identity and supported paths.
`fr git stage PATH...` previews raw staging entries; `--basis TOKEN --write` applies them through a prepared index on Unix.
See [staging semantics and limits](docs/git-staging.md) for raw byte and mode semantics.
`fr git stage-history` inspects staging records and previews checked undo, redo and recovery; see [staging history](docs/git-stage-history.md).
`fr git commit -m MESSAGE` previews the entire index; `--basis TOKEN --write` publishes it after index and HEAD checks.
See [reviewed commits](docs/git-commit.md) for identity configuration, disabled hooks/signing and publication limits.
`fr git worktree list` pages through registered workspaces, branches, HEADs and lock metadata.
See [worktree inspection](docs/git-worktrees.md) for observation limits and cursor identity.
`fr git worktree create PATH --branch NAME` previews a fresh raw checkout; `--basis TOKEN --write` creates it on Unix.
See [reviewed worktree creation](docs/git-worktree-creation.md) for branch checks and partial-failure outcomes.
Use `--existing-branch NAME` for an unused local branch; [existing-branch checkout](docs/git-worktree-existing-branches.md) preserves its tip and configuration.
`fr git worktree recover PATH` previews completion of a recorded incomplete checkout, with checked application through `--basis TOKEN --write`.
See [recorded recovery](docs/git-worktree-recovery.md) for ownership receipts and preservation rules.
`fr git worktree remove PATH` previews removal of a clean owned worktree; `--basis TOKEN --write` archives metadata and removes reviewed files.
[Reviewed removal](docs/git-worktree-removal.md) retains the branch and refuses extra content.
[Removal resumption](docs/git-worktree-removal-resumption.md) uses `fr git worktree resume-removal RECORD` to inspect partial removals and `--basis TOKEN --write` to finish them.
[Archive compaction](docs/git-worktree-archive-compaction.md) uses `fr git worktree compact-removal RECORD` to review discarding completed recovery records while retaining audit summaries.
The browser already exports patches and can restore its initial workspace.
`fr project` now provides compact hierarchy maps and bounded source inspection.
Its package and dependency pages report Cargo/npm manifest declarations with shared revision checks.
`fr project links` adds local manifest links and workspace pattern candidates with explicit unresolved cases.
`fr project workspaces` adds observed Cargo membership and supports inherited local dependency links.
Cargo exclusions use literal path prefixes, with matching literal member prefixes taking precedence.
Glob-shaped exclusions produce unresolved rows.
Cargo member patterns can start with `../` within the selected snapshot; observed sibling packages still need ownership evidence.
Its expansion helper has an anchored Lean model with finite convergence and exact closure laws.
Shared graph cases test Rust correspondence; Cargo interpretation remains outside the proofs.
`fr project calls` and `implementations` page through call sites and hierarchy candidates, retaining uncertainty and coverage gaps.
`fr project routes` adds bounded declarations and handler candidates for five pattern readers, Next.js App Router exports and direct FastAPI decorators.
Next.js candidates include local function export aliases and terminal catch-all paths; contract rows retain catch-all cardinality.
Direct variable handlers expose initializer annotations. Nested app candidates retain captured npm dependency and package-boundary evidence.
`fr project contracts` adds paged path parameters, Axum/Spring request type candidates and declared handler return types, with explicit gaps.
FastAPI contract rows include explicit parameter markers and decorator response models, with separate return annotations and no inferred wire schemas.
`fr project schemas` pages Python class, TypeScript interface/object-alias and Rust struct fields, with followable candidates and explicit validation gaps.
`fr project contracts --types` adds optional declaration links from supported signature types, preserving ambiguity and unresolved names.
`fr project configuration` pages environment declarations and candidate code consumers, with captured-source checks and explicit analysis gaps.
`fr project tests` adds catalog candidates and bounded call-path witnesses, preserving the weakest edge confidence without claiming runtime coverage.
Complete dependency graphs, framework semantics and broader task evaluations remain roadmap work.

[TUTORIAL.md](TUTORIAL.md) walks through helm/helm.
[EXAMPLES.md](EXAMPLES.md) shows capabilities on pinned public repositories.
[CLI.md](CLI.md) documents the implemented commands.
[Project view evaluation](docs/project-context-evaluation.md) measures compact output and records the remaining task-level checks.

- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md) explains reference and translation boundaries.
- [API_CONTRACTS.md](API_CONTRACTS.md) describes HTTP contract extraction and route conversion.
- [RECIPES.md](RECIPES.md) defines the local recipe language.
- [IR.md](IR.md) describes the intermediary language and translation evidence.
- [docs/lean-specs.md](docs/lean-specs.md) separates implemented Lean checks from the adoption roadmap.

Lean readers, writers, edit and position models, and `spec check`, `sync` and `verify` exist.
Strict signature correspondence currently accepts Rust source declarations.
Model proofs and shared execution cases establish different kinds of evidence; neither alone proves every Rust behavior.

[BUGS.md](BUGS.md) records known limitations and fixed defects.
B5 tracks reachability that available source evidence cannot settle.
The list records known findings; it does not establish the absence of other defects.

`grammars/` holds grammar sources and provenance, including the Lean grammar and patched upstream grammars.
The build uses these where the published grammar cannot read supported source forms.

## Licence

AGPL-3.0-or-later.
