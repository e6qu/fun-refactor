# Refactoring a real codebase with fun-refactor

Every command below ran against [helm/helm](https://github.com/helm/helm) at commit
[`a8ab76e`](https://github.com/helm/helm/commit/a8ab76e). That repository holds 539
Go files, and it also carries 669 Helm templates, 95 Markdown documents and 83 plain
YAML files. The outputs come from those runs. Where a number looks odd, the tool
said it.

Get the repository and the binary:

```console
$ git clone https://github.com/helm/helm.git
$ cd helm && git checkout a8ab76e

$ git clone https://github.com/e6qu/fun-refactor.git ../fun-refactor
$ cargo install --path ../fun-refactor    # or: cargo build --release
```

---

## 1. What is in here?

Start by asking what the tool can read. It indexes nothing until you ask for
something. Asking first is also the cheapest way to learn whether it parses a
language you care about.

```console
$ fr parse --stats
LANGUAGE       FILES   ERRORS   UNREAD
bash              20        0        0
go               539        0        0
helm             669       13        0
markdown          95        0        0
yaml              83        0        0
```

`ERRORS` counts the files that did not parse cleanly. Those thirteen Helm files are
chart fixtures that hold broken templates on purpose, and Helm tests its own error
messages with them. Read that column closely: **no analysis below sees a file that
failed to parse**. A report that passes over parse failures leaves you short and says
nothing about it. Every command that indexes the workspace names the files it could
not parse in full, on stderr and as `unparsed_files` in its JSON. `fr rename` also
warns you that references may be missing, and `fr duplicates` lists the files it
skipped.

## 2. Naming the thing you mean

Every command that acts on a symbol takes a **target**. Write a target in one of two
ways.

**By name**, when the name is unambiguous:

```console
$ fr refs writeToFile
```

**By position**, as `path:line:col`:

```console
$ fr refs pkg/action/action.go:725:6
```

Count lines and columns from 1. Land on the symbol's identifier, and not on the
`func` keyword or the opening brace. Pick any occurrence you like, the definition or
any use of it. The tool resolves whatever sits under the cursor and works from the
symbol it found. An editor hands you the same information when you right-click, so
this form suits editors.

The tool takes **the project your shell is standing in** as the workspace. It finds
that root by walking up for a `.git`, a `Cargo.toml`, a `go.mod` and the like. Ask
from `pkg/deep` and it still reads the whole project, because a caller two
directories up is still a caller. It announces any widening of the root, and `-C .`
confines it to this directory alone.

Give `-C` a workspace and the tool reads relative paths **relative to that root**.
`fr -C ../helm refs pkg/x.go:3:6` names that file in that workspace, not one
relative to your shell. Leave `-C` off and it reads a path from where you stand.

The tool skips every file that `.gitignore` excludes. Pass `--no-ignore` to read
them, which is how you reach a generated tree or a vendored copy.

When a bare name is not enough, the tool refuses and shows you the choices rather
than guessing:

```console
$ fr refs RunAll
Error: 'RunAll' is defined 2 times; name one of these, or give a position as path:line:col
  RunAll (function) in internal/chart/v3/lint/lint.go
  RunAll (function) in pkg/chart/v2/lint/lint.go
```

This is common in helm, which carries a v2 and a v3 of several packages side by side.
Picking one for you would be a coin flip that reads like an answer.

A few commands take other target shapes, because what they act on is not a symbol:

| Form | Used by | Example |
|---|---|---|
| `path:line:col` | most commands | `fr def pkg/action/action.go:725:6` |
| a bare name | most commands | `fr callers RunAll` |
| `path:line:col-line:col` | `fr extract` | `fr extract a.go:10:2-12:20 helper` |
| `path` | `fr imports` | `fr imports pkg/action/install.go` |
| `path` (destination) | `fr move` | `fr move helper pkg/util/helper.go` |

## 3. Reading the code before changing it

Once you have a target, four commands answer the questions you would otherwise
answer by grepping.

```console
$ fr refs pkg/action/action.go:725:6
5 reference(s) to determineReleaseSSApplyMethod
  pkg/action/action_test.go:2280:54  [exact]
  pkg/action/action_test.go:2281:54  [exact]
  pkg/action/install.go:676:23  [exact]
  pkg/action/rollback.go:192:23  [exact]
  pkg/action/upgrade.go:334:23  [exact]
```

The tag on each line carries the **confidence**, and nothing else in the output
matters more. `exact` means the tool proved this reference resolves to that symbol,
because the scope chain, the import or the package says so. Three weaker tiers
follow, in descending order of evidence: `import-qualified`, `field-based` and
`name-only`. A refactoring rewrites `exact` and `import-qualified`, and reports
everything weaker for you to look at. Section 8 says what earns each tier.

`fr callers` walks the same edges upward:

```console
$ fr callers pkg/action/action.go:725:6 --depth 2
determineReleaseSSApplyMethod
  TestDetermineReleaseSSAApplyMethod
  Install::createRelease
  Rollback::prepareRollback
  Upgrade::prepareUpgrade
    Install::RunWithContext
    Rollback::Run
    Upgrade::RunWithContext
```

`fr callees` walks it downward, and `fr graph --dot` prints the whole thing.
`fr impact` answers "what could a change here touch" across all of it.

`fr entrypoints` finds the roots, the places where execution starts. Reachability
rests on them:

```console
$ fr entrypoints
cli-main           main                             cmd/helm/helm.go
cli-main           main                             internal/plugin/testdata/src/extismv1-test/main.go
cli-main           init                             pkg/cmd/helpers_test.go
cli-main           init                             pkg/cmd/profiling.go
cli-main           init                             pkg/kube/client.go
```

## 4. Finding work worth doing

### Code nothing uses

```console
$ fr unused --lang go --internal
```

Two flags matter here.

`--lang go` narrows the *report*, and leaves the index alone. Hold on to that
distinction. Scan only `pkg/` with `-C pkg` and the index loses sight of the callers
in `cmd/`, so it reports everything they call as dead. A filter here never invents a
finding.

`--internal` hides exported symbols. Helm ships as a library, and a library's public
API has no caller inside its own repository, which proves nothing about it. Run
without the flag and you get them, tagged:

```
method       Chart::SetDependencies             exported  internal/chart/v3/chart.go:73:18

199 of these are exported. In a library that is the public
API, which nothing in this repository can be expected to call. Pass
--internal to list only what is definitely dead here.
```

On helm the internal report lists 47 findings: 39 unused parameters and 8 unused
struct fields, and no functions, methods or variables. Eight bug fixes went into
being able to say that. Helm has very little dead code. Before those fixes the same
command reported 238 candidates, nearly all of them live code the tool could not
see.

The report also tells you what it *declined* to list and why. It leaves off a symbol
whose name appears in any string literal, because reflection and handler tables
reach code that no call edge shows. It leaves off anything beginning with `_`, and
anything reached only through an interface whose receiver it cannot prove.

### Code written twice

```console
$ fr duplicates --lang go --path pkg/cmd --path pkg/action --min-tokens 100
5 copies, 107 tokens each (428 redundant): go
  pkg/cmd/show.go:79-98
  pkg/cmd/show.go:100-119
  pkg/cmd/show.go:121-140
  pkg/cmd/show.go:142-161
  pkg/cmd/show.go:163-182
3 copies, 180 tokens each (360 redundant): go
  pkg/cmd/get_hooks_test.go:1-51
  pkg/cmd/get_manifest_test.go:1-51
  pkg/cmd/get_notes_test.go:1-51
```

Those five blocks in `show.go` are five cobra subcommands built the same way, one
after another. The tool compares **structure**: it hashes a subtree from the node
kinds under it, so a copy whose variables were renamed still matches. Those renamed
copies are the ones worth finding, because a textual search never turns them up. Add
`--exact` to fold the identifiers and literals back in and ask the stricter question.

The report lists only the largest duplicated block of each finding. A duplicated
function duplicates its body, its loop and each of its statements. Printing all of
them says one finding five times and buries the useful one.

Across all of helm the tool found **337 duplicated blocks, 64,530 redundant tokens**,
in 3.6 seconds. The largest pairs `internal/release/v2/info_test.go` with
`pkg/release/v1/info_test.go`: 377 lines that differ only in the package clause and
one blank line.

## 5. Making the change

`determineReleaseSSApplyMethod` names itself poorly: `SS` stands for "server-side",
and nothing in the identifier says so. It stays unexported, four files in one package
use it, and a test covers it, so it makes a good first refactor.

Every mutating command prints a diff and changes nothing until you pass `--write`.

```console
$ fr rename pkg/action/action.go:725:6 releaseApplyMethod
--- a/pkg/action/action.go
+++ b/pkg/action/action.go
@@ -722,7 +722,7 @@
 	cfg.HookOutputFunc = hookOutputFunc
 }

-func determineReleaseSSApplyMethod(serverSideApply bool) release.ApplyMethod {
+func releaseApplyMethod(serverSideApply bool) release.ApplyMethod {
 	if serverSideApply {
 		return release.ApplyMethodServerSideApply
 	}
--- a/pkg/action/action_test.go
+++ b/pkg/action/action_test.go
@@ -2277,8 +2277,8 @@
 func TestDetermineReleaseSSAApplyMethod(t *testing.T) {
-	assert.Equal(t, release.ApplyMethodClientSideApply, determineReleaseSSApplyMethod(false))
-	assert.Equal(t, release.ApplyMethodServerSideApply, determineReleaseSSApplyMethod(true))
+	assert.Equal(t, release.ApplyMethodClientSideApply, releaseApplyMethod(false))
+	assert.Equal(t, release.ApplyMethodServerSideApply, releaseApplyMethod(true))
 }
```

…and the same for `install.go`, `rollback.go` and `upgrade.go`. Look at what the
tool left alone: `TestDetermineReleaseSSAApplyMethod` keeps its name. That different
identifier contains the old one, and renaming by text would have caught
it.

The run also ends with a section worth reading every time:

```
Not changed. Review these yourself:
  incomplete-facts (13):
    internal/chart/v3/lint/rules/testdata/malformed-template/templates/bad.yaml:1:1  file has syntax errors; references in it may be missing
    …
```

Those lines name the thirteen deliberately broken chart fixtures from section 1. If
any of them mentioned this function, the tool would not know. Here they hold YAML and
cannot mention it, and the tool still declines to assume that on your behalf.

Apply it:

```console
$ fr rename pkg/action/action.go:725:6 releaseApplyMethod --write
Applied to 5 file(s).
```

Then check with the language's own compiler, the only verification that counts:

```console
$ go build ./...
$ go test ./pkg/action/ -run 'TestDetermineReleaseSSAApplyMethod|TestIsDryRun'
ok  	helm.sh/helm/v4/pkg/action	0.543s
```

### The other mutating commands

All of them follow the same shape: a diff by default, `--write` to apply, a refusal
with a reason when they cannot do it safely:

```console
$ fr extract pkg/action/install.go:221:5-221:20 itemCount     # expression → binding
--- a/pkg/action/install.go
+++ b/pkg/action/install.go
@@ -219,7 +219,8 @@
 	}
-	if len(totalItems) > 0 {
+	itemCount := len(totalItems)
+	if itemCount > 0 {

$ fr extract <range> prepare --function   # statements → a function, with parameters
$ fr inline <target>                      # the reverse of the binding form
$ fr signature releaseApplyMethod move:0:1  # reorder parameters, fix every call
$ fr move helper pkg/util/helper.go       # move, carrying and fixing imports
$ fr delete oldHelper                     # refuses if anything uses it
$ fr imports pkg/action/install.go        # drop unused imports, sort the rest
```

Run `fr rewrite` with no transformation named. It lists the ones that apply at that
position, and an editor builds its code-action menu from that list:

```console
$ fr rewrite pkg/action/install.go:224:3
invert-if      swap the branches and negate the condition

$ fr rewrite pkg/action/install.go:224:3 invert-if
```

The menu offers a transformation only when its result reparses, so it never lists
something the tool would then refuse to apply.

`fr delete` refuses more often than any other command, and usefully so:

```console
$ fr delete pkg/action/action.go:725:6
Error: refusing to delete 'releaseApplyMethod': 4 reference(s) still resolve to it
  pkg/action/action_test.go:2280:54
  pkg/action/action_test.go:2281:54
  pkg/action/install.go:676:23
  pkg/action/rollback.go:192:23
Remove or repoint these uses first; nothing was changed.
```

## 6. Configuration is code too

Here the tool leaves a language server behind. helm ships charts, and a chart pairs a
values file with the templates that read it. The Go compiler never sees that path.

Ask where a rendered value comes from:

```console
$ fr flow back pkg/cmd/testdata/testcharts/alpine/templates/alpine-pod.yaml:4:38
declaration Name: my-alpine  (pkg/cmd/testdata/testcharts/alpine/values.yaml:1)

Stopped at:
- 'values key Name' can still be overridden externally, from `-f` files and `--set` on the helm command line
- origin: literal value my-alpine
```

Or where a declared value goes:

```console
$ fr flow fwd pkg/cmd/testdata/testcharts/alpine/values.yaml:1:1
declaration Name: my-alpine  (pkg/cmd/testdata/testcharts/alpine/values.yaml:1)
  template-action {{.Values.Name}}  (…/templates/alpine-pod.yaml:4) [name-only]
  template-action {{.Values.Name}}  (…/templates/alpine-pod.yaml:17) [name-only]
```

The **Stopped at** section records what the tool could not resolve. `-f` and `--set`
can override a values key at any time. The tool cannot know your `helm install`
command, so tell it, and the answer sharpens:

```console
$ fr flow back <target> -f values-prod.yaml --set image.tag=v2
```

Helm's own precedence then decides the winner: chart `values.yaml`, then each
enclosing parent chart, then each `-f` in the order given, then `--set`. The report
still lists every loser, including a values file you say is *not* passed.

Rename a values key and the tool rewrites the templates that read it:

```console
$ fr rename pkg/cmd/testdata/testcharts/alpine/values.yaml:1:1 appName
-  name: "{{.Release.Name}}-{{.Values.Name}}"
+  name: "{{.Release.Name}}-{{.Values.appName}}"
-    values: {{.Values.Name}}
+    values: {{.Values.appName}}

Not changed. Review these yourself:
  textual-occurrence (1):
    …/alpine-pod.yaml:3:17  'Name' appears in a string or comment; left unchanged
```

The tool leaves `{{.Release.Name}}` and `{{.Chart.Name}}` alone. They spell the same
word and mean a different thing.

## 7. Opening the pull request

This step holds nothing special. You commit ordinary git work, reviewable line by
line, with no tool-specific artifacts in it:

```console
$ git checkout -b rename-release-apply-method
$ git diff --stat
 pkg/action/action.go      | 2 +-
 pkg/action/action_test.go | 4 ++--
 pkg/action/install.go     | 2 +-
 pkg/action/rollback.go    | 2 +-
 pkg/action/upgrade.go     | 2 +-

$ go build ./... && go test ./pkg/action/
$ git commit -am "Rename determineReleaseSSApplyMethod to releaseApplyMethod"
$ gh pr create --fill
```

Two habits worth keeping:

- **Commit the refactor alone.** A reviewer checks a mechanical change across five
  files quickly, and struggles once you mix a behavioural change into it.
- **Paste the refusals into the PR description.** When the tool lists six unparseable
  files or four weakly-resolved sites, a reviewer should check them. Nobody knows to
  look unless you say so.

---

## 8. How it works

### Parsing, and where the trees live

The tool parses every file with [tree-sitter](https://tree-sitter.github.io/), one
grammar per language, into a syntax tree that keeps every byte, including comments
and whitespace. An edit splices one byte range, so everything outside that range
survives by construction.

**The tool stores no tree.** Each one lives for the length of one file's extraction,
and then the tool drops it. A much smaller set of *facts* survives: symbols,
references, scopes and imports, each carrying a byte span. Keeping 16,000 syntax
trees in memory would cost more than re-parsing the handful of files any single
command needs.

The tool works in byte offsets everywhere except the command line, never in
line/column pairs. Line and column serve display; an emoji in a comment of a UTF-8
file makes them ambiguous. A refactoring tool that miscounts a column corrupts a
file.

Helm takes one extra step. Before parsing, the tool *masks* each template action and
puts filler of the same byte length in its place. The surrounding YAML then holds
valid structure, and every offset in the file stays correct. Where the action sits
decides whether the filler is spaces or `x` characters. An action alone on its line
has to vanish structurally, while one inside a value has to become scalar text. The
tool then parses the actions separately, so `.Values.image.tag` becomes a reference
to a values key.

### The index, and confidence

Extraction produces facts per file. The index joins them and resolves each reference
to the symbol it names, trying these rules in order:

- The lexical scope chain.
- The same file.
- An import binding in that file.
- A string key (CSS classes, Helm values).
- A member of a value.
- The enclosing package or directory.
- Finally, a unique exported name anywhere.

The first rule that answers wins, and the report names *which rule answered*. That
name is the confidence:

| Tier | What proved it | Rewritten? |
|---|---|---|
| `exact` | the scope chain, the file, or the package | yes |
| `import-qualified` | an import binding names the file it came from | yes |
| `field-based` | the name is a member somewhere, but the receiver's type is unknown | no |
| `name-only` | nothing but the name matched | no |

A refactoring rewrites the top two tiers and reports the rest. The whole tool rests
on that one design decision. **It hands you a list to check rather than change a line
it cannot justify.**

### The cache

Indexing helm takes a few seconds, and repeating that for every command would not do.

The cache addresses content. It keys a file's facts by the SHA-256 of the contents,
combined with a fingerprint of the query set that produced them. Change the file and
the key changes; change a `queries/*/facts.scm` and every key changes. No
invalidation logic can go wrong here, because nothing needs invalidating. Nothing
ever looks up a stale entry.

Entries live under `$FUN_REFACTOR_CACHE`, or the platform cache directory
(`~/Library/Caches/fun-refactor` on macOS, `~/.cache/fun-refactor` on Linux). The
directory carries a schema version. A release that changes what a fact *is* then
starts a new namespace, instead of reading old data with a new meaning.

```console
$ fr cache             # where it is and how big
location  ~/Library/Caches/fun-refactor/v2-dbe4d237fed430ca
size      3791 KiB

$ fr cache --clear     # throw it away
$ fr <command> --no-cache
```

The directory name carries both the schema version and the query-set fingerprint.
Edit a query file and every stale entry becomes unreachable rather than wrong.

The tool indexes files in parallel and merges results in scan order, so the output
stays the same whichever thread finishes first.

### The edit engine

Every refactoring returns a *plan*, a set of edits, and touches nothing itself. The
engine then:

1. **Rejects overlaps.** Two edits to the same bytes are a bug in the caller, not
   something to resolve by picking one.
2. **Applies in descending offset order**, so earlier offsets stay valid as it goes.
3. **Reparses every changed file** and refuses the whole edit if a file that parses
   cleanly now would not afterwards. That check catches a rewrite producing
   `if !(a)` in a language that requires the brackets.
4. **Commits atomically.** It writes every file, or it writes none.

That reparse check works as a safety net, and it does not stand in for the analysis.
It misses a change that parses and means something else. A statement moves out from
under the condition that guarded it, say, or a de Morgan result loses the brackets it
needs. Correct analysis catches those. The confidence tiers exist for that reason,
and the tool refuses so much.

---

## Where to go next

- `fr capabilities` prints what each of the 17 languages supports, with the reason
  attached to every cell that is not. The code derives it, and nobody maintains it by
  hand.
- `fr <command> --help` for the flags each command takes.
- `--json` on any command, for editor integration and scripting.
- [BUGS.md](BUGS.md) for what is known not to work, each entry with the measurement
  that established it.
