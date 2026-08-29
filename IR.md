# The intermediary language

What a translation crosses, and where it stops.

`fr translate` never rewrites one language into another directly. It reads the
source into a single representation, and writes that out again. Seven languages
have a reader and a writer: Rust, Go, Java, Python, TypeScript, Zig and Bash.
Forty-two ordered pairs go through one vocabulary.

The vocabulary lives in `src/transpile/ir.rs`. This document says what is in it,
why each piece earned a place, and what a writer does when it cannot spell one.

## Why a middle at all

Direct translation costs a reader and a writer per pair. Seven languages need
forty-two of them. A middle costs seven of each, and every improvement to one
reader reaches six targets at once.

The middle also decides the honest thing. A pair-to-pair translator either finds
a spelling or produces nothing. A middle holds the construct, hands it to a
writer, and lets the writer say it cannot spell it. That refusal is the report.

## The shape

```
Module
 └── items: Vec<Item>
       ├── Function
       ├── Record
       ├── Sum
       └── …
```

A `Function` holds `Vec<Param>`, an `Option<Type>` return, and a `Vec<Stmt>`
body. A `Record` holds fields and methods. Statements hold expressions, and
expressions hold types.

A `Module` is one file. It carries the file's doc comment, and its stem, which
only Java needs. Java has no top level below the type, so a public class takes its file's name.

## Items

Nine kinds of top-level thing.

| Item | What it holds | The interesting part |
|---|---|---|
| `Function` | Name, parameters, return, body | Carries `receiver`, `receiver_name`, `is_async`, `is_property`, `is_constructor` and `is_private` apart from the body |
| `Record` | Fields and methods | Methods live with the type even where the source declared them apart, as Rust and Go do |
| `Constant` | A name bound once at the top level | |
| `Newtype` | A distinct type over another | Python's `NewType`, a TypeScript brand, a Rust tuple struct |
| `Sum` | A closed choice of variants | An enum with payloads, a tagged union, a discriminated union |
| `Import` | A module path and its named bindings | Counted apart from failures |
| `Test` | A named test beside the code | Zig spells the name as prose, so each writer slugs it |
| `Statement` | A statement at the top of the file | `main();` in a module Python runs top to bottom |
| `Unsupported` | A construct with no counterpart | Holds the source verbatim and the node kind |

Three fields on `Function` exist because the languages disagree about facts, not
about syntax.

- `receiver_name` records the word the source used. Rust, Python and Zig say
  `self`, Java and TypeScript say `this`, and Go says whatever the author chose.
  A writer that kept the source's word would name something its output never
  binds.
- `is_constructor` records that a function makes a value of its type. Java names
  it after the class, Python calls it `__init__`, Rust writes `new` by habit.
  Each target picks its own name from this flag.
- `is_private` stands apart from `exported`. A Rust `fn` without `pub` is the
  module's own, which Java spells package-private. Folded into one bit, a Zig
  method came out `private` in Java and its own class could not call it.

`Import` and `Comment` stay outside `Unsupported` on purpose. Counting them as
failures makes a perfect translation report one, and that noise teaches a reader
to ignore the number.

## Types

Twelve types.

| Type | Reason it is its own thing |
|---|---|
| `Unit` | |
| `Bool` | |
| `Int` | |
| `Float` | |
| `String` | |
| `List(T)` | |
| `Set(T)` | Read as a list, `seen.add(x)` twice put two in and every size was wrong |
| `Map(K, V)` | |
| `Optional(T)` | |
| `Tuple(Vec<Type>)` | Go's multiple return has to cross as the pair it is |
| `Named { name, args }` | Structured, so each writer spells the generic arguments its own way or says it cannot |
| `Fn { params, returns }` | Every language spells a function type differently enough that no name crosses |

Two flags travel with a type-like expression. `assignable` asks whether the
expression can stand left of `=` in any of these languages. Rust assigns through
a dereference of a call, and no target here can. `nameable` asks whether a name
can spell this name as a type at all. A closure or a trait object has no spelling
outside the language that owns it.

## Expressions

Twenty-nine. The scalars and the obvious shapes carry no argument.

`Int`, `Float`, `Str`, `Bool`, `Null`, `Name`, `Field`, `Index`, `Call`,
`Binary`, `Unary`, `ListLit`, `MapLit`, `Tuple`, `Unsupported`.

The rest exist because reading them as something simpler produced a wrong
program.

