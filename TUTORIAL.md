# Refactoring a real codebase with fun-refactor

Everything below was run against [helm/helm](https://github.com/helm/helm) at commit
[`a8ab76e`](https://github.com/helm/helm/commit/a8ab76e), a 539-file Go codebase that
also carries 669 Helm templates, 95 Markdown documents and 83 plain YAML files. The
outputs are copied from those runs. Where a number looks odd, the tool said it.

You will need the repository and the binary:

```console
$ git clone https://github.com/helm/helm.git
$ cd helm && git checkout a8ab76e

$ git clone https://github.com/e6qu/fun-refactor.git ../fun-refactor
$ cargo install --path ../fun-refactor    # or: cargo build --release
```

---

## 1. What is in here?

Start by asking what the tool can read. Nothing is indexed until you ask for
something. So this is also the cheapest way to find out whether a language you care
about is being parsed at all.

```console
$ fr parse --stats
LANGUAGE       FILES   ERRORS   UNREAD
bash              20        0        0
go               539        0        0
helm             669       13        0
markdown          95        0        0
yaml              83        0        0
```

`ERRORS` is the number of files that did not parse cleanly. Those thirteen Helm files
are chart fixtures that hold broken templates on purpose. Helm tests its own
error messages with them. This matters more than it looks: **a file that does not
parse is invisible to every analysis below**. So a report that ignores parse failures
is quietly incomplete. Every command that indexes the workspace names the files it
could not parse in full, on stderr and as `unparsed_files` in its JSON. `fr rename`
will also tell you that references may be missing. `fr duplicates` lists the files
it skipped.

## 2. Naming the thing you mean

Every command that acts on a symbol takes a **target**, and there are two ways to
write one.

**By name**, when the name is unambiguous:

```console
$ fr refs writeToFile
```

**By position**, as `path:line:col`:

```console
$ fr refs pkg/action/action.go:725:6
```

The position is 1-based in both line and column. It must land on the symbol's
identifier, and not the `func` keyword or the opening brace. Any occurrence works:
the definition or any use of it. The tool resolves whatever is under the
cursor and then works from the symbol it found. That makes the form
editor-friendly; it is the same information an editor has when you right-click.

The workspace is **the project your shell is standing in**, found by walking up
for a `.git`, a `Cargo.toml`, a `go.mod` and the like. Asked from `pkg/deep`, the
tool reads the whole project. A question about a symbol is a question about the
project: a caller two directories up is still a caller. Widening the root is
announced, and `-C .` means this directory alone.

Where `-C` says which workspace to use, relative paths are read **relative to
that root**. `fr -C ../helm refs pkg/x.go:3:6` means that file in that
workspace, not one relative to your shell. Where the root was found rather than
stated, a path is read from where you stand.

Files that `.gitignore` excludes are outside all of this. `--no-ignore` reads
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

The tag on each line is the **confidence**, and it is the most important thing in the
output. `exact` means the tool proved this reference resolves to that symbol, because the
scope chain, the import or the package says so. The other tiers are
`import-qualified`, `field-based` and `name-only`, in descending order of evidence.
Only `exact` and `import-qualified` are rewritten by a refactoring; everything weaker
is reported for you to look at. Section 8 explains what earns each tier.

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

`fr callees` walks it downward, `fr graph --dot` prints the whole thing, and
`fr impact` answers "what could a change here touch" across all of it.

`fr entrypoints` finds the roots: the places where execution starts. That makes
reachability mean anything:

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

`--lang go` narrows the *report*, not the index. That distinction is the whole
point: you could scan only `pkg/` with `-C pkg`, but then the index cannot see the
callers in `cmd/`. Everything they call would be reported as dead. Filters here
never invent a finding.

`--internal` hides exported symbols. Helm is a library, and the public API of one
has no caller inside its own repository, and that is not evidence of anything. Run
without the flag and you get them, tagged:

```
method       Chart::SetDependencies             exported  internal/chart/v3/chart.go:73:18

199 of these are exported. In a library that is the public
API, which nothing in this repository can be expected to call. Pass
--internal to list only what is definitely dead here.
```

On helm the internal report is 47 findings, and none of them are functions, methods
or variables: 39 unused parameters and 8 unused struct fields. That is a real
result, and it took eight bug fixes to be able to say it. Helm has very little dead
code. Before them the same command reported 238 candidates, nearly all of which
were live code the tool could not see.

The report also tells you what it *declined* to list and why. A symbol whose name
appears in any string literal is left off, because reflection and handler tables
reach code that no call edge shows. So is anything beginning with `_`, and anything
reached only through an interface the tool cannot prove the receiver of.

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
after another. The comparison is **structural**: a subtree is hashed from the node
kinds under it, so a copy whose variables were renamed still matches. That is the
copy worth finding, because a textual search will never turn it up. `--exact` folds
the identifiers and literals back in when you want the stricter question.

Only the largest duplicated block of each finding is listed. A duplicated function
duplicates its body, its loop and each of its statements. Printing all of them is one
finding said five times with the useful one buried.

Across all of helm: **337 duplicated blocks, 64,530 redundant tokens**, in 3.6
seconds. The largest is `internal/release/v2/info_test.go` against
`pkg/release/v1/info_test.go`: 377 lines whose only differences are the package
clause and one blank line.

## 5. Making the change

`determineReleaseSSApplyMethod` is a poor name: `SS` is "server-side", which nothing
about the identifier says. It is unexported, used from four files in one package, and
covered by a test, and a good first refactor.

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

…and the same for `install.go`, `rollback.go` and `upgrade.go`. Note what the tool
did **not** do: `TestDetermineReleaseSSAApplyMethod` keeps its name. It is a different
identifier that contains the old one, and renaming by text would have caught
it.

The run also ends with a section worth reading every time:

```
Not changed. Review these yourself:
  incomplete-facts (13):
    internal/chart/v3/lint/rules/testdata/malformed-template/templates/bad.yaml:1:1  file has syntax errors; references in it may be missing
    …
```

Those are the thirteen deliberately broken chart fixtures from section 1. If any of
them mentioned this function, the tool would not know. Here they are YAML and cannot,
but the tool does not assume that on your behalf.

Apply it:

```console
$ fr rename pkg/action/action.go:725:6 releaseApplyMethod --write
Applied to 5 file(s).
```

And check with the language's own compiler, which is the only verification that
counts:

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

`fr rewrite` with no transformation named lists the ones that apply at that position,
which an editor uses to build a code-action menu:

```console
$ fr rewrite pkg/action/install.go:224:3
invert-if      swap the branches and negate the condition

$ fr rewrite pkg/action/install.go:224:3 invert-if
```

The menu offers a transformation only when its result reparses, so the menu
never lists something that applying it would then refuse.

`fr delete` is the one that refuses most often, and usefully:

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

This is where the tool differs from a language server. helm ships charts, and a chart
is a values file and the templates that read it. The Go compiler never sees that path.

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

The **Stopped at** section records what it could not resolve. A values key can always be overridden
by `-f` and `--set`. The tool cannot know your `helm install` command, so
tell it, and the answer sharpens:

```console
$ fr flow back <target> -f values-prod.yaml --set image.tag=v2
```

Helm's own precedence then decides the winner: chart `values.yaml`, then each
enclosing parent chart, then each `-f` in the order given, then `--set`. Every loser
is still listed, including a values file you say is *not* passed.

Renaming a values key rewrites the templates that read it:

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

`{{.Release.Name}}` and `{{.Chart.Name}}` are left alone. They are the same word and
a different thing.

## 7. Opening the pull request

Nothing about this step is special. The change is ordinary git
work, reviewable line by line, with no tool-specific artifacts in it:

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

- **Commit the refactor alone.** A mechanical change that touches five files is
  trivial to review when it is only that. Impossible when it is mixed with a
  behavioural one.
- **Paste the refusals into the PR description.** If the tool listed six unparseable
  files or four weakly-resolved sites, a reviewer should check them,
  and they cannot know to look unless you say so.

---

## 8. How it works

### Parsing, and where the trees live

Every file is parsed with [tree-sitter](https://tree-sitter.github.io/), one grammar
per language, into a syntax tree that keeps every byte, including comments
and whitespace. Nothing is lost, because an edit is a byte-range
splice, so anything outside the range is untouched by construction.

**The trees are not stored.** They exist for the duration of one file's extraction and
are dropped. What survives is a much smaller set of *facts*: symbols, references,
scopes and imports, each carrying a byte span. Keeping 16,000 syntax trees in memory
would cost more than re-parsing the handful of files any single command needs.

Positions are byte offsets, never line/column pairs, everywhere except the command
line. Line and column are a display format; a UTF-8 file with an emoji in a comment
makes them ambiguous. A refactoring tool that miscounts a column corrupts a file.

Helm gets one extra step. The tool *masks* a template action before it parses, and
replaces it with filler of the same byte length. The surrounding YAML then has valid
structure and every offset in the file stays correct. Whether the filler is spaces or
`x` characters depends on where the action sits. An action alone on its line has to
vanish structurally, while one inside a value has to become scalar text. The actions
themselves are then parsed separately, so `.Values.image.tag` becomes a
reference to a values key.

### The index, and confidence

Extraction produces facts per file. The index joins them and resolves each reference
to the symbol it names, trying in order: the lexical scope chain, the same file, an
import binding in that file, a string key (CSS classes, Helm values), a member of a
value, the enclosing package or directory. Finally a unique exported name
anywhere. The first rule that answers wins, and the report names *which rule answered*, which is the
confidence:

| Tier | What proved it | Rewritten? |
|---|---|---|
| `exact` | the scope chain, the file, or the package | yes |
| `import-qualified` | an import binding names the file it came from | yes |
| `field-based` | the name is a member somewhere, but the receiver's type is unknown | no |
| `name-only` | nothing but the name matched | no |

A refactoring rewrites the top two and reports the rest. This is the single design
decision the whole tool rests on. **it would rather hand you a list to check than
change a line it cannot justify.**

### The cache

Indexing helm takes a few seconds; doing it again for every command would not.

The cache is content-addressed. A file's facts are keyed by the SHA-256 of its
contents combined with a fingerprint of the query set that produced them. Change the
file and the key changes; change a `queries/*/facts.scm` and every key changes. There
is no invalidation logic to get wrong, because there is nothing to invalidate. Nothing
ever looks up a stale entry.

Entries live under `$FUN_REFACTOR_CACHE`, or the platform cache directory
(`~/Library/Caches/fun-refactor` on macOS, `~/.cache/fun-refactor` on Linux). The
directory carries a schema version, so a release that changes what a fact *is* starts
a new namespace instead of reading old data with a new meaning.

```console
$ fr cache             # where it is and how big
location  ~/Library/Caches/fun-refactor/v2-dbe4d237fed430ca
size      3791 KiB

$ fr cache --clear     # throw it away
$ fr <command> --no-cache
```

The directory name carries both the schema version and the query-set fingerprint,
so editing a query file makes every stale entry unreachable and not
wrong.

Indexing is parallel across files, and results merge in scan order, so the output does
not depend on which thread finished first.

### The edit engine

Every refactoring returns a *plan*, which is a set of edits, and touches nothing. The engine
then:

1. **Rejects overlaps.** Two edits to the same bytes are a bug in the caller, not
   something to resolve by picking one.
2. **Applies in descending offset order**, so earlier offsets stay valid as it goes.
3. **Reparses every changed file** and refuses the whole edit if the file parses
   cleanly now and would not afterwards. This is what catches a rewrite that produces
   `if !(a)` in a language that requires the brackets.
4. **Commits atomically.** Either every file is written or none is.

That reparse check is a safety net and not the safety itself. It cannot catch a change that
parses and means something else, such as moving a statement out from under the condition
that guarded it, say, or dropping the brackets a de Morgan result needs. Those are
caught by the analysis being right. The confidence tiers exist for that reason, and
the tool refuses so much.

---

## Where to go next

- `fr capabilities` prints what is supported for each of the 16 languages, with the
  reason attached to every cell that is not. It is derived from the code, not
  maintained by hand.
- `fr <command> --help` for the flags each command takes.
- `--json` on any command, for editor integration and scripting.
- [BUGS.md](BUGS.md) for what is known not to work, each entry with the measurement
  that established it.
