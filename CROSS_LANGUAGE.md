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
as a Helm template. The second **translates**, between Rust, Go, Java, Python, TypeScript and Zig,
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

### Names take the target's convention

Every one of these languages has a naming convention and they disagree: TypeScript
writes `userName`, Python writes `user_name`, Go says "exported" with a capital letter.
Adopting the target's is most of what makes a translated file read as written rather
than converted.

The rule is the same one the refactorings follow: **rename what the file declares and
nothing else.** Functions, records, fields, constants, parameters and locals are the
module's own and are re-spelled at their declaration *and* at every use. A name the
module does not declare — `db.users.find`, `NextResponse`, a library function — is
foreign and is left exactly as written, because re-casing it would rename somebody
else's API.

One map, built once, consulted everywhere. The alternative — re-casing at each site
with whichever helper was to hand — is how `interface User { userName }` became
`class User: user_name` whose bodies still said `.userName`.

Three details that only real code surfaces:

- **Acronyms.** Splitting before every capital turns `HTTPServer` into
  `h_t_t_p_server` and `MAX_RETRY` into `M_A_X__R_E_T_R_Y`. A separator goes where a
  word actually starts.
- **Fields are their own namespace.** A Go `Reading` with an exported `sensor` field is
  `Sensor`, while a *parameter* also called `sensor` stays lowercase. One map keyed by
  name alone gave the parameter the field's spelling.
- **Keywords.** `select` is a name sqlmodel exports and a keyword in Go. `select(User)`
  is not something Go's grammar accepts, so the whole file was refused — which gives
  the reader nothing. It is escaped and *reported* instead.

### Java, and the language that has no top level

Java is the fifth, and it is the one that made the writer do something no other does.
Every other target takes a module's items and writes them out. Java has **no top level
below the type**: a function has to be inside a class, and a public class must be named
after its file — which is a rule the compiler enforces rather than a convention. So a
module becomes a class, `sensors.py` becomes `Sensors.java`, and a record that would
have been public becomes a package-private sibling with a comment saying why.

Reading Java is the same job in reverse: a file *is* a class, so reading one means
unwrapping it to find the module inside. A `static final` field is Java's only spelling
for a module constant and reads as one; an instance field reads as a field.

### Zig, and the language where a type is a value

Zig is the sixth, and it is the far end of the range. A `struct` is not a declaration
form — it is a **value**, so `const Reading = struct { … };` is a constant whose value
happens to be a type, and the methods live inside it. Reading one means noticing that
the grammar reuses `variable_declaration` for an assignment too: `sum = sum + x` and
`var sum = 0` are the same node, and only the keyword tells them apart.

Four things about the language shape the writer:

- **No `new`, no exception, no `async`.** Failure is a value in the return type and the
  error set is part of the type, so a `throw` arriving from Python or Java has nowhere
  to go; `async` was removed in 0.11 and has not come back. Each is carried, because
  inventing an error set would be inventing the program's vocabulary of failures.
- **No block comment.** `//` runs to the end of the line, so a carried fragment written
  beside an expression would swallow the rest of the statement, semicolon included. It
  goes on its own line above the statement — the only place in Zig a comment can go.
- **`var` is an error when nothing writes to it.** Only the Rust reader records
  mutability; every other one says "mutable" because it has nothing better to say.
  Which keyword a binding takes is therefore a fact about the rest of the body, worked
  out by looking at what assigns to it.
- **Three conventions, not two.** Types are `PascalCase`, functions are `camelCase` and
  everything else is `snake_case` — a split no other target here makes, and one that
  spelled every local like a function until the naming map learned to tell them apart.

`error` is Go's type and Zig's keyword. It is written `@"error"`, which is how Zig
spells an identifier that collides with one of its own words, and under it the name
still says what the source said.

### How much of this is checked

Two properties, both over every real file in the repository and every vendored corpus,
into every target:

1. **The output parses as the language it claims to be.** The strongest check available
   without six compilers. It found nine defects the first time it ran.
2. **Every function comes back with the parameters it left with.** A round trip — read,
   translate, read the result back, compare. Parsing cannot see a dropped parameter; the
   file is still perfectly good in the target's grammar, and the fidelity report says
   every signature carried across intact. This found four more.

