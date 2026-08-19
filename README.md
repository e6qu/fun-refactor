# fun-refactor

`fr` finds and changes code across 16 languages. It is one program with no
dependencies. It needs no language server, no background process and no
configuration file.

It reads source files with [tree-sitter](https://tree-sitter.github.io), a parser
library with a grammar for each language. It shares its grammars with
[funveil](https://github.com/e6qu/funveil).

```
fr rename btn-primary btn-cta
```

That command renames a CSS class. It rewrites the CSS rule that declares the class,
every HTML `class` attribute that uses it, and every TSX `className` property that
names it. Three languages, three grammars, one name. A language server sees one
language at a time, so it cannot follow a name across that boundary.

New here? Read [docs/terminology.md](docs/terminology.md) for the words this project
uses. [TUTORIAL.md](TUTORIAL.md) walks through a real repository.

## Why

Language servers work well for the four largest ecosystems. Elsewhere they are
missing or thin. `terraform-ls` cannot rename. `bash-language-server` cannot rename.
`zls` returns no edits. Nothing of rename quality exists for HTML, CSS, XML, Markdown
or Helm.

A reference that crosses languages is invisible to all of them, because each one
reads a single language. Three examples: a CSS class named in a JSX property, a Helm
values key read in a template, a Terraform variable passed through modules.

This tool covers that gap. [RESEARCH.md](RESEARCH.md) holds the survey behind the
design, with sources.

## Languages

`fr capabilities --markdown` generates this table from the code, so the two cannot
disagree. **✓** means the command works for that language. **n/a** means the
operation has no meaning there, and the table carries the reason. Run
`fr capabilities` to read the reason for every cell that is not a ✓.

`fr recipe` has no row. It runs the steps you write, so its answer for a language is
whatever those steps answer.

JavaScript has no row. The `typescript` grammar reads `.js`, `.mjs` and `.cjs`, and
the `tsx` grammar reads `.jsx`.

| Capability | rust | go | zig | java | typescript | tsx | python | bash | html | css | scss | hcl | yaml | helm | xml | markdown |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| symbols/def/refs | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| rename | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| safe delete | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| impact | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| restructure | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| call graph | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| flow | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| provenance | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a |
| entry points | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | n/a | ✓ | ✓ | ✓ |
| extract variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| extract function | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | n/a | n/a | ✓ | n/a | n/a |
| inline variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| inline call | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| change signature | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a |
| micro-rewrites | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| organize imports | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| remove flag | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | ✓ | n/a | n/a | n/a | n/a |
| move to file | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ |
| config→code stitch | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | ✓ | n/a | n/a |
| duplicate code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| dead code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| write as another language | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | n/a |
| declared HTTP contract | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| declared type | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |

**No cell is blank for want of work.** Each one is either supported or carries the
reason the operation has no meaning there. HTML has no way to bind a value. A
stylesheet has no entry point. A document does not import the elements of another.

## Commands

```
fr cache                      # where cached facts live, and how big
fr scan                       # files it can act on
fr parse --stats              # syntax health per language
fr symbols --kind function    # what is defined
fr def <name|path:line:col>   # where is it defined — every definition
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
fr remove-flag USE_NEW --value true  # and everything that only served it
fr rewrite <path:l:c>         # list local transformations that apply here
fr rewrite <path:l:c> guard-clause   # ...and apply one
fr translate <file> [language]  # write it as another language, or `fastapi`
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

Every command takes `--json`. Every mutation prints a diff and changes nothing
unless you add `--write`, and a multi-file change is applied atomically.

Files are indexed in parallel, and extracted facts are cached by file content and
query set. So a repeated command re-reads only what changed, roughly 1.7× faster
cold and 3–5× warm. On the one repository we timed, a warm index cost on the order
of 30 ms per file; take the magnitude, not the number. `--no-cache` bypasses the
cache; `fr cache --clear` empties it.

## What it will not do

The tool measures how sure it is, and says so. It does not guess and sound certain.

**Every resolved reference carries a confidence tier.** The four tiers are `exact`,
`import-qualified`, `field-based` and `name-only`. A change rewrites the top two
tiers only. The tool reports anything weaker for you to check.

- **Renames report, never rewrite, occurrences in strings and comments.** They
  defeat syntax analysis and language servers alike.
- **An ambiguous name is refused with a listing** of the candidates.
- **Change-signature refuses entirely** if any call site is unproven, because
  updating a subset does not compile.
- **Flow analysis stops loudly** at function boundaries, unresolved calls and weak
  resolutions instead of over-approximating through them.
- **No edit is written if it would break the file.** Every changed file is reparsed
  and the whole operation is rejected if it gains a syntax error.

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
derived from, each with its licence and a checksum in `vendor/MANIFEST.toml`. Nothing
there is compiled. It is reference material, and evidence of where the rules came
from. `cargo test --test vendor` fails in three cases. A file changed and its manifest
entry did not, a file arrived with no record of its source, or a licence arrived that
AGPL-3.0-or-later cannot include.

Run `python3 vendor/vendor.py` after you update a grammar, then read the diff. A
grammar that renames a node does not break the build. It makes a query stop matching,
and nothing reports that.

## Adding a language

Knowledge about a language lives in query files, and not in Rust.
`queries/<lang>/facts.scm` declares the definitions, references, scopes and imports
of one language. `src/extract.rs` documents the names a query may attach to a node.
Rules for entry points are YAML files in `catalogs/`. To add a language or a
framework, add data.

## Status

Every stage of [PLAN.md](PLAN.md) is complete except the optional LSP delegation
backend. Every capability a language can meaningfully support is built:
**273 of 384 capability × language pairs supported, 111 not applicable, none refused.**
The matrix above is generated, and `fr capabilities` prints the reason behind every
cell that is not a ✓.

[TUTORIAL.md](TUTORIAL.md) walks through one real repository, helm/helm, and shows
the output each command produced. The [project
site](https://e6qu.github.io/fun-refactor/demo.html) steps through the same session.

[EXAMPLES.md](EXAMPLES.md) holds one example for each capability. Each one ran
against a public repository at a fixed commit: ripgrep, requests, helm,
terraform-aws-vpc, zls and grafana. It also lists what the tool does not do, and what
each of those would take.

Three documents cover the parts that are not one language at a time:
[CROSS_LANGUAGE.md](CROSS_LANGUAGE.md) for what a name crossing a language boundary
can and cannot prove, [API_CONTRACTS.md](API_CONTRACTS.md) for rewriting a service
while preserving the contract its callers see. [RECIPES.md](RECIPES.md) for the
recipe language `fr recipe` runs.

[BUGS.md](BUGS.md) tracks the open limitations. The tool reports each one to you
instead of answering it wrongly in silence. There are five: reachability under
dynamic dispatch, Helm values passed on a command line, a CSS class named inside a
TSX helper, three SCSS forms the grammar does not cover, and deep Terraform index
traversals.

Not yet built: the optional LSP delegation backend. (A watch-mode daemon is on the
plan but the fact cache already recovers most of what it would have saved.)

## Licence

AGPL-3.0-or-later.
