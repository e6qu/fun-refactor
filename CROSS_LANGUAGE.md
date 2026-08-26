# Cross-language refactoring, what crosses, what does not, and what it would take

A rename that stops at a file extension does little. Markup names a CSS class, a
template names a Helm value, and a manifest names an environment variable the program
then reads. This document maps those boundaries: which the tool crosses today, which it
does not, and what each missing one would cost.

`examples/crosslang.rs` measured every number here and estimated none of them:

    cargo run --example crosslang -- /path/to/repo
    cargo run --example crosslang -- /path/to/repo "rust->zig"    # name the crossings

## The first thing the measurement changed

The tool's flagship cross-language feature renames a Helm value across a chart. It
**does not cross a language boundary at all**. The tool detects a `values.yaml` sitting
beside a `Chart.yaml` *as* Helm, and the template that reads it too. Measured by
language, a production `ingress-nginx` chart reports zero cross-language references.
Measured by *file role*, the same chart reports **627**.

    ingress-nginx chart (47 helm files, 36 templates)
      by language:   0 crossings
      by file role:  template -> values   627

That covers 627 of the 674 `.Values.*` expressions in the chart, 93%. The edge works
very well. The word "language" describes a different kind of edge.

Two different things deserve separating here. Conflate them and you end up believing a
feature is missing or present when it is not:

| | |
| --- | --- |
| **Cross-file-role** | A definition and its uses live in different *kinds* of file that the tool labels the same language. Helm values → templates. This is where nearly all the working cross-file resolution is. |
| **Cross-language** | The two files are different languages. Markup → stylesheet, TSX → TypeScript. Rarer, and the interesting frontier. |

## What crosses today

Measured on the bundled sample, the only corpus that exercises many languages at once:

    web/sample (24 files, 15 languages, 574 resolved references)
            html -> css          18   selector
             tsx -> css           2   selector
             tsx -> typescript    8   function 6, interface 2

And on real repositories, most of them monolingual and saying so:

    psf/requests   7,687 resolved references,  2 crossings (html -> css)
    ripgrep       37,934 resolved references,  0 crossings
    helm          79,394 resolved references,  0 crossings

State the lesson plainly: **on a real single-language repository, cross-language
refactoring does nothing**. It pays off in polyglot repositories: a service with a
chart, a frontend with stylesheets, infrastructure beside the code it configures.
Nothing else does the job in those repositories.

## What may cross, and why that is now a table

Resolution matches candidates by name across the whole workspace. Until recently it
did so **without asking what language a candidate was written in**. In the bundled
sample it produced four false crossings, one of them dangerous:

    ingest.rs:56 `push` [import-qualified] -> method Ring::push in buffer.zig

A Rust `out.push(…)`, a `Vec::push`, resolved to a Zig struct method, at a
confidence tier the tool *rewrites*. Renaming the Zig method turned the Rust call into
`out.pushReading(…)`. Two languages, no relationship, and a perfectly ordinary diff.

`lang::may_resolve_across(from, to, kind)` now enumerates the boundaries a reference
may cross. It stays a table rather than a heuristic because a wrong entry costs you an
edit that compiles somewhere else and breaks here.

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

**Deliberately absent: every pair of imperative languages.** Rust cannot name a Zig
method; Go cannot name a Python function. Where an FFI does connect them, a build file
this tool does not read declares the binding. The tool reports those as unresolved, the
honest answer, rather than guessing and occasionally rewriting the wrong file.

## What does not cross, and what each would take

These edges exist in real code and the tool does not follow them. They run in order of
how often they appear in the repositories people have.

### 1. CSS modules, `styles.primary` in TSX to `.primary` in a stylesheet

The single most common unsupported edge in modern frontend code.

```tsx
import styles from "./Button.module.css";
<button className={styles.primary} />          // resolves, and to the imported file
```

Measured again since this was written: it resolves, import-qualified, and to
the selector in the file the import names. A member the imported module does
not declare reaches nothing, rather than a same-named selector elsewhere.

What was wrong was the identity. Two modules declaring `.primary` were one class,
so a rename took both. B754 scoped a module's selectors to their file.

**What it needs.** The default import of a `*.module.css` binds an object whose
members are that file's selectors. The import path names the file, a real declared
relationship, so the edge would be `Exact` rather than a guess. The work sits in import
resolution: recognise a CSS-module import and bind the local name. Then resolve a
member access on it to a selector in the named file.

