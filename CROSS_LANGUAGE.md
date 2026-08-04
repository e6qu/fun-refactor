# Cross-language refactoring — what crosses, what does not, and what it would take

A rename that stops at a file extension is not much use. A CSS class is named by
markup, a Helm value by a template, an environment variable by a manifest and by the
program that reads it. This document maps those boundaries: which the tool crosses
today, which it does not, and what each missing one would cost.

Every number here was measured, not estimated. `examples/crosslang.rs` produced them:

    cargo run --example crosslang -- /path/to/repo
    cargo run --example crosslang -- /path/to/repo "rust->zig"    # name the crossings

## The first thing the measurement changed

The tool's flagship cross-language feature — a Helm value renamed across a chart —
**does not cross a language boundary at all.** A `values.yaml` sitting beside a
`Chart.yaml` is detected *as* Helm, and so is the template that reads it. Measured by
language, a production `ingress-nginx` chart reports zero cross-language references.
Measured by *file role*, the same chart reports **627**.

    ingress-nginx chart (47 helm files, 36 templates)
      by language:   0 crossings
      by file role:  template -> values   627

That is 627 of the 674 `.Values.*` expressions in the chart — 93%. The edge works
very well. It is just not the kind of edge the word "language" describes.

So there are two different things worth separating, and conflating them is how you
end up believing a feature is missing or present when it is not:

| | |
| --- | --- |
| **Cross-file-role** | A definition and its uses live in different *kinds* of file that the tool labels the same language. Helm values → templates. This is where nearly all the working cross-file resolution is. |
| **Cross-language** | The two files are different languages. Markup → stylesheet, TSX → TypeScript. Rarer, and the interesting frontier. |

## What crosses today

Measured on the bundled sample, which is the only corpus that exercises many
languages at once:

    web/sample (24 files, 15 languages, 574 resolved references)
            html -> css          18   selector
             tsx -> css           2   selector
             tsx -> typescript    8   function 6, interface 2

And on real repositories, which are mostly monolingual and say so:

    psf/requests   7,687 resolved references,  2 crossings (html -> css)
    ripgrep       37,934 resolved references,  0 crossings
    helm          79,394 resolved references,  0 crossings

The lesson is worth stating plainly: **on a real single-language repository,
cross-language refactoring does nothing.** It earns its place in polyglot
repositories — a service with a chart, a frontend with stylesheets, infrastructure
beside the code it configures — and those are exactly the repositories where nothing
else will do the job.

## What may cross, and why that is now a table

Resolution matches candidates by name across the whole workspace. Until recently it
did so **without asking what language a candidate was written in**. In the bundled
sample that produced four false crossings, one of them dangerous:

    ingest.rs:56 `push` [import-qualified] -> method Ring::push in buffer.zig

A Rust `out.push(…)` — a `Vec::push` — resolving to a Zig struct method, at a
confidence tier the tool *rewrites*. Renaming the Zig method turned the Rust call into
`out.pushReading(…)`. Two languages, no relationship, and a perfectly ordinary diff.

`lang::may_resolve_across(from, to, kind)` now enumerates the boundaries a reference
may cross. It is a table rather than a heuristic because the cost of a wrong entry is
an edit that compiles somewhere else and breaks here.

| From | To | For | Why it is real |
| --- | --- | --- | --- |
| any | itself | everything | the ordinary case |
| TypeScript | TSX, and back | everything | TSX *is* TypeScript with JSX; a `.tsx` imports from a `.ts` constantly |
| CSS | SCSS, and back | everything | SCSS compiles to CSS and they share one selector namespace |
| HTML, XML, TSX, TypeScript, Markdown | CSS, SCSS | selectors, custom properties | markup names a style rule by class or id |
| Helm | YAML, and back | keys | a template names a key in its values file |
| HTML, XML, TSX, TypeScript | HTML, XML | element ids | a template names an element the markup declares |

**Deliberately absent: every pair of imperative languages.** Rust cannot name a Zig
method; Go cannot name a Python function. Where an FFI does connect them, the binding
is declared in a build file this tool does not read, and reporting those as unresolved
is the honest answer rather than a guess that occasionally rewrites the wrong file.

## What does not cross, and what each would take

These are edges that exist in real code and that the tool does not follow. Ordered by
how often they appear in the repositories people actually have.

### 1. CSS modules — `styles.primary` in TSX to `.primary` in a stylesheet

The single most common unsupported edge in modern frontend code.

```tsx
import styles from "./Button.module.css";
<button className={styles.primary} />          // resolves to nothing today
```

Measured: plain `class="primary"` in HTML resolves; `styles.primary` does not.

**What it needs.** The default import of a `*.module.css` binds an object whose
members are that file's selectors. That is a real, declared relationship — the import
path names the file — so the edge would be `Exact`, not a guess. The work is in
import resolution: recognise a CSS-module import, bind the local name, and resolve a
member access on it to a selector in the named file.