What the round trip compares is deliberately narrow: which functions exist and what
their parameters are called. Types are where the legitimate differences live, and a
check that argued about those would spend its life growing exceptions. A parameter
appearing or vanishing is never legitimate.

### What real code has that a fixture does not

The strongest check available without six compilers is that the output parses as the
language it claims to be. Run over this repository's own source — twenty thousand lines
of Rust, thirteen files of TypeScript, three of Go, and the vendored Python — that
failed 97 of 235 translations, and every failure was a thing nobody thinks to put in a
fixture:

- **A comment between two parameters.** A comment is an *extra* in every one of these
  grammars: it can appear between any two nodes anywhere. A reader that walks a
  parameter list positionally reads it as a parameter.
- **A string with an escape in it.** The IR has to hold the string's *value*; holding
  its spelling means the next writer escapes the backslash again, and a newline becomes
  a backslash and an `n` in a file that still parses.
- **A doc comment quoting a glob.** `app/**/route.ts` contains `*/`, which closes the
  `/** ... */` a Java or TypeScript writer put it in.
- **A number with its width written into it.** `0usize` is a Rust spelling; everywhere
  else it is a number glued to an identifier.
- **A struct whose fields have no names.** A tuple struct is a Rust idea, and a record
  here is a named product.

None of these is exotic. All five are in the first file you would pick up.

### What the IR has, and what it deliberately does not

Two additions came out of reading real Java rather than running it.

**The conditional expression** — `a ? b : c`, `b if a else c`, `if a { b } else { c }` —
is one expression that chooses between two, and five of these six languages have it. It
is a node rather than a branch because it *is* a value: reading it as an `if` would need
somewhere to put the result, and there is no such place inside an argument list. Go is
the exception and says so.

**The base class.** Three of these languages inherit and three do not. It is carried
into Java, Python and TypeScript, and *reported* for Rust, Go and Zig — because
`class JsonPrimitive extends JsonElement` becoming a class that extends nothing is a
different type, and a translation that says nothing about it is the failure this whole
document is about. Only a single base: Python allows several, the other two do not, and
picking one of them would be a guess.

### The constructor has six spellings

| | |
| --- | --- |
| Java | named after the class, no return type |
| Python | `__init__`, takes `self` |
| TypeScript | `constructor`, takes neither |
| Rust | `Thing::new`, returns the type |
| Go | `NewThing`, returns the type |
| Zig | `Thing.init`, returns the type |

The name is not what carries; what carries is that the function **makes a value of its
type**. The last three have no constructor at all, only a habit — so one is read as a
constructor there only when it *also returns the type*, because a `new` that returns
something else is an ordinary function with a common name.

The consequence worth knowing: **a constructor's body only travels where a receiver
does.** The first three act on a value that already exists; the last three build one and
return it, so a body that assigns through a receiver has nowhere to run. The tool says
so rather than writing `self.n = n` inside a function that binds no `self`. And Java is
the one target that overloads constructors — a second one written anywhere else keeps
the name its source gave it, and the report says which name to call instead.

### The receiver has six names

`self`, `this`, or whatever the Go author called it. The receiver is the one binding
that is **not in the parameter list**, so it never went through the rename that every
other name goes through — and each body kept its source's word. `this.cache` inside a
Rust `impl` is not a typo; it is a file that cannot compile, and it was in every
translated method of every class until the IR started recording which word the source
used and each writer started putting its own back on.

Rust makes this sharper than the rest: `self` is the one keyword it refuses to
raw-escape, so the escape that made every other reserved word writable turned a correct
file into `r#self`, which is a compile error.

### Typed Python and typed TypeScript

The two languages with the closest correspondence get the closest translation.
`list[str]` is `string[]`, `dict[str, int]` is `Record<string, number>`,
`Optional[T]` is `T | null`, `@dataclass` is an `interface`, an f-string is a template
literal, and a comprehension is a `.filter().map()` chain. `await` means the same
thing in both.