**Cost.** Moderate. The import machinery already resolves paths, and the new part
treats a stylesheet as a module with an export list.

### 2. Element ids named from code, `getElementById("panel")`

```ts
document.getElementById("open-path")           // a string, resolving to nothing
```

The tool already resolves ids *within* markup (`<label for>` → `<input id>`). From
code, the id arrives as a string literal.

**What it needs.** String-keyed resolution already exists, and Helm values and some
config keys resolve through it. The same mechanism takes a narrower trigger here: a
string argument to a known DOM accessor. Keep it `NameOnly`. Nothing proves the string
names an id rather than matching by coincidence, so the tool should report it and leave
it alone.

**Cost.** Small. Report it and do not rewrite it.

### 3. Environment variables, manifest to `os.getenv`

```yaml
env: [{ name: RETENTION_DAYS, value: "30" }]   # a manifest
```
```python
os.environ["RETENTION_DAYS"]                    # the program that reads it
```

`fr stitch` **already traces this**, end to end, including the `.Values` path behind
the manifest value. It stops short of making the trace a *rename* edge: stitch reports
chains, and renaming the manifest key leaves `os.environ[…]` alone.

**What it needs.** Promote the stitch chain into the reference index at `NameOnly`,
so a rename reports it as a use it will not rewrite. Rewriting would be wrong: an
environment variable name is a runtime string that other systems also read.

**Cost.** Small, and mostly a question of whether the answer belongs in `refs`.

### 4. Bash to a program's flags, `--retention-days`

```bash
./collector --retention-days 30
```

Go or Rust declares the flag as a struct field or a clap attribute. Rename that flag
and scripts and CI break silently.