| Expression | Spelled | Why it is not folded into `Call` or `Binary` |
|---|---|---|
| `Await` | `await x`, `x.await` | Go has none, and dropping the keyword turns a suspension into a plain call |
| `Propagate` | `x?`, `try x` | A dropped `?` turns an early return into a plain call |
| `Keyword` | `name=value` | Only Python has these; dropping the name and trusting the position is a guess |
| `Cast` | `(T) x`, `x as T`, `@as(T, x)` | The source settled what it means; the assertion keeps its place |
| `InstanceOf` | `x instanceof T`, `isinstance(x, T)` | Emitting the call form would be writing Python inside the TypeScript reader |
| `New` | `new Thing(a, b)` | The languages disagree about whether construction is a call |
| `RecordLit` | `Counter { value: 0 }` | Fields stay named, since declaration order says nothing about any constructor |
| `Coalesce` | `a ?? b`, `a orelse b` | Most of these spell it outside the operator table, and three can only say it by naming the value twice |
| `Ternary` | `a ? b : c` | It is a value. A branch needs somewhere to put its result, and an argument list has none |
| `Variant` | `Shape::Circle { radius }` | Without it the types cross while every value of one carries verbatim |
| `Template` | `f"Hi {name}"` | Flattened to text, the expressions inside are lost in silence |
| `Lambda` | `lambda x: e`, `\|x\| e` | Bare names lose a typed TypeScript arrow entirely |
| `SetLit` | `{a, b}`, `set()` | Read as a call, `set()` in Python became a call to a function named `set` in Rust |
| `Comprehension` | `[f(x) for x in xs if p(x)]` | Python builds it one way and TypeScript chains it; modelling it lets each write its own |

`Lambda` holds full `Param` values rather than names. A lambda whose parameters
the source typed keeps those types, and Go and Zig cannot write a function value
without them.

## Statements

Twenty-six. Six are the shapes every language shares.

`Return`, `Let`, `Assign`, `If`, `While`, `ForEach`, plus `Expr`, `Break`,
`Continue`, `Block` and `Comment`.

The rest are loops and bindings that one family writes natively and another says
longhand.

| Statement | Native in | Said longhand by |
|---|---|---|
| `TupleAssign` | Go, Python, Rust, TypeScript | Carried elsewhere, since a lost swap is silent |
| `IfPresent` | Rust, Zig | Python and TypeScript test against null; Java and Go bind twice |
| `CountedFor` | Go, Java, TypeScript, Zig | Rust and Python write the start above and the step at the foot |
| `ForEachIndexed` | Go, Zig, Python, Rust | TypeScript and Java count alongside |
| `WhilePresent` | Rust, Zig | The other four open an unconditional loop and break when empty |
| `Defer` | Go, Zig | Python, TypeScript and Java write `try`/`finally`; Rust carries it |
| `ErrDefer` | Zig | A catch that cleans up and rethrows, in the three exception languages |
| `Switch` | All | Only literal arms cross. Structure, a binding or a range carries whole |
| `MatchVariants` | Rust, Zig | Each asks its own way: a discriminator, `isinstance`, a type switch, `instanceof` |
| `Try` | Python, TypeScript, Java | Rust, Go and Zig model failure in the return, and carry the text |
| `Throw` | Python, TypeScript, Java | |
| `Assert` | Python, Rust, Zig | The rest test the condition and throw or panic |
| `LocalFunction` | All | A nested function, a function literal, or an object holding one method |
| `BreakWith` | Zig | Lowering labeled blocks consumes these; a survivor carries whole |

`CountedFor` deserves its own note. It is Go's only loop keyword, so carrying
its three spellings as comments loses more of Go than any other gap. Rust and
Python have no such header. Their writers put the start above the loop and the
step at the foot of the body. A `continue` skips a step written that way, and
the loop never ends. A body with a `continue` carries whole instead.

## Operators

Seventeen binary operators and three unary ones. Thirteen mean the same thing
in every language here.

`Add`, `Sub`, `Mul`, `Div`, `Rem`, `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`, `And`
and `Or`.

The other four exist because one spelling carried two meanings, and the
translations were quietly wrong.

| Operator | The trap |
|---|---|
| `FloorDiv` | Only Python spells `//` as an operator. Read as unknown, it left a runnable `null` where a number belonged |
| `TrueDiv` | Python's `/` always yields a float. Read as `Div`, Java answered 5 where the source answered 5.34 |
| `FloorRem` | `-7 % 2` is `1` in Python and `-1` in the other six |
| `Xor` | Given its own precedence tier, since C puts it between arithmetic and comparison |

`UnaryOp` has `Not`, `Neg` and `Unwrap`. `Unwrap` is Zig's `x.?` and
TypeScript's `x!`, an assertion that a value is there. A target without the
assertion uses the value and fails where the source would have trapped.

Every operator carries a precedence, and one table serves all seven targets.
The writers render `left op right` and nothing more. Without the table,
`(a + b) * c` comes out as `a + b * c` in every language, a different number.

`==` is in the table with a caveat. It is reference equality in some of these
languages and structural in others. The reader emits it and the writer notes it
where the meaning differs.

## Reading

A reader turns one file into a `Module` and nothing else. It never emits another
language's syntax, and never guesses at a construct it does not recognise. What
it does not recognise becomes `Unsupported`, holding the source text verbatim
and the tree-sitter node kind.

Where a source declared no type, the reader leaves it absent. Type inference
runs afterwards over assignments and call sites. A signature that stays untyped
reaches the writer, and each target reaches for its widest type: `object`,
`any`, `anytype`, `Object`, a Rust type parameter.

