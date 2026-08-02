# fun-refactor

Refactoring and code intelligence across 12 languages, in one self-contained binary.

Built on tree-sitter, sharing its language taxonomy and grammar set with
[funveil](https://github.com/e6qu/funveil). No language server, no daemon, no
project configuration.

```
fr rename btn-primary btn-cta
```

That command rewrites the CSS selectors that declare the class, every HTML `class`
attribute that uses it, and every TSX `className` prop — three languages, three
grammars, one entity. No language server can see across that boundary.

## Why

Language servers are excellent at the four big ecosystems and absent everywhere
else: `terraform-ls` has no rename, `bash-language-server` has no rename, `zls`
returns empty edits, and nothing rename-grade exists for HTML, CSS, XML, Markdown
or Helm. Meanwhile *cross-language* references — a CSS class named in a JSX prop, a
Helm values key read in a template, a Terraform variable threaded through modules —
are invisible to every one of them, because each sees a single language.

That gap is the whole point of this tool. See [RESEARCH.md](RESEARCH.md) for the
survey the design is based on, with sources.

## Languages

| | Rust | Go | Zig | TS/TSX | Python | Bash | HCL | Helm/YAML | CSS/SCSS | HTML | XML | Markdown |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| symbols / refs | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| rename | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| call graph | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | — | — | — | — |
| entry points | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ | ✓ | ✓ |
| flow (def-use) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | — | — | — | — |
| provenance | — | — | — | — | — | — | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| extract / inline var | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | — | — | — | — | — |
| extract function | ✓ | ✓ | — | ✓ | ✓ | — | — | — | — | — | — | — |
| inline call | ✓ | ✓ | — | ✓ | ✓ | — | — | — | — | — | — | — |
| change signature | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | — | — | — | — | — |
| move to file | — | — | — | ✓ | ✓ | — | — | — | — | — | — | — |
| safe delete | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| organize imports | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | — | — | — | — | — |
| restructure | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| micro-rewrites | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | — | — | — | — | — |

A `—` is a deliberate refusal, not a silent no-op: the tool tells you the operation
does not apply and why.

## Commands

```
fr cache                      # where cached facts live, and how big
fr scan                       # files it can act on
fr parse --stats              # syntax health per language
fr symbols --kind function    # what is defined
fr def <name|path:line:col>   # where is it defined
fr refs <target>              # where is it used, with confidence per site
fr rename <target> <new>      # rename it and everything that points at it
fr extract <path:l:c-l:c> <n> # extract an expression into a binding
fr extract <range> <n> --function   # extract statements into a function
fr inline <target>            # replace a variable's uses with its value
fr inline <path:l:c> --call   # replace a call with the callee's body
fr signature <target> remove:1  # change parameters, update every call site
fr move <target> <dest-file>  # move a symbol, update imports
fr delete <target>            # delete it, refusing if anything uses it
fr unused                     # symbols nothing appears to use
fr imports <file>             # drop unused imports, sort the rest
fr restructure 'old($X)' 'new($X)' --lang rust
fr remove-flag USE_NEW --value true  # and everything that only served it
fr rewrite <path:l:c>         # list local transformations that apply here
fr rewrite <path:l:c> guard-clause   # ...and apply one
fr callers <fn> --depth 3     # who calls this
fr callees <fn> --depth 3     # what does it call
fr graph --dot                # the call graph
fr flow back <target>         # where did this value come from
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
query set, so a repeated command re-reads only what changed — roughly 1.7× faster
cold and 3–5× warm. `--no-cache` bypasses the cache; `fr cache --clear` empties it.

## What it will not do

The design principle is that characterised imprecision beats confident guessing.

**Every resolved edge carries a confidence tier** — `exact`, `import-qualified`,
`field-based`, `name-only` — and mutations only rewrite the top two. Anything
weaker is reported for you to check.

- **Renames report, never rewrite, occurrences in strings and comments.** They
  defeat syntax analysis and language servers alike.
- **Ambiguous names are refused with a listing**, not resolved by picking one.
- **Change-signature refuses entirely** if any call site is unproven, because
  updating a subset does not compile.
- **Flow analysis stops loudly** at function boundaries, unresolved calls and weak
  resolutions rather than over-approximating through them.
- **No edit is written if it would break the file.** Every changed file is reparsed
  and the whole operation is rejected if it gains a syntax error.

Formatting is never touched. Edits are byte-range splices on the original source,
so comments, spacing and trailing whitespace outside the edited range survive
exactly — including inside an extracted expression.

## Install

```
cargo install --path .
```

## Adding a language

Language knowledge lives in tree-sitter query files, not Rust. `queries/<lang>/facts.scm`
declares definitions, references, scopes and imports through capture conventions
documented in `src/extract.rs`; entry-point rules are YAML in `catalogs/`. Adding a
framework or a language means adding data.

## Status

Stages 0–7 of [PLAN.md](PLAN.md) are complete; 8 is partial. 673 tests.

Not yet built: the optional LSP delegation backend. (A watch-mode daemon is on the
plan but the fact cache already recovers most of what it would have saved.)

Known limitations are tracked in [BUGS.md](BUGS.md) — notably SCSS runs on the CSS
grammar, Helm template actions are masked before YAML parsing so `.Values` references
are not yet resolved, and import liveness is decided by name, which cannot see a
module imported purely for a side effect.

## Licence

AGPL-3.0-or-later.