**What it needs.** Each framework declares a flag recognisably (clap attributes,
Go's `flag` package, `argparse`). On the shell side, look for a word starting with
`--`. Keep it `NameOnly`, always. The catalogs are the natural home for the
per-framework rules; they already encode "what a test looks like" per language in this
shape.

**Cost.** Moderate, and it grows with every framework. The catalog format keeps that
growth out of the code.

### 5. CI configuration to the scripts it runs

```yaml
- run: ./scripts/deploy.sh --namespace signals
```

A path in a YAML `run:` step naming a file, and flags naming a script's options.

**What it needs.** Resolving a path-valued string to a file is a small,
high-confidence edge. The path either exists in the workspace or it does not. It
answers "what runs this?" as much as it serves a rename.

**Cost.** Small.

### 6. Terraform to the scripts and templates it renders

```hcl
user_data = templatefile("${path.module}/init.sh", { port = var.port })
```

The file reference is a path, and the substituted names are template variables inside
another language's file.

**Cost.** The path half is small. The variable half needs a template grammar per
target and is probably not worth it.

### 7. Markdown to the code it documents

A link to `src/ingest.rs#L20`, or a fenced block naming a function somebody has since
renamed. Documentation drifts from code more reliably than anything else in a
repository.

**Cost.** The link half is small and genuinely useful. The tool already covers prose
mentioning a symbol, as a *textual occurrence*, reported and never rewritten, which is
the right answer.

## Rewriting a file as another language

Since this document was written, `fr translate` gained a second mode. The first,
containment, writes the same bytes under a different grammar: CSS as SCSS, a manifest
as a Helm template. The second **translates**, between Rust, Go, Java, Python, TypeScript and Zig,
and it makes a different promise entirely.

The signature is the contract. The writer carries every parameter in order, with its
type and the return type, and spells each the target's way. `fn averages(readings:
&[Reading]) -> HashMap<String, f64>` becomes `def averages(readings: list[Reading]) ->
dict[str, float]`. Declarations come out idiomatic. A record becomes a Rust `struct`
with an `impl`, a Python `@dataclass`, a Go `struct`, a TypeScript `interface` or
`class`.

The writer carries everything with no counterpart into the output **verbatim, inside a
comment**, and counts it: ownership, closures, macros, comprehensions, error
propagation. The result is a draft that says how much of it is real.

See `src/transpile/` and RECIPES-style notes in that module's documentation.

### Names take the target's convention

Every one of these languages has a naming convention and they disagree: TypeScript
writes `userName`, Python writes `user_name`, Go says "exported" with a capital letter.
Adopt the target's convention and the translated file reads as written rather than
converted.

The refactorings follow the same rule: **rename what the file declares and nothing
else.** Functions, records, fields, constants, parameters and locals belong to the
module. The writer re-spells each at its declaration *and* at every use. A name the
module does not declare stays as written: `db.users.find`, `NextResponse`, any library
function. Re-casing one would rename somebody else's API.

One map, built once, consulted everywhere. The alternative re-cases at each site with
whichever helper was to hand. It turned `interface User { userName }` into
`class User. User_name` whose bodies still said `.userName`.

Only real code surfaces these three details:

- **Acronyms.** Split before every capital and `HTTPServer` turns into
  `h_t_t_p_server`, `MAX_RETRY` into `M_A_X__R_E_T_R_Y`. Put a separator where a
  word starts.
- **Fields are their own namespace.** A Go `Reading` spells an exported `sensor` field
  `Sensor`, while a *parameter* also called `sensor` stays lowercase. One map keyed by
  name alone gave the parameter the field's spelling.
- **Keywords.** sqlmodel exports the name `select`, and Go reserves it as a keyword.
  Go's grammar rejects `select(User)`, so the writer refused the whole file, which
  gives the reader nothing. It now escapes the name and *reports* it instead.

### Java, and the language that has no top level

Java is the fifth, and it made the writer do something no other target does.
Every other target takes a module's items and writes them out. Java has **no top level
below the type**: a function has to sit inside a class. A public class must carry its
file's name, a rule the compiler enforces rather than a convention. So a module becomes
a class, and `sensors.py` becomes `Sensors.java`. A record that would have been public
becomes a package-private sibling with a comment saying why.

Reading Java runs the same job in reverse: a file *is* a class, so the reader unwraps
it to find the module inside. A `static final` field is Java's only spelling for a
module constant, so the reader takes it as one. An instance field reads as a field.

### Zig, and the language where a type is a value

Zig is the sixth, and it sits at the far end of the range. A `struct` is a **value**
rather than a declaration form. `const Reading = struct { … };` declares a constant
whose value happens to be a type, and the methods live inside it. The reader also has
to notice that the grammar reuses `variable_declaration` for an assignment.
`sum = sum + x` and `var sum = 0` are the same node. Only the keyword tells them apart.

Four things about the language shape the writer:

- **No `new`, no exception, no `async`.** Failure is a value in the return type, and
  the error set belongs to the type. So a `throw` arriving from Python or Java has
  nowhere to go. Zig removed `async` in 0.11 and has not brought it back. The writer
  carries each, because inventing an error set would invent the program's vocabulary
  of failures.
- **No block comment.** `//` runs to the end of the line. A carried fragment written
  beside an expression would swallow the rest of the statement, semicolon included. It
  goes on its own line above the statement, the only place in Zig a comment can go.
- **`var` is an error when nothing writes to it.** Only the Rust reader records
  mutability. Every other reader says "mutable" because it has nothing better to say.
  The rest of the body therefore decides which keyword a binding takes. The writer
  works that out by looking at what assigns to it.
- **Three conventions, not two.** Zig writes types `PascalCase`, functions `camelCase`
  and everything else `snake_case`, a split no other target here makes. The writer
  spelled every local like a function until the naming map learned to tell them apart.

`error` is Go's type and Zig's keyword. The writer emits `@"error"`, Zig's spelling for
an identifier that collides with one of its own words. Under the escape the name still
says what the source said.

### How much of this is checked

The tests check two properties over every real file in the repository and every
vendored corpus, into every target:

1. **The output parses as the language it claims to be.** The strongest check available
   without six compilers. It found nine defects the first time it ran.
2. **Every function comes back with the parameters it left with, every field and
   constant comes back. No type changes shape.** A round trip: read, translate,
   read the result back, compare. Parsing cannot see a dropped parameter, because the
   file is still perfectly good in the target's grammar. The fidelity report says every
   signature carried across intact. This check has found eighteen.

The check compares a type as a *shape*: a list stays a list, an optional stays
optional, a named type keeps its name. It does not compare which scalar. TypeScript has
one numeric type, so an `i64` that goes through it comes back a `number` and nothing is
wrong. It does not compare the qualifier either. Go has room for one level of it, so
`crate::model::Reference` is `model.Reference` there and can be nothing else. Two
things may change, and the report states both when they do. A placeholder replaces a
type this tool cannot write at all. A constructor's name changes, because in three of
these languages "constructor" *is* a naming convention.