Both directions of the round-trip carry **zero** constructs verbatim
(`tests/transpile.rs::typed_python_and_typed_typescript_round_trip`), which is the
strongest claim any pair in this tool makes. It is a claim about *typed* Python:
translating a signature with no annotations means writing `object` for every parameter,
and the report says so.

## Porting a framework, not just a language

`fr translate <route.ts> fastapi` is a third mode, and it exists because a Next.js API
route cannot be translated by reading the file alone.

**The URL is the path.** `app/api/users/[id]/route.ts` serves `/users/{id}` and nothing
inside the file says so. `app/api/files/[...path]/route.ts` serves
`/files/{path:path}` — a catch-all, which FastAPI spells with a converter and which a
translation that emitted `{path}` would silently point at the wrong URLs. This is the
one thing no content-only translation could ever do, and it is most of the value.

| Next.js | FastAPI |
| --- | --- |
| `app/api/users/route.ts` exporting `GET` | `@router.get("/users")` |
| `app/api/users/[id]/route.ts` | `@router.get("/users/{id}")` |
| `app/api/files/[...path]/route.ts` | `@router.get("/files/{path:path}")` |
| `export async function POST` | `@router.post(...)` on an `async def` |
| an exported `interface` | a Pydantic `BaseModel` |
| `NextRequest` | `Request` — same headers, same `await .json()` |
| `const id = context.params.id` | *dropped* — FastAPI already supplies `id` |
| `return NextResponse.json(x)` | `return x` |
| `NextResponse.json(x, { status: 404 })` | `JSONResponse(x, 404)` |
| `NextResponse.redirect(u)` | `RedirectResponse(u)` |

Three of those rows are worth dwelling on, because each was wrong first:

- **`NextRequest` is kept, not dropped.** It is Starlette's `Request` under another
  name. Dropping it and commenting out every line that read it produced a file where
  `await request.json()` — perfectly good Python — was a comment.
- **`context.params.id` is dropped, not carried.** It is the commonest line in a
  Next.js route and it is exactly the work FastAPI already did. Carrying it opened
  every translated handler with a line naming an object Python does not have.
- **Nested returns are rewritten too.** An error return inside an `if` is the
  commonest branch in a route; rewriting only the top level missed precisely those.

### Measured against real projects

The fixtures in `tests/nextjs.rs` and `tests/transpile.rs` are written by whoever
writes the assertion, so `tests/corpus.rs` runs the same translations over two
MIT-licensed projects vendored unmodified and pinned — `fastapi/full-stack-fastapi-template`
and `shadcn-ui/taxonomy`; see `tests/corpus/PROVENANCE.md`. Running against them found
what 1,300 fixture tests did not:

| What real code had | What it produced | Now |
| --- | --- | --- |
| `def create_user(*, session, …)` | `createUser(*: unknown, …)` — will not parse | the marker is dropped and the change of calling convention counted |
| `try { … } catch (e) { … }` | the whole handler body as one comment | `try:` / `except Exception as e:` |
| `error instanceof z.ZodError` | carried | `isinstance(error, z.ZodError)` |
| `new Response(null, {status: 403})` | carried | `Response(status_code=403)` |
| `session?.user.id` | **`None.id`** — a silent wrong answer | carried, with the original |
| `params.postId as string` | **`None`** — a silent wrong answer | `params.postId`; an assertion has no runtime effect |
| `// Validate the route params.` | "not translated: comment" | `# Validate the route params.` |
| `select(User)` in Go | the file refused outright | escaped and reported |

On the hardest route in the corpus — two handlers, each a single `try`, with typed
error branches and four response shapes — the count of constructs carried over went
from 15 to 3. The three that remain are object destructuring, which Python has no
counterpart for.

What it refuses, with the reason: a `.tsx` file containing JSX. A React component
renders a user interface and a FastAPI endpoint answers HTTP. There is no translation
between them, and a file that pretended there was would be worse than no file.

What is left foreign is your own dependencies — `db.posts.find(id)` and the helpers
the route imported — which no translation could supply. When nothing at all is carried
the banner says so instead of saying DRAFT, because a banner that cries draft over a
complete file is a banner nobody reads.

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
