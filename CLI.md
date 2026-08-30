# The command line

Every command `fr` has, what it answers, and what it refuses.

The binary is `fr`. It reads a workspace, answers questions about it, and
changes it. There is no daemon, no index to warm and no configuration file.

Three conventions hold across the whole surface, and knowing them removes most
of what you would otherwise have to look up.

**Every command takes `--json`.** The text output is for reading and the JSON is
for a program. Both carry the same facts. The JSON writes every path
absolutely, under the key `file`.

**Every mutation is a dry run until you say otherwise.** A command that changes
files prints a unified diff and exits. Pass `--write` to apply it. A multi-file
write is atomic: all of it lands or none of it does.

**A refusal names the gap.** Where an operation cannot be done for a language
or for an input, the tool says which and why. It exits non-zero, does not do
half the work, and does not do nothing quietly.

## Naming what to act on

Most commands take a `<TARGET>`. Write it one of two ways.

- A position: `src/parse.rs:120:8`. The line and column are 1-based and land on
  the identifier.
- A bare name: `parse_file`. Where the workspace declares that name once, the
  command proceeds. Where it declares it more than once, the refusal lists every
  declaration and asks for a position.

A position is always unambiguous. A bare name is convenient and sometimes
ambiguous, and the tool tells you which case you are in.

## Global options

| Option | What it does |
|---|---|
| `--json` | Machine-readable output instead of text |
| `-C`, `--root <ROOT>` | The workspace to act on. Naming a single file scans that file alone. Default `.` |
| `--max-file-size <BYTES>` | Skip files larger than this. Default 4 MiB. Every command warns when a scan skipped one |
| `--no-ignore` | Read files `.gitignore` excludes, and hidden files. Generated and vendored trees are refactoring targets like any other |
| `--no-cache` | Re-read every file instead of reusing cached facts |
| `-V`, `--version` | Print the version |

`--max-file-size` matters more than it looks. A skipped file is invisible to
every analysis, so a rename can miss uses inside it. The warning is there so a
silent gap cannot read as a clean answer.

## Understanding a workspace

### `fr scan`

List the files `fr` can act on, and count the ones it cannot.

```
fr scan [--lang <LANGUAGE>]
```

Files in no supported language count by extension rather than listing one by
one. A build that omits a grammar says so here rather than calling the file
unsupported.

### `fr parse`

Parse every file and report syntax health.

```
fr parse [--lang <LANGUAGE>] [--stats]
```

A file with syntax errors still reaches the index, carrying partial facts. Run
this to find out which files those are. `--stats` adds node counts per
language.

### `fr symbols`

List what the workspace declares.

```
fr symbols [PATHS]... [--lang <L>] [--name <N>] [--kind <K>] [--stats]
```

`--kind` takes the kinds the model has: `function`, `struct`, `enum`, `key`,
`selector`, `element-id` and the rest. `--name` filters by name.

### `fr def`

Show every place a symbol is defined.

```
fr def <TARGET> [--first]
```

More than one definition is an ordinary answer: Java overloads, a name declared
per platform, a key set in two values files. `--first` takes the first and is
for scripts that want one.

### `fr type`

Show a symbol's type: what the source declared, or what follows from what it
did.

```
fr type <TARGET>
```

Where the source wrote a type, that is the answer. Where it did not, the
assignments and call sites settle one. The report says which of the two you are
reading.

### `fr implementations`

Show the concrete implementations of an abstract declaration.

```
fr implementations <TARGET>
```

Reads the four declared hierarchies. A Rust `impl Trait for Type`, a Go
interface a type covers, a TypeScript `implements` or `extends`, a Python base
class. Zig and Bash declare no such relationship, and the refusal says so.

### `fr usages` and `fr refs`

Show every use of a symbol.

```
fr usages <TARGET> [--include-unresolved]
fr refs   <TARGET> [--include-unresolved]
```

`usages` groups by file and is for reading. `refs` is flat and is for scripts.
Both carry a confidence per reference: `exact`, `import-qualified`,
`field-based` or `name-only`. `--include-unresolved` adds the references that
resolved to nothing, which is what you want before deleting anything.

### `fr callers` and `fr callees`

Walk the call graph.

```
fr callers <TARGET> [--depth <N>]
fr callees <TARGET> [--depth <N>]
```

Each edge carries its confidence and its origin. An edge from a hierarchy fan-out
is marked, and so is one through a function held in a field. A walk stops at an
unresolved call and says so rather than guessing past it.

### `fr graph`

Export the whole call graph.

```
fr graph [--dot]
```