### What real code has that a fixture does not

The output parsing as the language it claims to be is the strongest check available
without six compilers. Run it over this repository's own source: twenty thousand lines
of Rust, thirteen files of TypeScript, three of Go, and the vendored Python. That run
failed 97 of 235 translations, and every failure was a thing nobody thinks to put in a
fixture:

- **A comment between two parameters.** A comment is an *extra* in every one of these
  grammars: it can appear between any two nodes anywhere. A reader that walks a
  parameter list positionally reads it as a parameter.
- **A string with an escape in it.** The IR has to hold the string's *value*. Holding
  its spelling means the next writer escapes the backslash again. A newline becomes
  a backslash and an `n` in a file that still parses.
- **A doc comment quoting a glob.** `app/**/route.ts` contains `*/`, which closes the
  `/** ... */` a Java or TypeScript writer put it in.
- **A number with its width written into it.** `0usize` is a Rust spelling; everywhere
  else it is a number glued to an identifier.
- **A struct whose fields have no names.** A tuple struct is a Rust idea, and a record
  here is a named product.

None of these is exotic. All five are in the first file you would pick up.

### What the IR has, and what it deliberately does not

Reading real Java, rather than running it, produced two additions.

**The conditional expression**, `a ? b : c`, `b if a else c`, `if a { b } else { c }`,
is one expression that chooses between two. Five of these six languages have it. The IR
holds it as a node rather than a branch because it *is* a value. Reading it as an `if`
would need somewhere to put the result. An argument list offers no such place. Go is
the exception and says so.

**The base class.** Three of these languages inherit and three do not. The writer
carries it into Java, Python and TypeScript, and *reports* it for Rust, Go and Zig.
`class JsonPrimitive extends JsonElement` becoming a class that extends nothing is a
different type. A translation that says nothing about it fails the way this document
warns against. The IR holds a single base only: Python allows several, the other two do
not, and picking one of them would be a guess.

### The constructor has six spellings

| | |
| --- | --- |
| Java | named after the class, no return type |
| Python | `__init__`, takes `self` |
| TypeScript | `constructor`, takes neither |
| Rust | `Thing::new`, returns the type |
| Go | `NewThing`, returns the type |
| Zig | `Thing.init`, returns the type |

The name does not carry. The function's job carries: it **makes a value of its type**.
The last three have no constructor at all, only a habit. So the reader takes one as a
constructor there only when it *also returns the type*. A `new` that returns something
else is an ordinary function with a common name.

One consequence matters: **a constructor's body only travels where a receiver
does.** The first three act on a value that already exists. The last three build one and
return it. So a body that assigns through a receiver has nowhere to run. The tool says
so rather than writing `self.n = n` inside a function that binds no `self`. And Java
alone among these targets overloads constructors, so a second one written anywhere else
keeps the name its source gave it. The report says which name to call instead.

### The receiver has six names

`self`, `this`, or whatever the Go author called it. The receiver is the one binding
**outside the parameter list**. So it never went through the rename that every other
name goes through, and each body kept its source's word. `this.cache` inside a
Rust `impl` produces a file that cannot compile, not a typo. It appeared in every
translated method of every class. Then the IR started recording which word the source
used, and each writer started putting its own back on.

Rust sharpens this further: `self` is the one keyword it refuses to
raw-escape. So the escape that made every other reserved word writable turned a correct
file into `r#self`, a compile error.

### Typed Python and typed TypeScript

The two languages with the closest correspondence get the closest translation.
`list[str]` is `string[]`, `dict[str, int]` is `Record<string, number>`,
`Optional[T]` is `T | null`, `@dataclass` is an `interface`, an f-string is a template
literal. A comprehension is a `.filter().map()` chain. `await` means the same
thing in both.

Both directions of the round-trip carry **zero** constructs verbatim
(`tests/transpile.rs::typed_python_and_typed_typescript_round_trip`), the strongest
claim any pair in this tool makes. The claim covers *typed* Python only. Translating a
signature with no annotations writes `object` for every parameter, and the report says
so.

## Porting a framework, not just a language

`fr translate <route.ts> fastapi` is a third mode, and it exists because nobody can
translate a Next.js API route by reading the file alone.

