# The command line

Every command `fr` has, what it answers, and what it refuses.

The binary is `fr`. It reads a workspace, answers questions about it, and
changes it. There is no daemon, no index to warm and no configuration file.

Three conventions hold across the whole surface, and knowing them removes most
of what you would otherwise have to look up.

**Every command takes `--json`.** The text output is for reading and the JSON is
for a program. Both carry the same facts. The JSON writes every path
absolutely, under the key `file`. History records use workspace-relative `path` values.

**Every mutation is a dry run until you say otherwise.** A command that changes
files prints a unified diff and exits. Pass `--write` to apply it. `--save-plan` stores a plan without changing source.
`openapi --out` also authorizes writing its named output. See [Write guarantees](#write-guarantees) for failure and recovery behavior.

**A refusal names the gap.** Where an operation cannot be done for a language
or for an input, the tool says which and why. It exits non-zero, does not do
half the work, and does not do nothing quietly.

## Write guarantees

Native CLI writes record a transaction in `.fr-history/state.json` before changing source.
The journal stores before/after text, existence, Unix permission modes, validation labels and source digests.
Replacements use staged files and filesystem renames. Other processes can observe intermediate states.
A handled failure restores the starting snapshots when the current files still match this transaction.
A conflicting file prevents recovery; the journal retains the source needed for manual repair.
The next invocation reports pending recovery. Further writes refuse until recovery succeeds.

`fr history recover <ID> --write` restores an interrupted operation to its starting state.
It checks all affected files before writing and accepts only the recorded before or after state.
Recovery can itself fail or stop; the same command can resume it.
Journal checkpoints, replacement contents and directory updates use filesystem sync operations.
These guarantees assume the filesystem honors sync and atomic rename.
Directory locks coordinate `fr` writers. Another program can still race a check and its subsequent rename.
Changes must stay inside the selected workspace and must not traverse symlinks.
With a single file as `-C`, its parent owns the journal.

The native library's `edit::commit` retains handled-failure recovery without a persistent journal.
The browser workspace has no durable filesystem transaction.
A successful JSON write report follows the commit and includes its `transaction` identity.
A failed commit emits one error object. Run `fr history` to inspect pending recovery.
Syntax validation rejects new parser errors; compilation and behavior require their own checks.

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
| `--save-plan` | Store a change plan and return its transaction ID. Conflicts with `--write`. |
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

Each edge carries its confidence and its origin. The output marks an edge from
a hierarchy fan-out, and one through a function held in a field. A walk stops at an
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
depends on one still resolves.

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

Run a refactoring recipe: find, do, expect. Failed planning prevents the write.
Accepted plans use the shared commit and recovery path.
`--explain` prints what the recipe would do without doing it. The language is
documented in [RECIPES.md](RECIPES.md).

`--vocabulary` prints the language itself. Every verb with the form its arguments
take. The predicates a step takes for a symbol, and the fewer it takes for a
file. The rewrites this build has, and the languages. Every list comes from the
code that reads a recipe. With `--json`, a program writing a recipe reads what it
may write.

### `fr spec`

```
fr spec check [PATH...]
fr spec sync [PATH...] [--write]
fr spec verify [PATH...]
```

Check that Lean models still point at the declarations they model. With no path,
the command reads `kernels/` and `specs/`. A model names a declaration with an
anchor such as `-- fr:spec src/edit.rs::apply_to_string @ 3e192284`. The hash
comes from the declaration's full source span. A changed hash is stale. A gone or
ambiguous declaration is missing. The report also counts live `sorry` obligations.
`--json` returns every anchor as data. Stale and missing anchors exit unsuccessfully.

`sync` turns each stale hash into a reparse-checked Lean edit. It prints the exact
diff by default and commits every renewal together with `--write`. A missing target
stops the whole run before it writes, so a partly repaired spec cannot hide a broken
one. Sync renews the source identity only; it never guesses a Lean signature or edits
the declaration body and its proofs. Before commit, it rechecks every source declaration
it planned against.

An anchor can carry one `-- fr:signature` line. It goes before the Lean definition. Each
semicolon-separated part maps one source surface to the model surface, for example
`source: &str => source: String; return: Result<String> => return: Option String`.
`check` verifies both declarations against that explicit mapping. A stale or malformed
mapping also blocks `sync`.

Add `--strict` to require a mapping beside every anchor in the selected specs. This is
the CI mode for a kernel tree that treats a source hash alone as incomplete evidence.

`verify` always runs that strict correspondence check first. When it passes, the command
finds the `lakefile.lean` or `lakefile.toml` owning each selected spec and runs
`lake build --wfail` once per package. It writes nothing. JSON includes the strict report,
each package, its result, and Lean's output.

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
  their types where the source gave them. The output marks every construct without
  a counterpart, and the report counts it. The intermediary language
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
undeclared stays undeclared here, rather than invented.

## Housekeeping

### `fr project`

```sh
fr project map
fr project map src --depth 4 --limit 80
fr project map src/app.py --fields id,parent,kind,name,signature
fr project map --cursor '<NEXT>'
fr project show '<ID>' --revision '<REVISION>'
fr project show '<HANDLE>' --source --bytes 2048
fr project show '<HANDLE>' --source --offset 2048 --bytes 2048
fr project show '<HANDLE>' --relations --limit 40
fr project calls src --direction outgoing --limit 40
fr project calls '<HANDLE>' --direction incoming
fr project calls --cursor '<NEXT>'
fr project implementations '<ID>' --revision '<REVISION>'
fr project routes src --limit 40
fr project routes '<FILE_HANDLE>' --cursor '<NEXT>'
fr project contracts src --limit 40
fr project contracts '<FILE_HANDLE>' --cursor '<NEXT>'
fr project configuration deploy --limit 40
fr project configuration src/settings.py
fr project configuration --cursor '<NEXT>'
fr project tests
fr project tests src/service.py --depth 3 --limit 40
fr project tests '<ID>' --revision '<REVISION>' --depth 5
fr project packages --limit 40
fr project dependencies --manifest Cargo.toml --limit 40
fr project dependencies --cursor '<NEXT>'
fr project links --manifest Cargo.toml --limit 40
fr project links --cursor '<NEXT>'
fr project workspaces --limit 40
fr project workspaces --manifest crates/core/Cargo.toml
fr project gaps --limit 40
```

`fr project` returns compact JSON under schema `fr-project-1` in both output modes.
It leaves existing `symbols`, `refs`, `type` and other response shapes unchanged.
Maps describe directories, indexed files and lexical symbol containment.
They do not classify packages, frameworks or architectural layers.
Variables and parameters stay hidden unless `--locals` selects them.
Named functions inside another function remain visible.

Map responses contain `columns` and corresponding `rows` arrays.
Default columns are `id,parent,kind,name,line,children`.
Other fields are `handle,path,depth,language,exported,signature,qualifier`.
IDs and map parent IDs use hexadecimal strings within the response's revision.
Use a short ID with `--revision`, or join `handle_prefix` and the ID to obtain a full handle.
`--fields handle,...` emits full handles directly.
`map` accepts a path, a full handle, or a short ID with its revision.
A scoped map treats its selected node as the root; its parent cell is null.
Other parents can refer to rows on earlier pages. `children` counts all direct indexed children, including hidden locals.

`--depth` defaults to 3 and accepts 0 through 64. Limits accept 1 through 500 rows.
Maps default to 80 rows; other pages default to 40.
`page` states the total, returned count, earlier count, remaining count and next cursor.
`omitted` counts nodes hidden by depth and local-symbol filtering.
Reuse the same query and fields with a cursor. The page size may change.
Changed source, manifest content, inventory, scan options or query scope invalidates the corresponding handle or cursor.
A short ID without its revision cannot identify a symbol for `show`.

`show` returns a bounded signature and node metadata before any source body.
Its `position` gives the name’s 1-based line and column for existing refactoring targets.
Its `span` gives the definition’s absolute byte range in the file.
A `syntax-header` is the source prefix before a grammar-recognized body.
A `name-only` result means the reader could not recover that header.
Headers can contain defaults and attributes; they do not establish a complete semantic contract.
Qualifiers report the index's owner name separately from lexical containment.
Names cap at 160 UTF-8 bytes; paths and signatures cap at 512.
A clipped cell becomes `{text, omitted_bytes}` instead of a string.

`--source` adds up to 2048 bytes by default and accepts budgets from 4 through 65536.
Offsets count bytes from the selected symbol or file's start and must land on UTF-8 boundaries.
The response includes the absolute byte span and `next_offset`; use that value for the next page.
The final UTF-8 character may leave a page smaller than its budget.
A directory has no source slice or reference page.

`--relations` pages file imports and incoming/outgoing indexed references.
Each reference retains its kind, confidence and resolved target handle, or null when unresolved.
Outgoing references include nested source spans. Import records describe the containing file.
This view does not include call-graph dispatch expansion, route contracts or inferred architecture.
`coverage` reports indexed files, skipped files, unsupported files, syntax gaps and unresolved references.
`gaps` pages the corresponding diagnostics; unsupported extensions appear as counts.
Ignore rules and size limits bound discovery. Hidden files follow `--no-ignore`.
The workspace still requires indexing; output limits do not limit analysis to the returned nodes.

`calls` and `implementations` accept a directory, file, full handle or short ID with `--revision`.
Both default to the workspace root and 40 rows, with limits from 1 through 500.
Endpoints contain bounded names, qualifiers, paths, kinds, lines and handles for subsequent inspection.
They omit source bodies. Nested definitions participate in the selected scope.

`calls --direction outgoing` selects call sites within the scope; `incoming` selects callee definitions within it.
The default `both` combines these selections. A call within both appears once with `scope_relation: internal`.
Call rows preserve the graph's confidence and origin, including dispatch and function-value candidates.
`status: indexed-target` means the index supplied a target; its confidence still governs uncertainty.
Dispatch rows have `status: dispatch-candidate` and `dispatch_candidate: true`.
Unresolved sites have a null callee and appear only in outgoing or combined queries.
A site can have both unresolved and candidate rows. Indexed file-scope calls have a null caller and `caller_scope: file`.
Site offsets count absolute file bytes; line and column are 1-based.

`implementations` selects declarations within the scope and pages their hierarchy candidates.
Each row links a declaration to an implementation with `basis: hierarchy-analysis` and `status: candidate`.
Its `confidence` is null because the analyzer supplies no per-implementation confidence or runtime guarantee.
An empty result does not establish that no implementation exists.

Both queries include workspace-wide `analysis-gap` and `coverage-gap` rows in their paged items.
The `analysis` object gives workspace totals and unsupported-language counts, including missing hierarchy support.
These queries build the workspace hierarchy, and `calls` also builds its call graph.
Page limits bound output, not analysis work. Relationships themselves are not formally verified.
Cursors bind the selection, direction, revision and resulting rows; the page size may change.
The final source and inventory checks also apply before emitting a relationship page.

`routes` pages route declaration patterns in a directory or file, including a file handle or short ID with `--revision`.
It defaults to the workspace root and 40 rows; limits accept 1 through 500.
Symbol handles require selecting their containing file instead.
Each `route` row carries a method, normalized URL, declaration line, file handle and `framework_candidate`.
The readers recognize Express, Flask, Axum, Gin and Spring patterns, plus Next.js App Router and FastAPI subsets.
They do not verify framework identity or runtime reachability.
A Flask-style decorator without sufficient FastAPI evidence can still produce a `flask` candidate.
Methods and URLs describe the reader's interpretation, with `status: candidate` and null confidence.

For the five pattern readers, the handler summary counts callable declarations with the reader's name in the same file.
Its status is `unnamed`, `unresolved`, `candidate` or `ambiguous`.
Separate `route-handler` rows carry candidate handles with `confidence: name-only` and `basis: same-file-name`.
The route row's `id` joins these rows within the revision; it is not a handle for `show`.
Candidate rows can appear on another page. No handler body or unbounded candidate array accompanies a route.
URLs and paths cap at 512 UTF-8 bytes; handler names cap at 160, with explicit omitted-byte counts.

The `nextjs-app` reader recognizes `route.ts` and `route.js` beneath `app` or `src/app` at the selected project root or supported nested packages.
Top-level named function exports identify GET, POST, PUT, PATCH, DELETE, HEAD and OPTIONS candidates.
It preserves static segments, including `/api`, removes route groups and spells `[petId]` as `{petId}` without changing the parameter name.
Static segments accept letters, digits, hyphens, underscores, dots and tildes; names beginning with an underscore produce a private-directory gap.
Handler rows use `basis: declaration-span` and null confidence; names elsewhere in the file cannot supply these matches.
The reader preserves duplicate HTTP exports as separate declaration candidates and adds a diagnostic.
It does not infer implicit methods. These conventions follow the [Next.js route reference](https://nextjs.org/docs/app/api-reference/file-conventions/route).

Terminal `[...parts]` and `[[...parts]]` segments also supply route candidates, including beneath route groups.
The URL display uses `{...parts}` for catch-all and `{...parts?}` for optional catch-all segments; these are templates, not literal request URLs.
Malformed or repeated parameter names, nonterminal catch-alls, private, parallel and intercepting paths produce `analysis-gap` rows.

Local named exports such as `export { handle as GET }` follow matching top-level function declarations in the captured file.
Unaliased `export { GET }` also matches. Their route basis is `nextjs-app-local-function-export`.
The method comes from the export name; the handler name and position come from the function declaration.
Route lines identify the export specifier, while handler lines identify the declaration. Nested functions cannot supply local export matches.
Every matching declaration remains a candidate when names repeat; duplicate HTTP exports produce a diagnostic.
The reader does not verify lexical binding validity, reassignment, declaration merging or runtime reachability.
Type-only exports supply no HTTP candidates. Other export forms do not establish HTTP handlers.
Direct exported `const`, `let` and `var` bindings also match when the initializer is an arrow function or function expression.
Their route basis is `nextjs-app-variable-export`; local export aliases use `nextjs-app-local-variable-export`.
Matching uses the binding's name and position, including named function expressions whose inner name differs.
Multiple bindings and duplicate declarations remain separate candidates. The reader does not follow rebinding, initializer aliases or wrapper calls.
Parenthesized/asserted initializers, generators, destructuring and other unsupported initializers remain gaps.
Unresolved local exports, cross-file HTTP re-exports, plain star exports and default exports also produce gaps.
Files without supported HTTP exports report a gap. All Next.js diagnostics share the page limit.
`analysis.nextjs_gaps` counts these diagnostics; `analysis.nextjs_limitations` records the scope.
Pages Router and custom extensions remain outside this reader.
Package identity, layout precedence, route validity, `basePath` and rewrites remain unchecked.
Both root layouts produce candidates when both exist. Empty results do not establish a complete application route inventory.

Nested package layouts require a captured `package.json` with a nonempty string `next` entry in `dependencies` or `devDependencies`.
The nearest observed package manifest bounds each candidate layout; an inner package cannot borrow an outer package's dependency evidence.
Missing dependency declarations, malformed manifests and skipped manifests produce gaps for App Router-shaped files within observed nested packages.
Nested folders without an observed package boundary do not establish an app root. Peer/optional dependencies and hoisted dependency resolution remain outside this subset.
Next.js route rows carry `nextjs_project` with bounded root/manifest paths and `basis: observed-npm-next-dependency` for nested packages.
Project-root layouts retain `basis: project-root-layout`, root `.` and a null manifest, without requiring dependency evidence.
Other frameworks leave `nextjs_project` null. URLs start at the candidate app root; route paths and handles retain workspace-relative locations.
Package version validity, installation, configuration, workspace membership and runtime framework identity remain unchecked.
Both root layouts remain candidates under each package; the reader does not apply the [Next.js layout precedence rules](https://nextjs.org/docs/app/api-reference/file-conventions/src-folder).
Package metadata comes from the project snapshot; manifest changes invalidate revisions and cursors before output.

The `fastapi` reader recognizes top-level verb decorators on a direct `FastAPI()` or `APIRouter()` assignment.
The file must contain the corresponding top-level `fastapi` import; constructor and module import aliases also match.
Repeated direct assignments to a receiver exclude it from this subset. Conditional rebinding and general shadowing analysis remain unchecked.
One plain absolute string supplies the path, either positionally or through `path=`.
Escapes, concatenation, dynamic paths and method-list decorators produce `analysis-gap` rows for recognized receivers.
FastAPI evidence replaces the older Flask interpretation of the same decorator, including when the FastAPI reader reports a gap.
Handler matches use `basis: declaration-span`, with null confidence. Stacked decorators retain separate route IDs and metadata.
`analysis.fastapi_gaps` counts reader diagnostics; `analysis.fastapi_limitations` records the supported subset.
Imports supply syntax evidence only. Factories, nested definitions, prefixes, router includes and runtime framework identity remain unchecked.

Route analysis reads captured source only within selected files, after workspace indexing.
Syntax errors produce `analysis-gap` rows instead of route guesses from the broken file.
Unsupported languages produce count-based `coverage-gap` rows. Both diagnostics share the page limit.
`analysis` reports selected-file totals, files without patterns, reader names and interpretation limits.
The view has no expanded request/response schemas, middleware, mounted-router prefixes or cross-file handler resolution.
An empty page does not prove the absence of routes. Route and handler inference remain outside the Lean paging proofs.
Revision checks and query-bound cursors apply, including final source and inventory verification.

`contracts` extends the route view with partial request and response evidence from captured source.
It accepts the same directory/file scopes, handles, revision checks and page limits as `routes`.
Route IDs and handler candidate handles agree across both views; their cursors belong to separate queries.
Each `route-contract` summary counts request fields, response fields, handler candidates and contract gaps.
Every summary has `status: candidate`, `completeness: partial` and null confidence, even when its gap count is zero.

Separate `route-contract-field` rows carry the route ID, candidate handler, direction, location, line and evidence basis.
Simple whole-segment `{name}` URL markers supply path names, with no handler or inferred type.
Path names repeat only once per route. Complex markers produce a gap row.
Next.js catch-all path fields instead use `basis: nextjs-catch-all-path` and retain the original parameter name.
Their `segment_kind` distinguishes `catch-all` from `optional-catch-all`, with `min_segments` of one or zero and no finite maximum (`max_segments: null`).
This cardinality follows [Next.js dynamic-segment conventions](https://nextjs.org/docs/app/api-reference/file-conventions/dynamic-routes); declared types and wire requiredness remain null.
The reader carries path metadata before URL clipping, without reinterpreting display templates as source annotations.
Axum-style `Path<T>`, `Query<T>`, `Json<T>` and `Form<T>` parameters expose their declared type and payload type spelling.
Qualified extractor names also match. Aliases, optional wrappers and custom extractors remain unknown.
Spring-style `PathVariable`, `RequestParam`, `RequestBody`, `RequestHeader` and `CookieValue` annotations supply candidate request locations.
Literal annotation names and parameter bindings remain separate; dynamic, escaped, empty or conflicting explicit names stay null.
Without an explicit annotation name, the parameter binding supplies a candidate name.
Requiredness stays null. Defaults and annotation values other than binding names stay outside the output.
Extractor and annotation matches carry name-only confidence; the reader does not resolve their imports or types.

Declared handler return types produce response fields with null confidence.
For direct variable handlers, contract inspection reads parameters and return annotations from the arrow/function-expression initializer.
This also applies to same-file Express handler candidates; inline handlers and cross-file targets remain unresolved.
Bare arrow parameters also report unknown input bindings. A binding's callable type annotation does not supply an inferred initializer return type.
Java array dimensions after a name join the type spelling; Rust absolute type qualifiers remain intact.
Return types can describe wrappers, context values or implementation types; they do not establish the HTTP payload, status or media type.
Unsupported parameters and absent return annotations produce `route-contract-gap` rows.
Missing, inline and unsupported handler declarations also produce gaps. Ambiguous handlers retain separate fields and handles.
Names and bindings cap at 160 UTF-8 bytes; type spellings cap at 512, with omitted-byte counts.
Fields, summaries, gaps, route declarations and handlers all share the page limit.

FastAPI request fields recognize `Path`, `Query`, `Body`, `Header`, `Cookie`, `Form` and `File` calls.
Markers can occupy a default value or `Annotated` metadata; qualified names also match by their final component.
Exactly one supported marker must identify the binding. Conflicting markers, dependencies, aliases and implicit parameter classification remain unknown.
`binding_kind` preserves the marker name. Form and file markers use the body location without inferring a media type.
Literal aliases supply candidate names; header and body bindings without aliases leave the name null.
Dynamic, escaped, empty, competing or expanded alias arguments also leave it null. Requiredness stays null throughout.
The reader strips `Annotated` metadata before emitting the type; defaults, descriptions and validation arguments stay outside the output.
Unsupported type expressions leave `declared_type` null. These patterns follow FastAPI's [parameter declarations](https://fastapi.tiangolo.com/tutorial/body-multiple-params/).

Explicit `response_model` arguments produce separate response fields with `basis: fastapi-response-model` and `location: response-model`.
Names, qualified names, generic subscriptions, unions and `None` form the supported type subset, with a depth limit of 16.
`model_state` distinguishes a declared model from explicit `None`, which denotes disabled model handling.
Python return annotations remain separate fields; the decorator model takes precedence in FastAPI's [response-model rules](https://fastapi.tiangolo.com/tutorial/response-model/).
Runtime framework identity and wire correspondence remain unchecked, and every summary stays partial.
Computed models and competing arguments produce gaps. Expanded decorator options report potentially missing response metadata.
Schema fields, status codes, serialization, model validation and implicit request classification remain outside this view.
Use `project schemas` on the route's file to inspect supported declarations separately.

This view reads signatures without analyzing handler bodies or expanding schema definitions.
Next.js candidates expose path names and declared return types through the same contract rows; request/context bindings remain unknown.
The existing route-reader limitations still apply, including mounted routers and framework uncertainty.
Output limits do not bound workspace indexing or selected-file analysis work.
The Lean page-length laws apply; contract extraction, wire correspondence and type resolution remain outside those proofs.

`contracts --types` adds bounded type-name references from supported declared handler returns and Axum request parameter types.
The type reader supports Python, TypeScript/TSX and Rust syntax. Other return languages and unsupported expressions produce `route-contract-gap` diagnostics.
FastAPI marker types, decorator response models and Spring request types remain outside reference inspection.
Ordinary contract pages omit these reference rows. `analysis.type_references_requested` records the mode; cursors cannot cross modes.

```sh
fr project contracts src/app.rs --types --limit 12
fr project schemas '<type-candidate-handle>' --limit 12
```

Inspected fields gain a revision-bound `id` and `type_reference_count`; a null count denotes unsupported syntax, while zero denotes no reference names.
Fields outside this inspection subset have neither field. Supported spellings use the schema reader's depth limit and syntax rules.
Matching uses full names before output clipping. Existing bounded type spellings remain on the contract fields even when reference inspection reports a gap.
Separate `route-contract-type-reference` rows join the route and field IDs, with bounded names, candidate counts and unresolved/candidate/ambiguous status.
Separate `route-contract-type-candidate` rows join each reference ID and provide declaration handles with name-only confidence.
Only classes, interfaces, type aliases and structs in the same file supply candidates; lexical scope, imports, qualified names and namespaces remain unresolved.
Repeated names within one field produce one reference; duplicate declarations remain separate candidates, with no recursive expansion.
Follow candidate handles through `schemas` or `show`. A candidate does not establish the serialized payload or runtime schema identity.
Fields, references, candidates and gaps share the page limit; the existing revision and snapshot checks apply.

`schemas` pages direct declared fields and type-reference candidates from captured Python, TypeScript/TSX and Rust source.
It accepts directory, file and symbol scopes, full handles, or short IDs with `--revision`.
Symbol scopes include supported declarations within that symbol; a schema handle selects its declaration and any nested declarations.
The default scope is the workspace root, with 40 rows and limits from 1 through 500.

```sh
fr project schemas src/models.py --limit 12
fr project schemas '<schema-declaration-handle>' --limit 12

fr project schemas src/models.py --cursor '<next-cursor>' --limit 12
fr project show '<type-candidate-handle>' --source --bytes 512
```

Python classes expose direct annotated assignments. TypeScript interfaces and direct object type aliases expose property signatures.
Rust named structs expose field declarations; unit structs have no declared fields, while tuple structs and Rust aliases produce gaps.
Classes need not inherit from a known model library; these are declaration candidates, with no inferred runtime schema identity.
Every `schema` summary has a declaration endpoint, field/reference/gap counts, `completeness: partial`, `status: candidate` and null confidence.
Separate `schema-field` rows join its declaration handle through `schema`, with their own revision-bound `id`.
Fields carry names, lines and supported type spellings. TypeScript `optional_marker` and `readonly_marker` record syntax only.
Python and Rust leave those markers null. Wire requiredness remains null throughout.

Supported types include names, qualified names, generic applications, unions and TypeScript arrays, tuples and intersections.
Rust also supports primitive types, references, pointers, slices and tuples, including lifetime/mutability spelling without lifetime reference rows.
Rust array-length expressions, const literals in generic arguments, associated bindings and raw type identifiers remain outside this syntax subset.
Type traversal stops beyond depth 16. Top-level Python `Annotated` exposes its first type argument without metadata.
Quoted forward references, literal values, computed types, inline object/function types and types containing comments remain unknown.
Unsupported type syntax leaves `declared_type` null and produces a gap; the reader emits no partial references for that field.
Defaults, validation arguments, decorators and method bodies stay outside the output.

Each `schema-type-reference` records a field ID, full-name matching basis and candidate count.
It has `status: unresolved`, `candidate` or `ambiguous`; repeated names within one field produce one reference row.
Separate `schema-type-candidate` rows join the reference ID and carry followable declaration endpoints with name-only confidence.
Matching uses the full spelling before output clipping and retains every matching class, interface, type alias or struct in the same file.
It does not resolve imports, qualified names, lexical scope, builtins or type parameters. Candidates can lie outside the selected symbol.
Duplicate declarations remain separate, including TypeScript interface declarations that may merge under compiler rules.
References do not recursively expand definitions, so cycles require no special traversal.
Use the candidate handle with `schemas` or `show`; field/reference IDs only join rows within a revision.

Every supported schema has a gap for unchecked wire names, requiredness, validation, serialization and runtime identity.
Additional `schema-gap` rows cover inheritance, generics, decorators, duplicate fields and unsupported members or declaration forms.
Python class/instance distinctions and inherited or computed fields remain unchecked.
Rust attributes, derives, `cfg`, visibility, generic bounds and const/type namespaces remain unchecked; attribute values stay outside the output.
Runtime schema builders such as Zod calls do not supply declared fields in this subset.
Syntax errors skip the file with an `analysis-gap`; unsupported languages produce count-based `coverage-gap` rows.
Empty results do not establish a complete schema inventory. `contracts --types` supplies optional declaration candidates without resolving types or wire models.
Names cap at 160 UTF-8 bytes, type spellings and endpoint paths at 512, with explicit omitted-byte counts.
Fields, references, candidates, summaries and diagnostics share the page limit, with query/scope/revision-bound cursors and final snapshot checks.
Output limits do not bound indexing or selected-file analysis work.
The shared Lean page-length laws apply; schema extraction and type resolution remain outside those proofs.

`configuration` pages environment declarations and candidate consumers across indexed files.
It accepts a directory, file, file handle or short ID with `--revision`; symbol handles require selecting their containing file.
The default scope is the workspace root, with 40 rows and limits from 1 through 500.
Selecting a declaration file or a candidate values file includes all matching consumers in the workspace.
Selecting a code file includes its candidate reads and their declarations, including declarations outside that file.

`config-declaration` rows preserve competing manifest declarations separately, with `basis: manifest-pattern` and `confidence: name-only`.
Each row has a revision-bound correlation ID, variable name, declaration site and total/selected consumer counts.
`config-consumer` rows join that ID and provide a file handle, line, language and the analyzer's name-only confidence.
These rows share the page limit; consumers can appear on a later page.
Correlation IDs join rows only; use file handles with `show` and declaration/read lines to narrow further inspection.
Reads without a matching declaration have a null declaration and `status: no-observed-declaration`.
Declarations without matching reads have `consumer_status: no-observed-consumer`. Neither status proves absence outside this analysis.

The reader covers YAML/Helm environment lists, Compose environment mappings/lists and selected uppercase accessor patterns in code.
Accessor text in comments or strings can produce candidates, and overlapping patterns can produce duplicate sites.
Dynamic and lowercase names can remain unseen. Name equality does not establish deployment identity, runtime use or precedence.
Helm declarations retain a bounded condition and dotted values path, plus the number of path components.
A `values_file_candidate` uses the nearest ancestor file containing the leaf key; its line is null.
Its `nearest-ancestor-leaf-name` basis does not validate the complete path, overlays or deployment inputs.
Variable names cap at 160 UTF-8 bytes; paths, conditions and dotted values paths cap at 512, with omitted-byte counts.
The response omits literal environment values and full source lines.

Configuration analysis consumes captured source and checks its indexed content hash before using source spans.
Missing, mismatched or syntactically broken inputs produce analysis gaps and cannot supply declarations, reads or values-file candidates.
Workspace-wide analysis and language gaps share the page; `analysis` includes workspace totals and selected relationship counts.
Cursors bind the revision, scope and resulting rows. Final source and inventory verification still applies.
Output limits do not bound workspace analysis work. The shared paging proofs do not verify configuration inference or source/model correspondence.

`tests` pages test-entry candidates in a directory, file or symbol scope, including nested definitions.
It accepts full handles and short IDs with `--revision`, and defaults to the workspace root and 40 rows.
Limits accept 1 through 500. `--depth` limits call-path length, defaults to 3 and accepts 0 through 16.
Candidates within the selection appear with `basis: catalog-in-scope`, zero hops and null path confidence.
Candidates outside the selection appear when a graph path reaches a selected callable definition within the depth limit.
Each such row has `basis: catalog-and-call-path`, a target handle, hop count and one shortest witness.
At depth zero, only in-scope candidates appear.

Each `test-candidate` preserves its built-in catalog rule and an inspection handle, with `status: candidate` and null detection confidence.
The catalogs include fixtures, setup hooks and convention-matched helpers. A candidate does not establish runner discovery or execution.
`path_confidence` takes the weakest confidence along the chosen witness; it does not strengthen any edge.
Alternative paths may have different confidence. Separate `test-path-edge` rows carry the candidate ID, step number, endpoints, call site, confidence and origin.
Dispatch flags survive unchanged. Witness rows share the page limit and can appear on later pages.
Candidate IDs join rows only; use endpoint handles for `show` or `calls`.
Names and rules cap at 160 UTF-8 bytes; paths cap at 512. These pages omit source bodies.

Test-rule detection uses captured source, verifies its indexed hash and reports missing, mismatched or broken inputs as catalog gaps.
Only built-in test rules participate; this query does not load external catalogs or Python packaging entry points.
The call graph uses fresh workspace hierarchy analysis, as `project calls` does, with final source and inventory checks before output.
Catalog gaps, hierarchy gaps and unsupported-language counts share the page as workspace diagnostics.
`analysis` also reports unresolved and file-scope call counts, which do not extend witnesses.
`depth_frontier_nodes` counts visited nodes at the limit with callers outside the visited set; it does not count omitted tests.
Cursors bind the revision, selection, depth, resulting rows and analysis metadata; page sizes may change.
Page and depth limits do not bound workspace indexing or graph construction.
Empty results and call paths do not establish runtime coverage. Catalog matching and reachability remain outside the confidence and paging model proofs.

`packages` pages discovered `Cargo.toml` and `package.json` manifests.
Each row reports the manifest path, its directory root, ecosystem, declared name/version and declaration count.
Virtual Cargo workspaces have `package_declared: false`.
These roots describe manifest locations; they do not assign source ownership or establish workspace membership.
Discovery stays within the selected scan root and does not search its ancestors.
It respects ignore rules, hidden-file settings and `--max-file-size`, and excludes `.fr-history`.
Discovery skips symlink manifests. Manifest changes participate in the shared revision and final snapshot check.
TOML manifests remain outside the source syntax index; their extension may still appear in source coverage gaps.

`dependencies` pages declarations from all discovered manifests, or from one selected with `--manifest PATH`.
Cargo rows cover ordinary, development, build, target-conditioned and workspace dependencies.
They retain aliases, version requirements, paths, Git selectors, registry names and inheritance/optional flags.
Feature lists become `feature_count`; unknown dependency fields become `unreported_fields` counts.
npm rows cover dependencies, devDependencies, peerDependencies and optionalDependencies.
Requirements stay literal, including `file:` and `workspace:` strings.
Workspace member patterns remain separate rows with `expanded: false`.
The reader includes Cargo exclude/default-member patterns and npm array or `workspaces.packages` forms.
Names and versions cap at 160 UTF-8 bytes; patterns, selectors and requirements cap at 512.

Every declaration has `basis: manifest-declaration`; dependency rows have `resolution: not-attempted`.
The declaration view does not invoke package managers, read lockfiles, follow dependency paths or expand globs.
It does not resolve workspace inheritance, evaluate target conditions or apply overrides, patches and feature activation.
Malformed manifests and unsupported shapes in inspected fields produce paged manifest diagnostics in `gaps`.
Affected dependency rows carry `declaration_status: partial` or `unsupported`.
`coverage.manifests` counts discovered manifests, parsed records and diagnostics.
This reader extracts selected fields; it does not validate complete package-manager schemas.
Pagination uses the existing Lean-checked page-length kernel. Manifest extraction has no formal proof yet.

`links` pages local manifest links and workspace member-pattern matches.
It accepts the same `--manifest`, `--limit` and revision-bound `--cursor` options as `dependencies`.
Matching uses full manifest values before clipping output; labels and paths retain the existing byte limits.
Links use only manifests in the current snapshot. They do not run package managers or read additional dependency paths.

For Cargo, `local-dependency` rows inspect explicit `path` fields in package, target and workspace dependency sections.
A `linked` row identifies a discovered package manifest whose name matches the dependency name or explicit `package` alias.
For npm, `file:` and `./` or `../` directory specifiers identify discovered package manifests; dependency aliases may differ from target names.
`target_manifest` and `target_name` describe that local target.
`version_check: not-performed` means the link does not establish version compatibility or an installed dependency.
The view preserves target conditions without evaluating them. Cargo inherited paths use the observed membership described below.
Registry requirements, Git dependencies and npm `workspace:` protocols remain in the declaration view.

Paths resolve relative to their declaring manifest directory and stay inside the selected project root.
Every traversed directory must occur among the discovered manifests' ancestors, including before a `..` step.
Symlinks, ignored or missing directories, unreadable target manifests and escaping paths cannot produce confirmed links.
Absolute paths, home expansion, encoded paths and platform-specific path syntax remain unsupported.
Unresolved rows state a reason; missing observations do not establish that a dependency is absent from the filesystem.

`workspace-member-match` rows compare Cargo/npm member patterns with observed package directories.
Supported patterns contain literal components and whole-component `*` wildcards, such as `crates/*`.
Each wildcard matches one directory level. `**`, partial wildcards, character classes, alternatives and parent traversal remain unsupported.
Cargo exclude patterns use the same matcher; unsupported exclusions keep matching candidates unresolved.
Rows report `matched`, `excluded` or `unresolved`, with the source pattern and target or candidate manifest.
Overlapping patterns retain separate evidence rows. Unmatched patterns remain visible.
Cargo nested workspaces and explicit `package.workspace` ownership prevent confirmed pattern matches.

Every pattern-match row retains `membership: candidate`.
The separate workspace view checks observed Cargo ownership and membership.
These rules extend beyond glob matching; see the [Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html).
The Lean matcher model proves depth preservation and literal-or-star matching, with 67,081 Rust/Lean comparison cases.
It does not prove filesystem interpretation, package-manager membership or Rust refinement for every input.

`workspaces` pages parsed Cargo package ownership and virtual workspace roots.
Its manifest filter, limits and cursors work like `links`.
Ownership uses a package's own workspace table, an explicit `package.workspace` pointer, or the nearest observed ancestor workspace.
Conflicting declarations, unavailable roots and unsupported pointers remain unresolved.
The selected project root bounds every lookup; a missing observation does not establish that a package is standalone.

The reader checks ancestor manifest metadata inside that scope, including ancestors excluded by ignore rules.
It does not read excluded contents. An unavailable ancestor blocks inheritance through it and appears in `gaps`.
These observations participate in the snapshot revision and final verification.

Membership starts with root packages and declared members under the supported pattern and exclusion rules.
It follows observed local dependency paths transitively, including inherited paths, to find automatic members inside the same workspace.
Unlisted packages do not acquire membership through directory containment alone.
Unused workspace dependency definitions do not add members. Cycles terminate after membership stops growing.
Member rows identify their ownership basis, membership basis and, for automatic members, the referring manifest.
The result describes observed manifests; `validation: package-manager-unchecked` preserves its limits.

For an observed member, `links` reads `workspace = true` dependencies from its owner's workspace dependency table.
Local paths resolve relative to that workspace root; dependency aliases retain the declared target package name.
Inherited rows include `workspace_manifest` and `membership_basis`.
Missing definitions, unsupported overrides, nonlocal definitions and unresolved membership produce explicit reasons.
Version compatibility and feature evaluation remain unchecked.
These rules follow the [Cargo inheritance reference](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#inheriting-a-dependency-from-a-workspace).

Default-member selection, member patterns with parent traversal, broader globs and npm workspace ownership remain pending.
The membership closure has regression tests and Cargo metadata comparisons, but no formal proof yet.

### `fr history`

```sh
fr rename OldName NewName --save-plan --json
fr history
fr history show 1
fr history patch 1 > /tmp/change.patch
fr history patch 1 --reverse > /tmp/reverse.patch
fr history apply 1                  # preview
fr history apply 1 --write
fr history undo 1 --write
fr history redo 1 --write
fr history recover 1 --write        # only when an operation remains pending
```

History uses schema 1 and numeric identities local to the workspace.
`history` lists status, validation labels, paths, applied IDs and the redo stack.
`patch` prints a Git text patch, or metadata with a `patch` string under `--json`.
Other history commands print JSON in both output modes. `show` and transition previews include diffs and existence/mode changes.
Patch export reads recorded snapshots and leaves history, working files and the Git index unchanged.
Use `git apply --check /tmp/change.patch` to check application in the receiving workspace.
See [recorded transaction patches](docs/git-patches.md) for reverse export, permissions and text scope.
Applying a saved plan checks its affected-file snapshots and its project source digest.
That digest covers recognized source files, including hidden and ignored files.
It excludes `.git`, `.fr-history`, `target`, `node_modules` and `.lake` directories.
It does not establish build, dependency or behavioral equivalence.
A changed source digest requires a fresh plan.
Undo and redo check only the affected files, preserving unrelated edits.

Undo requires the latest applied ID. Redo requires the next ID on the redo stack.
Saving a plan preserves the redo stack. Applying a new plan clears that stack and marks its old entries `abandoned`.
Completed records keep their snapshots. There is no automatic pruning in schema 1.
The journal contains full source text, resides in a private directory and ignores its own contents in Git.
Deleting `.fr-history` discards all saved plans and recovery data; retain it while an operation needs recovery.
The workspace scanner excludes this directory even with `--no-ignore`.
A killed process can leave temporary staging files beside source files; recovery restores targets but leaves those orphaned temporary files.
History restores file contents, existence and permission modes. It does not restore timestamps, ownership, extended attributes or empty directory topology.

### `fr cache`

```
fr cache [--clear]
```

Inspect or clear the fact cache. The cache keys an entry by content and by
the version of everything that could change a fact, so a stale answer is not
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
