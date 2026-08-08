# Every refactoring, on real code

One example per capability, each run against a public repository at a pinned commit.
The outputs are copied from those runs. Nothing here is invented, and where a command
refused or found nothing, that is what it printed.

| Repository | Commit | Languages used here |
|---|---|---|
| [helm/helm](https://github.com/helm/helm) | [`a8ab76e`](https://github.com/helm/helm/commit/a8ab76e) | Go, Helm, YAML |
| [BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep) | [`435f59f`](https://github.com/BurntSushi/ripgrep/commit/435f59f) | Rust |
| [psf/requests](https://github.com/psf/requests) | [`414f051`](https://github.com/psf/requests/commit/414f051) | Python |
| [terraform-aws-modules/terraform-aws-vpc](https://github.com/terraform-aws-modules/terraform-aws-vpc) | [`3ffbd46`](https://github.com/terraform-aws-modules/terraform-aws-vpc/commit/3ffbd46) | Terraform |
| [zigtools/zls](https://github.com/zigtools/zls) | [`8da87d4`](https://github.com/zigtools/zls/commit/8da87d4) | Zig |
| [grafana/grafana](https://github.com/grafana/grafana) | [`b3648173`](https://github.com/grafana/grafana/commit/b3648173) | TypeScript, TSX, SCSS |

Eleven of the examples below are bugs this exercise found and fixed. They are marked
🔎, because "we ran it on real code and this is what happened" is the only honest way
to describe a tool's behaviour.

---

## Understanding

### `fr refs` — where is this used, and how sure are we

Rust reaches a module item through a path. `super::` is the module a file's directory
forms, and the four uses live in two sibling files:

```console
$ fr refs crates/core/flags/doc/mod.rs:18:4
4 reference(s) to render_custom_markup
  crates/core/flags/doc/help.rs:169:22  [exact]
  crates/core/flags/doc/help.rs:178:22  [exact]
  crates/core/flags/doc/man.rs:72:22  [exact]
  crates/core/flags/doc/man.rs:84:22  [exact]
```

🔎 This returned **zero** before. Rust module paths were not resolved at all, so
`super::render_custom_markup(…)` matched nothing and the function read as dead code.

The harder case is a name four types share.
[`hiargs.rs`](https://github.com/BurntSushi/ripgrep/blob/435f59f/crates/core/flags/hiargs.rs)
declares `from_low_args` on `HiArgs`, `Patterns`, `Paths` and `BinaryDetection` — all
in one file:

```console
$ fr refs crates/core/flags/hiargs.rs:1016:8
1 reference(s) to Patterns::from_low_args
  crates/core/flags/hiargs.rs:153:34  [exact]
```

🔎 The written type is the evidence. Before, the nearest definition in the file won,
which meant one of the four absorbed the others' call sites and three looked unused.

### `fr callers` — who calls this

```console
$ fr callers src/requests/utils.py:160:5 --depth 2
super_len
  PreparedRequest::prepare_body
  PreparedRequest::prepare_content_length
  TestSuperLen::test_io_streams
  TestSuperLen::test_super_len_correctly_calculates_len_of_partially_read_file
  TestSuperLen::test_super_len_handles_files_raising_weird_errors_in_tell
  TestSuperLen::test_string
  TestSuperLen::test_file
  TestSuperLen::test_tarfile_member
```

### `fr impact` — what could a change here touch

Wider than the call graph: every reference, every textual occurrence, every file that
would need rereading.

```console
$ fr impact src/requests/utils.py:160:5
super_len affects 89 site(s) across 5 file(s) and 1 language(s)

Would definitely change (34):
  reference      src/requests/models.py:81:5
  reference      src/requests/models.py:605:26
  reference      src/requests/models.py:657:22
  reference      tests/test_utils.py:39:5
  …
```

### `fr entrypoints` — where execution starts

```console
$ fr entrypoints
   516 test
    83 exported-api
    60 doc
     5 cli-main
```

🔎 That was 141 tests. A Rust test declares itself with `#[test]`, and the catalog
could only match names and paths — ripgrep's tests are called `backslash`, `tab` and
`carriage`. Catalogs gained `annotated_with`, which reads the annotations above a
definition.

### `fr flow` — where a value came from and where it goes

Terraform, on the module's `azs` variable:

```console
$ fr flow fwd variables.tf:47:11
declaration var.azs: variable "azs" { …  (variables.tf:47)
  use count = local.create_public_subnets && (… || local.len_public_subnets >= length(var.azs)) ? …  (main.tf:146)
  use availability_zone = length(regexall("^[a-z]{2}-", element(var.azs, count.index))) > 0 ? …  (main.tf:151)
  use format("${var.name}-${var.public_subnet_suffix}-%s", element(var.azs, count.index))  (main.tf:167)
  use lookup(var.public_subnet_tags_per_az, element(var.azs, count.index), {})  (main.tf:172)
  …
```

🔎 `fr refs` on the same variable used to resolve it to the module's own
`output "azs"` — a different Terraform namespace that no traversal can reach. A rename
would have renamed the output and rewritten all 41 uses of the variable to match.
`var.` is written in the source; it is now read.

And Helm, where a value's origin is a values file and the answer is honest about what
it cannot see:

```console
$ fr flow back pkg/cmd/testdata/testcharts/alpine/templates/alpine-pod.yaml:4:38
declaration Name: my-alpine  (…/alpine/values.yaml:1)

Stopped at:
- 'values key Name' can still be overridden externally, from `-f` files and `--set` on the helm command line
- origin: literal value my-alpine
```

---

## Finding work

### `fr duplicates` — code written more than once

[`flags/defs.rs`](https://github.com/BurntSushi/ripgrep/blob/435f59f/crates/core/flags/defs.rs)
defines every ripgrep flag as a struct impl. Nineteen of them are the same shape:

```console
$ fr duplicates --language rust --min-tokens 120
19 copies, 137 tokens each (2466 redundant) — rust
  crates/core/flags/defs.rs:2673-2700
  crates/core/flags/defs.rs:3351-3379
  crates/core/flags/defs.rs:3409-3438
  crates/core/flags/defs.rs:4196-4230
  …
```

The comparison is structural, so copies whose identifiers differ still match — which
matters here, since a textual search finds none of these.

Zig, on the language server's test suite:

```console
$ fr duplicates --language zig --min-tokens 100
7 copies, 109 tokens each (654 redundant) — zig
  tests/lsp_features/semantic_tokens.zig:334-343
  tests/lsp_features/semantic_tokens.zig:344-353
  tests/lsp_features/semantic_tokens.zig:354-363
```

And helm, whose `v2` and `v3` package trees are a fork of one another:

```console
$ fr duplicates --language go
337 duplicated block(s), 64530 redundant token(s)
```

The largest is `internal/release/v2/info_test.go` against `pkg/release/v1/info_test.go`
— 377 lines whose only differences are the package clause and one blank line.

### `fr unused` — code nothing appears to use

```console
$ fr unused --language go --internal        # helm
47 symbol(s) with no detected use, of 427 found across the workspace
```

Thirty-nine unused parameters and eight unused struct fields; no dead functions,
methods or variables. helm has very little dead code.

🔎 The same command reported **238** candidates before this exercise. The difference
is eight resolution bugs, the largest being that a Go package is a directory and only
Terraform was treated that way — so `fr refs` returned nothing for symbols helm calls
from the file next door.

`--internal` matters for a library: `--language go` alone reports 199 exported
symbols, which is the public API, not dead code.

---

## Changing

Every command prints a diff and writes nothing without `--write`.

### `fr rename` — the symbol and everything pointing at it

```console
$ fr rename pkg/action/action.go:725:6 releaseApplyMethod
-func determineReleaseSSApplyMethod(serverSideApply bool) release.ApplyMethod {
+func releaseApplyMethod(serverSideApply bool) release.ApplyMethod {
-		ApplyMethod: string(determineReleaseSSApplyMethod(i.ServerSideApply)),
+		ApplyMethod: string(releaseApplyMethod(i.ServerSideApply)),

determineReleaseSSApplyMethod → releaseApplyMethod: 5 site(s) across 4 file(s)
```

`TestDetermineReleaseSSAApplyMethod` keeps its name — it contains the old one and is
not it. Verified afterwards with `go build ./...` and `go test ./pkg/action/`.

A Helm values key renames through the templates that read it:

```console
$ fr rename testcharts/alpine/values.yaml:1:1 appName
-  name: "{{.Release.Name}}-{{.Values.Name}}"
+  name: "{{.Release.Name}}-{{.Values.appName}}"

Not changed — review these yourself:
  textual-occurrence (1):
    alpine-pod.yaml:3:17  'Name' appears in a string or comment; left unchanged
```

🔎 `{{ … }}` is masked before parsing, so everything inside it was invisible to the
index: this rewrote `values.yaml` and nothing else, listing every template use as a
textual occurrence to fix by hand.

### `fr extract` — an expression into a binding

```console
$ fr extract pkg/action/install.go:221:5-221:20 itemCount
 	}
-	if len(totalItems) > 0 {
+	itemCount := len(totalItems)
+	if itemCount > 0 {
```

🔎 This put the binding at the top of the function — above the declaration of
`totalItems` — until the third private copy of "is this a statement container" was
merged with the other two. It parses, so no reparse check caught it; it simply does
not compile.

### `fr move` — a symbol, with what it needs

```console
$ fr move src/requests/utils.py:283:5 src/requests/naming.py --write
```

```python
# naming.py
from __future__ import annotations
import os
from typing import Any

def guess_filename(obj: Any) -> str | None:
    …
```

🔎 Three faults, all found here. The new import in `utils.py` was written *inside*
`from typing import (` — requests spans that statement over three lines and the
insertion point was found by scanning lines. `import os` stayed behind, because a
module import binds a name without naming it in the statement. And
`from __future__ import annotations` stayed behind too: it binds nothing at all and
decides how every annotation in the file is read, so `str | None` stopped parsing
without it.

### `fr signature` — parameters, and every call site

On grafana's `packages/grafana-data`, swapping two parameters of a function used
across four files:

```console
$ fr signature packages/grafana-data/src/field/scale.ts:19:17 move:0:1
-export function getScaleCalculator(field: Field, theme: GrafanaTheme2): ScaleCalculator {
+export function getScaleCalculator(theme: GrafanaTheme2, field: Field): ScaleCalculator {
-    expect(colorOf(field, 200)).toEqual(getScaleCalculator(field, theme)(-Infinity).color);
+    expect(colorOf(field, 200)).toEqual(getScaleCalculator(theme, field)(-Infinity).color);

getScaleCalculator: moved parameter 0 to position 1, updating 10 call site(s)
```

### `fr rewrite` — local transformations

```console
$ fr rewrite crates/cli/src/decompress.rs:477:9
guard-clause   return early instead of nesting the body

$ fr rewrite crates/cli/src/decompress.rs:477:9 guard-clause
-        if abs_prog.extension().is_none() {
-            for extension in ["com", "exe"] {
-                …
+        if !abs_prog.extension().is_none() {
+            continue;
+        }
+        for extension in ["com", "exe"] {
```

🔎 That `continue` was `return` until this example was written. The `if` ends a `for`
body inside a function returning `Result<PathBuf>`, so `return` left the loop
entirely *and* returned nothing from a function that owes a value. The exit now
follows from the block, and a function that owes a value is refused outright — what
to return early is the author's decision.

Only transformations whose result reparses are offered, so the menu never lists
something that applying it would then refuse.

### `fr restructure` — a pattern, everywhere it appears

Adding `from None` to exception re-raises, which is the idiom for suppressing a
chained traceback:

```console
$ fr restructure 'raise InvalidURL($X)' 'raise InvalidURL($X) from None' --lang python
         except LocationParseError as e:
-            raise InvalidURL(*e.args)
+            raise InvalidURL(*e.args) from None
```

🔎 Statement patterns were impossible in Python, shell and YAML. Those languages wrap
a fragment in nothing, so the statement the pattern writes is the outermost node — and
the narrowing that strips wrapper-introduced statement containers stripped that one
too.

### `fr delete` — and the refusal that matters more

```console
$ fr delete pkg/action/action.go:725:6
Error: refusing to delete 'releaseApplyMethod': 4 reference(s) still resolve to it
  pkg/action/action_test.go:2280:54
  pkg/action/action_test.go:2281:54
  pkg/action/install.go:676:23
  pkg/action/rollback.go:192:23
Remove or repoint these uses first; nothing was changed.
```

### `fr imports`, `fr inline`, `fr remove-flag`, `fr stitch`

`fr imports <file>` drops unused imports and sorts the rest, holding back the ones a
language brings into scope invisibly — Python `__future__` imports and dotted
registration imports, TypeScript type-only imports and JSX pragmas, Go blank imports.
`fr inline` is the reverse of `fr extract`, for a binding or a call. `fr remove-flag`
retires a feature flag and the branch that only served it. `fr stitch` links a
configuration key to the code that reads it — an environment variable declared in a
chart and read by a Go program.

---

## Not supported, and what each would take

These are refactorings the tool does not do. They are listed because the shape of
what is missing says more about a tool than the list of what it has.

| Refactoring | Why not, and what it needs |
|---|---|
| **Extract interface / trait** | Needs to decide which members belong to the abstraction, which is a design decision rather than a mechanical one. The mechanical part — finding every implementor — already exists as `fr implementations`. |
| **Pull up / push down a member** | Needs the type hierarchy *and* the type of every receiver at every call site, to know which sites still resolve after the move. Hierarchy analysis exists; receiver types do not. |
| **Introduce parameter object** | Mechanically an `fr signature` change plus a new type, but choosing which parameters group together is the substance of it. A version taking an explicit list is the likeliest of these to be built. |
| **Change a return type** | The edit is easy; finding every caller that must adapt needs the type of each call's context, which syntax does not give. |
| **Convert callback to promise / async** | Requires understanding control flow, not just shape. Each language spells it differently enough that it is really eight refactorings. |
| **Encapsulate a field** | Needs to distinguish reads from writes at every use site, which is dataflow rather than resolution. `fr flow` has the machinery; the refactoring does not exist yet. |
| **Rename a file or module** | Every language spells the dependency differently — a Go directory, a Rust `mod`, a TypeScript relative path, a Python package. `fr move` already does this for a *symbol*; doing it for a file is the same work at a different granularity, and is the second-likeliest to be built. |
| **Inline a class or type** | Needs to know every use is compatible with the inlined shape, which is type checking. |
| **Extract a superclass** | As with extract interface: the mechanical part is small and the judgement is the task. |
| **Split a class or module** | The tool can *find* the case for it — `fr duplicates` and `fr graph` show cohesion — but performing the split is a sequence of moves a human should direct. |

The common thread: everything above needs types, and this tool is built on syntax. It
stops where the syntax stops and says so, which is why a reference it cannot prove is
reported rather than rewritten. A refactoring that needs the type of an arbitrary
expression belongs in a language server; one that needs only what is written down
belongs here, across all sixteen languages at once.

---

## Reproducing any of this

```console
$ git clone https://github.com/BurntSushi/ripgrep && cd ripgrep
$ git checkout 435f59f
$ fr duplicates --language rust --min-tokens 120
```

Every command above is copy-pasteable against the commit in the table. See
[TUTORIAL.md](TUTORIAL.md) for a single refactoring followed end to end, and
[BUGS.md](BUGS.md) for the 🔎 entries with their measurements.