## Normalizing

Between the reader and the writer, `src/transpile/normalize.rs` rewrites idioms
into the shared vocabulary. Each pass exists because a literal reading built the
wrong thing.

- A map whose values carry nothing is a set. Go writes `map[T]struct{}` and Zig
  `HashMap(K, void)`. Read as maps, `seen.add` became a store of `None`.
- A constructor that only assigns to the receiver is the record literal it
  builds. Java, Python and TypeScript write a body of assignments. Rust, Go and
  Zig have no receiver to assign to yet.
- Printing folds to one canonical `print`. `fmt.Println`, `console.log`,
  `System.out.println` and Zig's writer plumbing all arrive at it. Zig's
  buffer, writer and flush are how Zig says the thing, so the reader drops them.
- A `%`-verb or `{d}` format string becomes a `Template`. Only the verbs every
  target shares: `%d`, `%s`, `%v`, `%f` and `%%`. Width or precision means
  formatting this cannot promise, so the call stays as written.
- A `match` on `true` and `false` is an `if`. Java has no such switch outside a
  preview feature, and the other five spell it with an `if` anyway.
- `Result<T, E>` becomes the exception shape: the return type is `T`, an `Err`
  is a throw, and `?` re-throws.

## Writing

A writer turns a `Module` into text, and lowers what its target cannot spell.
Four lowerings run before the writer sees the module, chosen by target.

| Pass | Runs for | What it does |
|---|---|---|
| `flatten_local_bases` | Rust, Go, Zig | A base declared in the same module lays its fields and methods flat into the extending record |
| `loops_for_comprehensions` | Go, Zig | A comprehension becomes the loop that builds the collection |
| `functions_for_lambdas` | Zig | A lambda that captures nothing becomes a named top-level function |
| `numbers_as_declared` | Rust, Java | Spell a whole number in a float position with its point |

Each writer holds its own state while it runs. The types of bindings in scope.
Which parameters hold functions, and which records take generic arguments. What
each function returns, and the spelling chosen for each sum variant. With that
state, a call to a sibling's constructor comes out as a construction.

## What carrying whole means

A construct with no counterpart in the target goes into the output as the source
wrote it, under a marker, and counts against the file. The writer never drops it
and never approximates it.

The alternative was tried and is worse. A dropped `defer` leaves a file that
compiles and never cleans up. A dropped `await` leaves a suspension point that
is now a plain call. Both read as a clean translation.

## The report

Every translation ends with a `Fidelity`. It is the point of the exercise: a
translated file is a draft, and using a draft responsibly means knowing where it
stops being one.

| Field | What it counts |
|---|---|
| `functions`, `records`, `constants`, `newtypes`, `sums` | Declarations that came across |
| `signatures_complete` | Every parameter and the return carried with its type |
| `signatures_with_foreign_types` | A type had no counterpart, so the name carried through |
| `signatures_untyped` | The source never gave a type, and the target wrote its widest one |
| `signatures_with_changed_calls` | The types carried, the calling convention did not. A caller writes the call differently |
| `carried_verbatim` | Statements and expressions with no counterpart |
| `imports_listed` | Imports listed rather than translated |
| `notes` | One line per thing that did not translate, with where it was |

`is_complete()` asks two questions: did anything cross at all, and did anything
carry verbatim. Both matter. Without the first, an empty file reports that every
signature carried its types intact, which is true and misleading.

A foreign type does not stop a translation being complete. The name crosses
verbatim, which is the defined behavior, and the count stays in the report. The
same holds for a calling convention the target cannot keep. Only a construct
carried verbatim makes a translation incomplete.

Reading a header:

```
python -> rust (2 function(s), 1 record(s), 0 constant(s)).
  signatures: 2 complete, 0 mentioning a type this tool does not know
```

and the same facts at the top of the file it wrote:

```rust
// Translated from python (demo.py) by fun-refactor.
// 2 function(s), 1 record(s), 0 constant(s).
// Every signature carried across with its types intact.
```

## How the gates hold this

The conformance suite is the evidence. `tests/conformance/` holds fourteen
groups, and each group has one native program per language printing the same
transcript. The suite translates, compiles and runs every ordered pair, then
compares the output against the transcript.

The list of passing cells is a two-way ratchet. A cell that starts passing must
join the list, and a cell that stops passing fails the suite. So no gain slips
away quietly and no regression gets waved through.

Two more gates sit behind it. `tests/corpus_compile.rs` asks each toolchain the
strongest question it can answer about a file whose dependencies live elsewhere.
`tests/corpus_semantic.rs` supplies the foreign world as a generated stub and
lets `rustc` type-check the translation against it.

## See also

- [CLI.md](CLI.md), and `fr translate` in it.
- [RECIPES.md](RECIPES.md), whose `translate` verb runs this under a plan.
- [CROSS_LANGUAGE.md](CROSS_LANGUAGE.md), which references cross a language
  boundary and which do not.