`--dot` writes Graphviz. The JSON carries every node, every edge, and counts by
confidence and by origin.

### `fr entrypoints`

List the detected entry points.

```
fr entrypoints [--kind <K>] [--catalogs <PATH>] [--unreachable]
```

Entry points are data and not hardcoded rules: per-framework catalogs say what
one looks like. `--catalogs` points at your own. `--unreachable` inverts the
question and lists what no entry point reaches.

### `fr flow`

Trace where a value comes from or goes to.

```
fr flow <backward|forward> <TARGET> [--depth <N>]
        [--set K=V] [--set-string K=V] [--set-file K=PATH] [--set-json K=JSON]
```

For imperative languages this is dataflow. For a configuration language with a
substitution model it is provenance: which document overrode which. The `--set`
family supplies Helm values the way `helm` itself takes them, so a chain that
depends on one can be followed.

### `fr stitch`

Trace configuration into the code that reads it.

```
fr stitch [--env <NAME>] [--orphaned] [--files] [--flags]
```

Three questions, one command.

- With no flag: environment variables. A manifest or a compose file declares
  one, and this finds every `getenv` that reads it. `--orphaned` lists the ones
  nothing reads.
- `--files`: the file a path names. The script a CI step runs, the template a
  Terraform resource renders, the file a Markdown link points at. A path either
  exists or it does not, so this edge is exact.
- `--flags`: the program that declares a `--flag` a script passes. clap, Go's
  `flag` package, `argparse` and commander. The report names a flag something
  passes and nothing declares, which is what a renamed flag looks like.

### `fr impact`

Show everything a change to a symbol could affect.

```
fr impact <TARGET> [--caller-depth <N>]
```

References, callers to a depth, and the config and contract edges that reach it.
This is the command to run before agreeing to a change, not after.

### `fr capabilities`

Show what this tool can do, per language.

```
fr capabilities [--capability <C>] [--lang <L>] [--markdown]
```

The matrix comes from asking each refactoring's own predicate, so it cannot drift
from the code. Every cell that is not supported carries its reason.
`--markdown` prints the table the README publishes.

## Finding work

### `fr duplicates`

Find code written more than once.

```
fr duplicates [--min-tokens <N>] [--exact] [--lang <L>] [--path <P>]
```

Structural by default, so two copies that differ in their names still match.
`--exact` demands the same tokens.

### `fr unused`

List symbols nothing appears to use.

```
fr unused [--catalogs <P>] [--lang <L>] [--path <P>] [--internal]
```

"Appears" carries weight. A symbol reached only through a hierarchy fan-out or a
function value is spared, and the report says which. `--internal` restricts the
answer to symbols the workspace does not export.

## Changing code

Every command here prints a diff and needs `--write` to apply it.

### `fr rename`

```
fr rename <TARGET> <NEW_NAME> [--write]
```

Renames the declaration and every reference that provably points at it. A
reference the tool cannot prove stays where it is, and the report names it.

### `fr extract`

```
fr extract <RANGE> <NAME> [--function] [--all] [--write]
```

An expression becomes a named binding. With `--function`, a run of statements
becomes a function. Its parameters come from what the selection reads, and its
return from what the rest of the body needs. `--all` extracts every occurrence
of the same expression.

A region holding a `return` leaves the enclosing function, and a call does not.
Every target extracts one anyway. The new function answers, and the call site
does the returning. Each says it its own way.

| Target | The new function answers | The call site |
|---|---|---|
| Rust | `Option<T>` | `if let Some(answer) = f(…) { return answer; }` |
| Go | `(T, bool)` | `if answer, ok := f(…); ok { return answer }` |
| Zig | `?T` | `if (f(…)) \|answer\| { return answer; }` |
| Java | `Optional<T>` | `var answer = f(…); if (answer.isPresent()) …` |
| TypeScript | `[T, true] \| [null, false]` | `const [answer, ok] = f(…); if (ok) …` |
| Python | a pair | `answer, ok = f(…)` then `if ok:` |

Where the enclosing function answers nothing, the new one answers a flag and the
call site returns bare.

TypeScript takes a discriminated pair rather than `[T \| null, boolean]`. Strict
mode refuses to return the nullable half. Go declares a zero on the way out,
since nothing here knows a named type's zero.

A region that both returns and produces a value the code after it reads refuses.
One answer cannot carry both, and the refusal names the bindings.

### `fr inline`

```
fr inline <TARGET> [--call] [--write]
```

The inverse. A variable's uses take its value; with `--call`, a call takes the
callee's body. Precedence is restored from structure, so an inlined `a + b`
inside a multiplication keeps its brackets.

