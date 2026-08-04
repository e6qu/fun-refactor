# Refactoring recipes

A **recipe** is a refactoring written down: a file that says what to find, what to do
to it, and what must be true afterwards. It is reviewable, re-runnable, and it fails
loudly.

**Built.** `fr recipe <file>` runs one; `src/recipe/` is the implementation and
`tests/recipe.rs` covers it. The tutorial at
[docs/recipes.html](https://e6qu.github.io/fun-refactor/recipes.html) works five of
them, in five languages, with the output the tool actually produced.

What the design argued for and what got built agree. Every predicate in the table below
is implemented, including the four that were not at first: `calls=` and `called-by=`
come from one call graph, built only when one of them is asked for; `implements=` from
the hierarchy; and `matches=` from the pattern matcher, which needs `lang=` beside it
because the same text parses into a different tree in every language.

The runner found a defect the design could not have: the refactorings read source through `crate::vfs`, so a step planned
after another step was measured against the file on *disk* — the text before any step
ran. The in-memory backing that the browser build uses is now compiled everywhere and
the runner installs the workspace on it, which is what makes "each step sees what the
last one left" true rather than intended.

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

Every command in this tool acts on **one** target. That is right for a person at a
terminal and wrong for the work people actually have:

- *"Retire `USE_LEGACY_AUTH`, delete what it was guarding, and tidy the imports that
  leaves behind."* — three commands, in order, each depending on the last.
- *"Turn every wrapping `if` in `pkg/services` into a guard clause."* — 1,498 sites in
  helm/helm alone. Nobody types that.

Today each is a shell loop over `fr`, which means the refusals scroll past, the
ordering is implicit, and the thing you actually did is written down nowhere a
reviewer can read. A recipe makes the *plan* the artifact. The diff is what it
produces.

## Non-goals

**Not a programming language.** No loops, no arithmetic, no user-defined functions,
no conditionals. The moment a recipe needs those it should be a program calling the
CLI, and we should make that pleasant instead. Every construct below earns its place
by appearing in a refactoring someone actually wants.

**Does not extend what the tool can do.** A recipe composes existing operations. If a
step could not be typed as an `fr` command, it is not a step.

**Not a linter.** It changes code. Reporting without changing is a mode (`--check`),
not a purpose.

## The cost of a bespoke syntax, and how it gets paid

This is a third mini-language in one repository — after the entry-point catalogs and
the `$META` patterns in `restructure`. That is a real cost and it buys terseness. To
be worth it, three things have to ship *with* the parser rather than after it:

1. **Errors that name the mistake and where it is**, with a caret and a suggestion
   from the closed vocabulary. `unknown predicate 'exportd'` with `did you mean
   'exported'?` is the difference between a language and a chore.
2. **`fr recipe fmt`** — one canonical layout, so nobody argues about alignment and a
   diff of a recipe is a diff of its meaning.
3. **`fr recipe explain`** — print what a recipe would select and do, in prose,
   without running it. A terse language earns its terseness only if you can ask it
   what it means.

Anything less and the YAML we did not write would have been the better choice.

## Lexical structure

| | |
| --- | --- |
| Comments | `#` to end of line |
| Identifiers | `[a-z][a-z0-9-]*` — kebab-case, matching the CLI (`remove-flag`, `on-refusal`) |
| Strings | `"…"` with `\"` and `\\`; or `'…'` raw, with no escapes at all |
| Numbers | non-negative integers |
| Booleans | `true`, `false` |
| Layout | insignificant: newlines and indentation are whitespace |

Two string forms because patterns *are code*, and code is full of quotes:
`'"%s" % ($X,)'` needs no escaping and stays legible. Raw strings cannot contain
their own delimiter; use the other form.

Statements are **not** terminated. A statement ends when the parser meets a token that
can only begin a new one. That works because **step keywords are reserved and no
predicate shares a name with one** — an invariant a test enforces, not a hope. It is
what lets a `where` clause run across as many lines as it needs with no punctuation:

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
                           | "symbol"   , STRING
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
            | "rewrite"     , IDENT ;

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

### The grammar is not the whole story

I wrote a throwaway lexer and parser for the above and ran the examples through it.
Every example parses, every step form parses, and a mistyped predicate produces
`unknown predicate 'exportd' — did you mean 'exported'?`. Three inputs parsed happily
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
worst — silently accepting a selector and ignoring it is exactly the accept-and-ignore
this codebase bans elsewhere.

The fix is not a bigger grammar. It is a **signature table**, checked immediately
after the parse:

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

"Rejected" means an error naming the operation and why, never a silent ignore.

Two more things the prototype argued for:

- **A value may not be a bare reserved word.** `where name=` followed by a newline and
  `imports` swallowed `imports` as the value and then failed confusingly two tokens
  later. Refusing keywords in value position turns that into
  `name= needs a value; found the step keyword 'imports'`.
- **`where` and modifiers should be order-independent.** The prototype rejected
  `delete on-refusal allow where unused`, which is a rule nobody will remember and
  which buys nothing.

`schema 1` is the first statement in the file and is mandatory. It is what makes the
staged answer to sharing possible: a reader can refuse a file it does not understand
before it has parsed a single step. See *Sharing*, below.

## Selection

The heart of it. Everything else is the existing CLI.

The predicate vocabulary is the **entry-point catalog matcher**, which already exists
in `src/analysis/entrypoints.rs` and already carries rules for thirteen languages:

| Predicate | Matches |
| --- | --- |
| `name="x"` | exactly |
| `name~"pre_*"` | by glob |
| `kind=function` | `function`, `method`, `class`, `selector`, `key`, … |
| `exported` / `!exported` | is or is not |
| `annotated-with="test"` | `#[test]`, `@property`, a build tag |
| `file~"*_test.go"` | by path glob |
| `lang=python` | one of the sixteen |

Reusing it means a recipe's selector and an entry-point rule mean the same thing by
construction, and the matcher gains from being used twice.

A recipe adds predicates that only make sense against a whole workspace — each one an
existing analysis, not new machinery:

| Predicate | Meaning | Comes from |
| --- | --- | --- |
| `in="src/adapters/"` | under a directory | the scanner |
| `unused` | nothing reaches it | `fr unused` |
| `duplicated` | part of a copy-paste class | `fr duplicates` |
| `calls="x"` / `called-by="x"` | edges in the call graph | `fr callers` / `callees` |
| `implements="Sink"` | a concrete answer to an abstraction | `fr implementations` |
| `matches='$A + $B'` | a structural shape | `fr restructure` |
| `changed` | this recipe already touched it | the run itself |

So *"every unused unexported helper under `src/adapters`"* is:

```
delete where unused !exported kind=function in="src/adapters/"
```

**A selector that matches nothing stops the recipe.** Silently doing nothing is the
failure this design most wants to avoid, because it looks exactly like success. Write
`allow-empty` when a step is genuinely conditional.

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

What it becomes sits next to the verb; the selector, which can be long, trails. Two
steps need comment.

**`rewrite`** has no target in the usual sense — it applies at a position. The
selector chooses *files*, and the step applies the transformation everywhere it
applies. This is the most dangerous statement in the language: `guard-clause` was once
wrong at 1,258 of 1,498 sites in helm/helm. It is the one that most needs `limit`,
the dry run, and an `expect`.

**`extract`** takes a range, and a range cannot be selected — it is a judgement about
one specific piece of code. It stays positional, which means a recipe containing one
is about a file rather than a policy. A real limit, better stated than papered over.

**`rename` takes a literal.** There are no computed names in v1: no captures, no case
conversions. A convention-wide rename — `handle_*` to `on_*` — is simply not
expressible, and that is deliberate. Small expression languages grow, and nobody has
asked for this one yet.

## Refusals

The tool refuses rather than guessing. A recipe run at scale collects refusals, and
what it does with them is the decision that matters most here.

| `on-refusal` | Meaning |
| --- | --- |
| `stop` (default) | abandon the run; nothing is written |
| `report` | record it, apply the rest, exit non-zero |
| `allow` | record it, apply the rest, exit zero — the refusals were expected |

There is deliberately no `ignore`. A refusal is always in the report; the only
question is what it does to the exit code. `allow` has to be typed by a person who has
decided these refusals are acceptable — permitted, visible, and attributable.

## Transactions

A recipe is **one transaction**. Either every step's edits are written or none are: a
half-applied recipe leaves a repository in a state nobody designed — the flag removed
and its dead branches still there.

Each step sees the workspace **as the previous step left it**, which means re-indexing
between steps. `Index::build_from_sources` already does exactly this for the cascade
machinery: re-resolve against in-memory results rather than writing to disk to read it
back.

Dry-run is the default, as everywhere else in this tool. `--write` applies.

## Expectations

```
expect changed > 0 files
expect no-new unused
expect no-new duplicates
expect refusals = 0
```

`no-new` is the interesting one: it re-runs the analysis afterwards and compares. A
refactoring that removes a call and orphans three functions has not finished, and this
is how a recipe says so.

Every edit is reparse-checked by the engine regardless — that is not an expectation
you opt into.

## Output

The report is the point, not a side effect. Human by default, `--json` for a machine.
For each step: what was selected and by which predicate, what changed, what was
refused and why, and what `expect` found.

```
recipe retire-legacy-auth — 3 steps

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

A policy, run everywhere — no positions, no file names, it is about a shape:

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

A clean-up that expects to be refused — some of this is public API:

```
schema 1

recipe drop-dead-adapters {
  delete where unused !exported in="src/adapters/"
         on-refusal allow

  expect changed > 0 files
}
```

## Sharing — staged, and honest about it

v1 recipes are **local**: a file beside the code it changes, run as
`fr recipe recipes/retire-legacy-auth.fr --write`. No registry, no fetching, no
running someone else's file against your source.

`schema 1` is carried from day one anyway, because it costs one line now and is
unaddable later. It is the hook a future reader uses to refuse a file it does not
understand.

What sharing would require, written down rather than answered badly:

- **Compatibility.** What does `schema 2` mean for a `schema 1` recipe — is the reader
  required to run it, refuse it, or upgrade it?
- **Blast radius.** A shared recipe edits your source. Does it declare the paths it
  may touch, and is that enforced or advisory?
- **Provenance.** Who wrote it, what does it hash to, and does the run record that in
  the commit it produces? The repository already insists on provenance for vendored
  corpora; a recipe that rewrites your code deserves at least as much.
- **Review.** A diff of a recipe is small and its effect is not. That asymmetry is the
  whole risk, and it is not solved by a version field.

None of these are answered here. They are the reason v1 does not fetch.

## What I am least sure about

1. **`rewrite` at scale.** Selecting files and applying at every applicable position
   is the most useful and most dangerous step. `limit N` is a partial answer; a
   `sample N` that applies to ten sites so a person can *read* them may be better, and
   I do not know which without watching someone use it.

2. **Statement termination by reserved word.** It gives the clean multi-line `where`
   with no punctuation, and it survived the adversarial inputs above — a mistyped
   *predicate* is caught precisely. A mistyped *step name* is the remaining ambiguity:
   `delte where unused` can only be reported as "not a step or directive", because at
   that point the parser genuinely cannot tell a bad step from a bad predicate. A
   closed vocabulary makes "did you mean `delete`?" easy, which is probably enough.

3. **Whether `expect` belongs in the language at all.** It could be a CI concern:
   run the recipe, then run `fr unused` and compare. Keeping it in the file makes the
   recipe self-describing; keeping it out makes the language smaller. I lean towards
   in, because the recipe is meant to be the artifact a reviewer reads.