**Cost.** Moderate. The import machinery already resolves paths; the new part is
treating a stylesheet as a module with an export list.

### 2. Element ids named from code — `getElementById("panel")`

```ts
document.getElementById("open-path")           // a string, resolving to nothing
```

The tool already resolves ids *within* markup (`<label for>` → `<input id>`). From
code the id is a string literal.

**What it needs.** String-keyed resolution already exists — it is how Helm values and
some config keys resolve. This is the same mechanism with a narrower trigger: a string
argument to a known DOM accessor. It must be `NameOnly`: nothing proves the string is
an id rather than a coincidence, and the tool should say so rather than rewrite it.

**Cost.** Small, and it should be reported rather than rewritten.

### 3. Environment variables — manifest to `os.getenv`

```yaml
env: [{ name: RETENTION_DAYS, value: "30" }]   # a manifest
```
```python
os.environ["RETENTION_DAYS"]                    # the program that reads it
```

`fr stitch` **already traces this**, end to end, including the `.Values` path behind
the manifest value. What it does not do is make it a *rename* edge: stitch reports
chains, and renaming the manifest key does not rewrite `os.environ[…]`.

**What it needs.** Promote the stitch chain into the reference index, at `NameOnly`,
so a rename reports it as a use it will not rewrite. Rewriting would be wrong — an
environment variable name is a runtime string that other systems also use.

**Cost.** Small, and mostly a question of whether the answer belongs in `refs`.

### 4. Bash to a program's flags — `--retention-days`

```bash
./collector --retention-days 30
```

The flag is declared in Go or Rust as a struct field or a clap attribute. This is how
scripts and CI break silently when a flag is renamed.

**What it needs.** A flag declaration is recognisable per framework (clap attributes,
Go's `flag` package, `argparse`), and the shell side is a word starting with `--`.
`NameOnly`, always. The catalogs are the natural home for the per-framework rules —
they already encode "what a test looks like" per language in exactly this shape.

**Cost.** Moderate, and it grows with every framework. The catalog format keeps that
growth out of the code.

### 5. CI configuration to the scripts it runs

```yaml
- run: ./scripts/deploy.sh --namespace signals
```

A path in a YAML `run:` step naming a file, and flags naming a script's options.

**What it needs.** Path-valued strings resolving to files is a small, high-confidence
edge — the path either exists in the workspace or it does not. Worth having for
"what runs this?" as much as for renaming.

**Cost.** Small.

### 6. Terraform to the scripts and templates it renders

```hcl
user_data = templatefile("${path.module}/init.sh", { port = var.port })
```

The file reference is a path; the substituted names are template variables inside
another language's file.

**Cost.** The path half is small. The variable half needs a template grammar per
target and is probably not worth it.

### 7. Markdown to the code it documents

A link to `src/ingest.rs#L20`, or a fenced block naming a function that has been
renamed. Documentation drifts from code more reliably than anything else in a
repository.

**Cost.** The link half is small and genuinely useful. Prose mentioning a symbol is
already covered — as a *textual occurrence*, reported and never rewritten, which is
the right answer.

## Rewriting a file as another language

Since this document was written, `fr translate` gained a second mode. The first —
containment — writes the same bytes under a different grammar: CSS as SCSS, a manifest
as a Helm template. The second **translates**, between Rust, Go, Python and TypeScript,
and is a different promise entirely.

The signature is the contract: every parameter in order, with its type and the return
type, carried exactly and spelled the target's way. `fn averages(readings: &[Reading])
-> HashMap<String, f64>` becomes `def averages(readings: list[Reading]) ->
dict[str, float]`. Declarations are idiomatic — a record is a Rust `struct` with an
`impl`, a Python `@dataclass`, a Go `struct`, a TypeScript `interface` or `class`.

Everything with no counterpart — ownership, closures, macros, comprehensions, error
propagation — is carried into the output **verbatim, inside a comment**, and counted.
The result is a draft that says exactly how much of it is real.

See `src/transpile/` and RECIPES-style notes in that module's documentation.

## What this changes about the design

Three things the measurements argue for:

1. **Report the file-role edge, not just the language edge.** The tool's most
   valuable cross-file capability is invisible in a language-based summary. `fr stats`
   should say "627 template→values references", because that is the number that tells
   you the chart is wired up.

2. **Cross-language edges should be `Exact` only where a path is written down.** The
   CSS-module import names a file. The `class` attribute names a class in a stylesheet
   the page includes. Everything reached by a bare string — an env var, an element id
   from code, a flag in a shell script — is `NameOnly` and must be reported rather
   than rewritten. The four false crossings that started this were all cases of a
   strong tier being handed out for a weak reason.

3. **The permitted table belongs beside the languages, not inside resolution.** It is
   a statement about how these languages refer to each other, which is knowledge about
   the world rather than about this program, and it should be readable as such.