### `fr signature`

```
fr signature <TARGET> <CHANGE> [--write]
```

Change a function's parameters and update every call site. The change takes one
of three forms.

| Form | What it does |
|---|---|
| `remove:<i>` | Drop the parameter at index `i`, and its argument at every call |
| `move:<from>:<to>` | Reorder, and reorder every call's arguments to match |
| `add:<i>:<declaration>:<argument>` | Insert a parameter, and pass `<argument>` at every call |

`remove:1` drops the second parameter. `add:0:limit: int:50` puts `limit: int`
first and passes `50`.

### `fr move`

```
fr move <TARGET> <DESTINATION> [--write]
```

Move a top-level symbol to another file and repoint every import. Refuses where
moving would change what a name means: Java ties a file to its public type, and
markup has no import to repoint.

### `fr delete`

```
fr delete <TARGET> [--write]
```

Delete a symbol, refusing if anything still uses it. A file that failed to parse
counts as possibly hiding a use, so the refusal states what it could not see.

### `fr imports`

```
fr imports [FILE] [--write]
```

Remove unused imports and sort the rest. Per file, or across the workspace.

### `fr remove-flag`

```
fr remove-flag <FLAG> [--value <BOOL>] [--write]
```

Remove a feature flag and everything that only existed to serve it. The branch
not taken goes, the condition goes, and the code that only that branch called
goes with them.

### `fr rewrite`

```
fr rewrite <TARGET> [REWRITE] [--write]
```

Apply a local transformation. With no rewrite named, lists the ones that apply
at that position.

### `fr restructure`

```
fr restructure <PATTERN> <TEMPLATE> [--lang <L>] [--write]
```

Rewrite every occurrence of a code shape. Write the pattern and the template in
the target language, with metavariables for the parts that vary.

### `fr recipe`

```
fr recipe <FILE> [--write] [--explain] [--catalogs <P>]
fr recipe --vocabulary [--json]
```

Run a refactoring recipe: find, do, expect. One transaction, all or nothing.
`--explain` prints what the recipe would do without doing it. The language is
documented in [RECIPES.md](RECIPES.md).

`--vocabulary` prints the language itself. Every verb with the form its arguments
take. The predicates a step takes for a symbol, and the fewer it takes for a
file. The rewrites this build has, and the languages. Every list comes from the
code that reads a recipe. With `--json`, a program writing a recipe reads what it
may write.

## Crossing languages

### `fr translate`

```
fr translate <FILE> [LANGUAGE] [--write] [--out <PATH>] [--force]
```

Rewrite a file as another language, beside the original. With no language named,
lists what this file could become and why each is possible.

Two different promises share this command.

- Where one grammar contains another, the result is the same bytes under the
  target's extension, checked by the target's parser. A `.ts` becomes a `.tsx`,
  a CSS file becomes SCSS, a YAML manifest becomes a Helm template.
- Between programming languages, the result is a draft. Signatures carry with
  their types where the source gave them. Every construct without a counterpart
  is marked in the output and counted in the report. The intermediary language
  this goes through is documented in [IR.md](IR.md).

`--out` chooses the destination and `--force` overwrites. The original always
stays: nobody can read a deleted input back out of the diff.

### `fr openapi`

```
fr openapi [--out <PATH>] [--yaml]
```

Derive an OpenAPI document from a route tree. Reads a Next.js `app/api`
directory, a FastAPI router, and Express, Flask, axum, gin and Spring beside
them. Paths, methods and path parameters are exact. Anything the source left
undeclared is listed as undeclared and not invented.

## Housekeeping

### `fr cache`

```
fr cache [--clear]
```

Inspect or clear the fact cache. The cache is keyed by content and by the
version of everything that could change a fact, so a stale answer is not
findable. `--clear` is for when you want to prove that.

### `fr completions`

```
fr completions <SHELL>
```

Print a completion script. `bash`, `zsh`, `fish`, `elvish` and `powershell`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | The command answered, or the change applied |
| 1 | A refusal, with the reason on stderr |
| 2 | The arguments did not parse |

A refusal is a normal outcome and not a crash. `fr delete` on a symbol something
uses exits 1, and the reason names the use.

## See also

- [RECIPES.md](RECIPES.md), the recipe language `fr recipe` runs.
- [IR.md](IR.md), the intermediary language every translation crosses.
- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md), which references cross a language
  boundary and which do not.
- [EXAMPLES.md](EXAMPLES.md), every refactoring run against real code.