**The URL is the path.** `app/api/users/[id]/route.ts` serves `/users/{id}` and nothing
inside the file says so. `app/api/files/[...path]/route.ts` serves
`/files/{path:path}`, a catch-all. FastAPI spells it with a converter, and a
translation that emitted `{path}` would silently point at the wrong URLs. No
content-only translation could ever do this, and it carries most of the value.

| Next.js | FastAPI |
| --- | --- |
| `app/api/users/route.ts` exporting `GET` | `@router.get("/users")` |
| `app/api/users/[id]/route.ts` | `@router.get("/users/{id}")` |
| `app/api/files/[...path]/route.ts` | `@router.get("/files/{path:path}")` |
| `export async function POST` | `@router.post(...)` on an `async def` |
| an exported `interface` | a Pydantic `BaseModel` |
| `NextRequest` | `Request`, same headers, same `await .json()` |
| `const id = context.params.id` | *dropped*. FastAPI already supplies `id` |
| `return NextResponse.json(x)` | `return x` |
| `NextResponse.json(x, { status: 404 })` | `JSONResponse(x, 404)` |
| `NextResponse.redirect(u)` | `RedirectResponse(u)` |

Dwell on three of those rows, because each was wrong first:

- **The tool keeps `NextRequest`.** It is Starlette's `Request` under another
  name. Dropping it and commenting out every line that read it produced a file where
  `await request.json()`, perfectly good Python, sat inside a comment.
- **The tool drops `context.params.id`.** It is the commonest line in a Next.js route
  and it repeats work FastAPI already did. Carrying it opened every translated handler
  with a line naming an object Python does not have.
- **The tool rewrites nested returns too.** An error return inside an `if` is the
  commonest branch in a route. Rewriting only the top level missed precisely those.

### Measured against real projects

Whoever writes the assertion writes the fixtures in `tests/nextjs.rs` and
`tests/transpile.rs`. So `tests/corpus.rs` runs the same translations over two
MIT-licensed projects, vendored unmodified and pinned: `fastapi/full-stack-fastapi-template`
and `shadcn-ui/taxonomy`, see `tests/corpus/PROVENANCE.md`. Running against them found
what 1,300 fixture tests did not:

| What real code had | What it produced | Now |
| --- | --- | --- |
| `def create_user(*, session, …)` | `createUser(*: unknown, …)`, will not parse | the marker is dropped and the change of calling convention counted |
| `try { … } catch (e) { … }` | the whole handler body as one comment | `try:` / `except Exception as e:` |
| `error instanceof z.ZodError` | carried | `isinstance(error, z.ZodError)` |
| `new Response(null, {status: 403})` | carried | `Response(status_code=403)` |
| `session?.user.id` | **`None.id`**, a silent wrong answer | carried, with the original |
| `params.postId as string` | **`None`**, a silent wrong answer | `params.postId`; an assertion has no runtime effect |
| `// Validate the route params.` | "not translated: comment" | `# Validate the route params.` |
| `select(User)` in Go | the file refused outright | escaped and reported |

Take the hardest route in the corpus: two handlers, each a single `try`, with typed
error branches and four response shapes. The count of constructs carried over went
from 15 to 3. The three that remain are object destructuring, which Python has no
counterpart for.

It refuses one thing, with the reason: a `.tsx` file containing JSX. A React component
renders a user interface and a FastAPI endpoint answers HTTP. No translation joins
them, and a file that pretended otherwise would be worse than no file.

Your own dependencies stay foreign: `db.posts.find(id)` and the helpers the route
imported, which no translation could supply. When the writer carries nothing at all,
the banner says so rather than DRAFT. A banner that cries draft over a complete file is
a banner nobody reads.

## What this changes about the design

Three things the measurements argue for:

1. **Report the file-role edge alongside the language edge.** A language-based summary
   hides the tool's most valuable cross-file capability. `fr stats` should say
   "627 template→values references", the number that tells you the chart is wired up.

2. **Cross-language edges should be `Exact` only where a path is written down.** The
   CSS-module import names a file. The `class` attribute names a class in a stylesheet
   the page includes. Everything a bare string reaches stays `NameOnly` and must be
   reported rather than rewritten. That covers an env var, an element id from code, a
   flag in a shell script. The four false crossings that started this all handed a
   strong tier out for a weak reason.

3. **The permitted table belongs beside the languages, not inside resolution.** It
   states how these languages refer to each other. That is knowledge about the world
   rather than about this program, and the table should read as such.
