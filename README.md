# fun-refactor

`fr` finds and changes code across 17 languages. One binary carries the whole
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

| Capability | rust | go | zig | java | typescript | tsx | python | bash | html | css | scss | sass | hcl | yaml | helm | xml | markdown |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| symbols/def/refs | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| rename | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| safe delete | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| impact | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| restructure | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| call graph | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| flow | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| provenance | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a |
| entry points | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | ✓ | n/a | ✓ | ✓ | ✓ |
| extract variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| extract function | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | n/a | n/a | ✓ | n/a | n/a |
| inline variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| inline call | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| change signature | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a |
| micro-rewrites | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| organize imports | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| remove flag | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | n/a | n/a | n/a | n/a |
| move to file | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ |
| config→code stitch | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | n/a | n/a |
| duplicate code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| dead code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| write as another language | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | ✓ | n/a |
| declared HTTP contract | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| declared type | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |

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
fr recipe <file.recipe>       # a refactoring written down: find, do, expect
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

Every command takes `--json`. Every mutation prints a diff and changes nothing until
you add `--write`, and it then applies a multi-file change atomically.

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

```
cargo install --path .
```

`./tools/check.sh` runs everything CI runs: formatting, clippy, the tests, the
capability report and the prose check. It runs them twice, once for the default build
and once with the browser API compiled in. CI calls the same script, so a pass here
and a pass there mean the same thing.

## Third-party material

`vendor/` holds the upstream tree-sitter query files the rules in `queries/` were
derived from, each with its licence and a checksum in `vendor/MANIFEST.toml`. The
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

Query files hold what `fr` knows about a language, and Rust holds none of it.
`queries/<lang>/facts.scm` declares the definitions, references, scopes and imports
of one language. `src/extract.rs` documents the names a query may attach to a node.
YAML files in `catalogs/` carry the rules for entry points. To add a language or a
framework, add data. A language whose published grammar cannot read it needs one thing
more: a patched copy under `grammars/`, which that directory's README explains.

## Status

Only one stage of [PLAN.md](PLAN.md) remains open, the optional LSP delegation
backend. The tool builds every capability a language can meaningfully support:
**286 of 408 capability × language pairs supported, 122 not applicable, none refused.**
The code generates the matrix above, and `fr capabilities` prints the reason behind
every cell that is not a ✓.

[TUTORIAL.md](TUTORIAL.md) walks through one real repository, helm/helm, and shows
the output each command produced. The [project
site](https://e6qu.github.io/fun-refactor/demo.html) steps through the same session.

[EXAMPLES.md](EXAMPLES.md) holds one example for each capability. Each one ran
against a public repository at a fixed commit: ripgrep, requests, helm,
terraform-aws-vpc, zls and grafana. It also lists what the tool does not do, and what
each of those would take.

Three documents cover the work that spans more than one language at a time:

- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md) for what a name crossing a language
  boundary can and cannot prove.
- [API_CONTRACTS.md](API_CONTRACTS.md) for rewriting a service while preserving the
  contract its callers see.
- [RECIPES.md](RECIPES.md) for the recipe language `fr recipe` runs.

[BUGS.md](BUGS.md) tracks the open limitations, and the tool reports each one to you
instead of answering it wrongly in silence. One stands open: reachability through a
function value nothing in the workspace names.

Where a published grammar could not read source the language accepts, this build
compiles a patched copy instead: `grammars/` holds one for Go, Python, Sass, SCSS,
TypeScript and Zig, each with its upstream pin, licence, patch and the corpus measurement
showing the patch changes no tree the published parser already read.

One piece remains unbuilt: the optional LSP delegation backend. (The plan also lists
a watch-mode daemon, though the fact cache already recovers most of what it would
have saved.)

## Licence

AGPL-3.0-or-later.
