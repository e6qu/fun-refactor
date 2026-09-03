# Refactoring recipes

A **recipe** is a refactoring written down: a file that says what to find, what to do
to it, and what must be true afterwards. A reviewer can read it, you can run it again,
and it fails loudly.

**Built.** Run `fr recipe <file>` to plan or apply one workspace transaction. A file
may hold several recipes: each sees the virtual workspace its predecessor left, and
`--write` commits only when every recipe succeeds. `src/recipe/` implements the runner
and `tests/recipe.rs` covers it. The tutorial at
[docs/recipes.html](https://e6qu.github.io/fun-refactor/recipes.html) works five of
them, in five languages, with the output the tool produced.

The design and the build agree. Every predicate in the table below works,
including the four that were missing at first. One call graph answers both `calls=` and
`called-by=`, and the runner builds it only when a recipe asks for one of them. The
hierarchy answers `implements=`. The pattern matcher answers `matches=`, and it needs
`lang=` beside it: the same text parses into a different tree in every language.

The runner found a defect the design could not have: the refactorings read source
through `crate::vfs`. So a later step read the file on *disk*, the text as it stood
before any step ran. The in-memory backing that the browser build uses now compiles
everywhere, and the runner installs the workspace on it. The design intended "each step
sees what the last one left". The runner now delivers it.

```
schema 1

recipe retire-legacy-auth {
  description "The legacy auth path has been dark for a year."

  requires symbol "USE_LEGACY_AUTH"

  remove-flag "USE_LEGACY_AUTH" = false

  delete where kind=function
               name~"legacy_auth_*"
               !exported
        on-refusal report

  imports where changed

  expect no-new unused
}
```

## Why

Every command in this tool acts on **one** target. A person at a terminal wants that.
The work people bring does not fit it:

- *"Retire `USE_LEGACY_AUTH`, delete what it was guarding, and tidy the imports that
  leaves behind."*, three commands, in order, each depending on the last.
- *"Turn every wrapping `if` in `pkg/services` into a guard clause."* That names 1,498
  sites in helm/helm alone. Nobody types that.

Today you write a shell loop over `fr`. The refusals scroll past, the ordering stays
implicit, and nowhere does a reviewer find what you did written down. A recipe makes
the *plan* the artifact and the diff its product.

Several recipes in one file are one larger transaction, not a shell loop with nicer
syntax. They run in file order against one virtual workspace. The preview and JSON
report describe the complete file, and a failed later recipe keeps an earlier recipe's
successful result out of the working tree.

## Non-goals

**Not a programming language.** No loops, no arithmetic, no user-defined functions,
no conditionals. The moment a recipe needs those it should be a program calling the
CLI, and we should make that pleasant instead. Every construct below appears in a
refactoring someone wants.

**Does not extend what the tool can do.** A recipe composes existing operations. If you
could not type a step as an `fr` command, it is not a step.

**Not a linter.** A recipe changes code. The default run reports without changing:
nothing reaches disk without `--write`, so every run doubles as a check.

## The cost of a bespoke syntax, and how it gets paid

This repository now holds a third mini-language, after the entry-point catalogs and
the `$META` patterns in `restructure`. The cost is real and it buys terseness. Three
things have to come with the language to justify it, and here is where each stands:

1. **Errors that name the mistake and where it is**, with a suggestion from the
   closed vocabulary. Built: type a predicate wrong and the parser answers `there is
   no predicate called 'exportd'. Did you mean 'exported'?`. A language without that
   answer is a chore.
2. **`fr recipe <file> --explain`**, print what a recipe would do, without running
   it. Built: it parses the file and prints the steps, the selectors and the
   expectations, selecting and running nothing. A terse language repays reading only
   when you can ask it what it means.
3. **One canonical layout.** Not built. The parser discards layout and no printer
   round-trips a file, so `fr recipe fmt` would need new machinery instead of a flag
   on what exists. Until someone writes it, the author owns the layout, and a diff of
   a recipe still shows a diff of its meaning.

Weaken either of the first two and the YAML we did not write becomes the better
choice.

## Lexical structure

| | |
| --- | --- |
| Comments | `#` to end of line |
| Identifiers | `[a-z][a-z0-9-]*`, kebab-case, matching the CLI (`remove-flag`, `on-refusal`) |
| Strings | `"…"` with `\"` and `\\`; or `'…'` raw, with no escapes at all |
| Numbers | non-negative integers |
| Booleans | `true`, `false` |
| Layout | insignificant: newlines and indentation are whitespace |

Patterns *are code*, and code is full of quotes, so the language offers two string
forms: `'"%s" % ($X,)'` needs no escaping and stays legible. A raw string cannot
contain its own delimiter; write it in the other form.

Nothing terminates a statement. The parser ends one when it meets a token that can only
begin the next. **Step keywords are reserved and no predicate shares a name with one**,
an invariant a test enforces. So a `where` clause runs across as many lines as it needs
with no punctuation:

```
delete where kind=function
             name~"legacy_auth_*"
             !exported
      on-refusal report
```

## Grammar

```ebnf
file        = "schema" INT , { recipe } ;
recipe      = "recipe" , IDENT , "{" , { directive } , "}" ;
directive   = description | requires | step | expect ;

description = "description" , STRING ;
requires    = "requires" , ( "language" , IDENT
                           | ( "symbol"   , STRING
                             | "any" , "symbol" , STRING , STRING , { STRING } ) , [ selector ]
                           | "path"     , STRING ) ;

step        = operation , [ selector ] , { modifier } ;

operation   = "rename"      , "to" , STRING
            | "delete"
            | "move"        , "to" , STRING
            | "imports"
            | "inline"      , ( "variable" | "call" )
            | "extract"     , ( "variable" | "function" ) , "at" , STRING , "as" , STRING
            | "signature"   , STRING
            | "remove-flag" , STRING , "=" , BOOL
            | "restructure" , IDENT , STRING , "=>" , STRING
            | "rewrite"     , IDENT
            | "translate"   , "to" , IDENT ;

selector    = "where" , predicate , { predicate } ;
predicate   = IDENT , "=" , value        (* kind=function      *)
            | IDENT , "~" , STRING       (* name~"legacy_*"    *)
            | IDENT                      (* unused             *)
            | "!" , IDENT ;              (* !exported          *)
value       = STRING | IDENT | INT | BOOL ;

modifier    = "on-refusal" , ( "stop" | "report" | "allow" )
            | "limit" , INT
            | "allow-empty" ;

expect      = "expect" , ( "no-new" , IDENT
                         | "changed" , comparison , INT , [ "files" ]
                         | "refusals" , comparison , INT ) ;
comparison  = "=" | ">" | "<" | ">=" | "<=" ;
```

`requires any symbol "before" "after"` accepts either settled name of a
migration, but rejects a tree that has neither.

A symbol guard may add `where kind=function in="src/cli.rs"`. It accepts a
name only when the same selector identifies the intended declaration.

### The grammar is not the whole story

I wrote a throwaway lexer and parser for the above and ran the examples through it.
Every example parses, every step form parses, and a mistyped predicate produces
`unknown predicate 'exportd', did you mean 'exported'?`. Three inputs parsed happily
that should not have:

| Input | What happened |
| --- | --- |
| `rewrite where lang=go` | The transformation name is missing. Parsed with no arguments. |
| `rename where name="a"` | No `to`. Parsed as a rename to nothing. |
| `remove-flag "F" = false where unused` | A selector on an operation that has no use for one. Accepted and ignored. |

All three are the same mistake: **the grammar is permissive where the operations are
not.** A production that says `operation , [selector] , {modifier}` cannot express
that `rewrite` needs a name, that `rename` needs a target, or that `remove-flag` acts
on the whole workspace and a `where` clause is meaningless to it. The third is the
worst: it accepts a selector and drops it without a word, the accept-and-ignore this
codebase bans elsewhere.

Fix all three with a **signature table**, checked immediately after the parse, rather
than with a bigger grammar:

| Operation | Positional | Selector | Notes |
| --- | --- | --- | --- |
| `rename` | `to STRING` | required | |
| `delete` | — | required | |
| `move` | `to STRING` | required | |
| `imports` | — | required | |
| `inline` | `variable`\|`call` | required | |
| `extract` | `variable`\|`function` `at` STRING `as` STRING | **rejected** | a range is not selectable |
| `signature` | STRING | required | |
| `remove-flag` | STRING `=` BOOL | **rejected** | acts on the whole workspace |
| `restructure` | IDENT STRING `=>` STRING | **rejected** | the pattern *is* the selector |
| `rewrite` | IDENT | required | |
| `translate` | `to IDENT` | required | writes a new file beside each source |

"Rejected" means an error naming the operation and why, never a silent ignore.

`translate` is the one operation that writes a file the workspace did not have.
It writes beside the source and never over it, so a destination that is already
there is a refusal. The step reports what it created apart from what it changed.
A construct the target has no counterpart for is a warning against the line of
the source it came from. That is the shape a rename uses for a use it left.
The parser checks the language as it reads the recipe. A target nothing becomes is a mistake in the recipe, not a fault of the file it reaches.

Two more things the prototype argued for:

- **A value may not be a bare reserved word.** The prototype read `where name=`, a
  newline, and `imports`, and took `imports` as the value. It then failed confusingly
  two tokens later. Refusing keywords in value position turns that into
  `name= needs a value; found the step keyword 'imports'`.
- **`where` and modifiers should be order-independent.** The prototype rejected
  `delete on-refusal allow where unused`, an unmemorable rule that buys nothing.

`schema 1` is the first statement in the file and is mandatory. It makes the
staged answer to sharing possible: a reader refuses a file it does not understand
before parsing a single step. See *Sharing*, below.

## Selection

The heart of it. Everything else is the existing CLI.

The predicates come from the **entry-point catalog matcher** in
`src/analysis/entrypoints.rs`, which already carries rules for thirteen languages:

| Predicate | Matches |
| --- | --- |
| `name="x"` | exactly |
| `name~"pre_*"` | by glob |
| `kind=function` | `function`, `method`, `class`, `selector`, `key`, … |
| `exported` / `!exported` | is or is not |
| `annotated-with="test"` | `#[test]`, `@property`, a build tag |
| `file~"*_test.go"` | by path glob |
| `lang=python` | one of the eighteen |

Reuse it and a recipe's selector means what an entry-point rule means, by
construction. The matcher gains from a second caller.

A recipe adds predicates that only make sense against a whole workspace. Each one is an
existing analysis:

| Predicate | Meaning | Comes from |
| --- | --- | --- |
| `in="src/adapters/"` | under a directory | the scanner |
| `unused` | nothing reaches it | `fr unused` |
| `duplicated` | part of a copy-paste class | `fr duplicates` |
| `calls="x"` / `called-by="x"` | edges in the call graph | `fr callers` / `callees` |
| `implements="Sink"` | a concrete answer to an abstraction | `fr implementations` |
| `matches='$A + $B'` | a structural shape | `fr restructure` |
| `changed` | this recipe already touched it | the run itself |

### Asking what a recipe may say

`fr recipe --vocabulary` prints the whole surface: the requirement forms, verbs
and their arguments, both predicate lists, rewrites, modifiers and languages. It
comes from the code that reads a recipe, so it says what this build takes rather
than what a document remembers. `--json` gives the same to a program.

### A file step takes the file predicates

`imports`, `rewrite` and `translate` act on a file. The rest act on a symbol. So a
selector for one of the three may only ask what a file can answer: `lang`, `file`,
`in`, `duplicated`, `changed`, `matches`.

Asking `kind=function` of a `rewrite` names a symbol, and a file cannot answer it.
The step refuses and says which predicate to drop. It used to match no file and
report that the rewrite found nothing, which sent a reader after the wrong thing.

So *"every unused unexported helper under `src/adapters`"* is:

```
delete where unused !exported kind=function in="src/adapters/"
```

**A selector that matches nothing stops the recipe.** Doing nothing without a word
looks like success, and this design fears that failure most. Write `allow-empty` when
a step is genuinely conditional.

## Steps

Each reads like the command it composes.

```
rename to "parse_uri"      where name="parse_url" kind=function
delete                     where unused !exported in="src/adapters/"
move to "src/convert.rs"   where name="hottest"
imports                    where changed
inline variable            where name="limits"
inline call                where name="sample"
signature "add:1:timeout: int:30"  where name="fetch"
remove-flag "USE_LEGACY_AUTH" = false
restructure python '"%s" % ($X,)' => 'f"{$X}"'
rewrite guard-clause       where lang=go in="pkg/services/"
extract function at "report.py:24:5-31:20" as "accumulate"
```

The result sits next to the verb, and the selector, which can run long, trails. Two
steps need comment.

**`rewrite`** has no target in the usual sense. It applies at a position. The
selector chooses *files*, and the step transforms every position in them that fits.
This is the most dangerous statement in the language: `guard-clause` was once wrong at
1,258 of 1,498 sites in helm/helm. It needs `limit`, the dry run, and an `expect` more
than any other step.

**`extract`** takes a range, and a selector cannot name a range. It is a judgement
about one specific piece of code, so it stays positional. A recipe containing one
describes a file rather than a policy. The limit is real, and we say so.

**`rename` takes a literal.** v1 computes no names: no captures, no case conversions.
You cannot write a convention-wide rename, `handle_*` to `on_*`, and we chose that.
Small expression languages grow, and nobody has asked for this one yet.

## Refusals

The tool refuses instead of guessing. A recipe run at scale collects refusals, and how
it treats them is the decision that matters most here.

| `on-refusal` | Meaning |
| --- | --- |
| `stop` (default) | abandon the run and write nothing |
| `report` | record it, apply the rest, exit non-zero |
| `allow` | record it, apply the rest, exit zero, the refusals were expected |

The language deliberately offers no `ignore`. Every refusal reaches the report; the
only question is its effect on the exit code. A person types `allow` after deciding
these refusals are acceptable, which leaves the permission visible and attributable.

## Transactions

A recipe is **one transaction**. The runner writes every step's edits or none of them.
A half-applied recipe leaves a repository in a state nobody designed: the flag removed,
its dead branches still there.

Each step sees the workspace **as the previous step left it**, so the runner re-indexes
between steps. `Index::build_from_sources` already does this for the cascade
machinery: it re-resolves against in-memory results instead of writing to disk to read
the text back.

Dry-run is the default, as everywhere else in this tool. `--write` applies.

## Expectations

```
expect changed > 0 files
expect no-new unused
expect no-new duplicates
expect refusals = 0
```

`no-new` is the interesting one: it re-runs the analysis afterwards and compares. A
refactoring that removes a call and orphans three functions has not finished, and
`no-new` says so.

The engine reparse-checks every edit regardless. You do not opt into that one.

## Output

Every run produces a report, human by default and `--json` for a machine.
For each step it prints what it selected and by which predicate, what changed, what
refused and why, and what `expect` found.

```
recipe retire-legacy-auth: 3 step(s)

  1  remove-flag "USE_LEGACY_AUTH" = false
     14 files changed, 212 lines removed

  2  delete where kind=function name~"legacy_auth_*" !exported
     matched 9, deleted 7, refused 2
       legacy_auth_token   src/compat.py:88   1 reference still resolves to it
       legacy_auth_header  src/compat.py:96   1 reference still resolves to it

  3  imports where changed
     11 files changed

expect
  ✓ changed > 0 files     31 files
  ✓ no-new unused         0 new
  ✗ refusals = 0          2

Nothing written. Re-run with --write to apply.
```

## Worked examples

A policy, run everywhere, with no positions and no file names, matching on shape alone:

```
schema 1

recipe no-legacy-string-formatting {
  restructure python '"%s" % ($X,)' => 'f"{$X}"'
  expect no-new unused
}
```

A migration with an order, each step depending on the last:

```
schema 1

recipe rename-parse-url {
  requires symbol "parse_url"

  rename to "parse_uri" where name="parse_url" kind=function
  imports where changed

  expect refusals = 0
}
```

A clean-up that expects refusals, because some of this is public API:

```
schema 1

recipe drop-dead-adapters {
  delete where unused !exported in="src/adapters/"
         on-refusal allow

  expect changed > 0 files
}
```

## Sharing, staged, and honest about it

v1 recipes are **local**: keep the file beside the code it changes and run
`fr recipe recipes/retire-legacy-auth.recipe --write`. No registry, no fetching, no
running someone else's file against your source.

Every file carries `schema 1` from day one anyway, because it costs one line now and
nobody can add later. A future reader grabs that hook to refuse a file it does not
understand.

What sharing would require, written down rather than answered badly:

- **Compatibility.** What does `schema 2` mean for a `schema 1` recipe? Must the reader
  run it, refuse it, or upgrade it?
- **Blast radius.** A shared recipe edits your source. Does it declare the paths it
  may touch, and is that declaration enforced or advisory?
- **Provenance.** Who wrote it, what does it hash to, and does the run record that in
  the commit it produces? The repository already insists on provenance for vendored
  corpora; a recipe that rewrites your code deserves at least as much.
- **Review.** A diff of a recipe is small and its effect is large. The asymmetry
  carries the whole risk, and a version field does not solve it.

None of these are answered here. They are the reason v1 does not fetch.

## What I am least sure about

1. **`rewrite` at scale.** Selecting files and applying at every applicable position
   is the most useful and most dangerous step. `limit N` is a partial answer. A
   `sample N` that applies to ten sites, so a person can *read* them, may be better.
   I cannot tell without watching someone use it.

2. **Statement termination by reserved word.** It gives the clean multi-line `where`
   with no punctuation. It survived the adversarial inputs above, and the parser
   catches a mistyped *predicate* precisely. A mistyped *step name* is the remaining
   ambiguity. The parser can only answer `delte where unused` with "not a step or
   directive". At that point it cannot tell a bad step from a bad predicate. A
   closed vocabulary makes "did you mean `delete`?" easy, which is probably enough.

3. **Whether `expect` belongs in the language at all.** It could be a CI concern:
   run the recipe, then run `fr unused` and compare. Keeping it in the file makes the
   recipe self-describing; keeping it out makes the language smaller. I lean towards
   in, because the recipe is meant to be the artifact a reviewer reads.
