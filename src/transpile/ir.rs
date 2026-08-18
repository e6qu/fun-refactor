//! What a file means, said in a way no one language owns.
//!
//! Translation goes source → IR → source, so adding a language costs one reader and one writer
//! instead of a pair for every language already here. Six languages is thirty ordered pairs and
//! twelve files.
//!
//! # In it
//!
//! Declarations, and the parts of a body every one of these languages has and agrees about. A
//! return, a binding, a branch, a loop over a collection, a call.
//!
//! # Not in it
//!
//! Constructs whose meaning is the language: ownership, goroutines, decorators,
//! generators, pattern matching beyond literal arms. A channel has no
//! translation into TypeScript.
//!
//! Those become [`Stmt::Unsupported`] or [`Expr::Unsupported`], carrying the original text. The
//! writer emits each as a comment beside a marker, and the report leads with how many crossed
//! that way.

use std::fmt;

/// One translated file.
#[derive(Debug, Default, Clone)]
pub struct Module {
    /// The file-level doc comment, where the language has one.
    pub doc: Vec<String>,
    /// What the file is called, where a language needs to know.
    ///
    /// Only Java does: it has no top level below the type. So every function has to be written
    /// inside a class, and a public class must be named after its file. The other three writers
    /// ignore this.
    pub name: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    Record(Record),
    Constant(Constant),
    Newtype(Newtype),
    Sum(Sum),
    /// An import.
    ///
    /// Not [`Item::Unsupported`], because an import is not a construct that failed to
    /// translate. It is a dependency declaration, and every one of these languages
    /// resolves dependencies differently. Counting them as failures made a perfect
    /// translation report one, which is the sort of noise that stops anyone reading
    /// the number at all.
    Import {
        text: String,
        line: usize,
        /// What the line says, taken apart, when the reader could take it apart.
        ///
        /// `None` for the forms nothing rewrites: a namespace import, a default
        /// import, a Rust `use`. Those carry as text alone, and the writers keep
        /// them as comments.
        target: Option<ImportTarget>,
    },
    /// A named test, declared in the source file beside the code it checks.
    ///
    /// Zig spells it `test "name" { … }`, and the name is prose rather than an
    /// identifier. Each writer slugs it into its own convention: a `#[test]`
    /// function, a `def test_…`, a `func Test…(t *testing.T)`. TypeScript and
    /// Java name no runner in the language itself, so both write a plain
    /// function and say what is missing.
    Test {
        doc: Vec<String>,
        name: String,
        body: Vec<Stmt>,
    },
    /// A statement at the top of the file: `main();`, the program's own entry.
    ///
    /// Python and TypeScript run a module top to bottom, so such a statement is the
    /// program. Dropped as unsupported, a translated program parsed cleanly, ran,
    /// and printed nothing. The targets whose top level only declares say so instead.
    Statement(Stmt),
    /// A top-level construct with no counterpart: a Rust `impl Trait for T`, a Go
    /// `init()`, a Python decorator that is not a known one.
    Unsupported(Unsupported),
}

#[derive(Debug, Clone)]
pub struct Function {
    pub doc: Vec<String>,
    pub name: String,
    /// The type this is a method on, when it is one.
    pub receiver: Option<String>,
    /// What the source called the receiver inside the body.
    ///
    /// The six languages disagree. Rust, Python and Zig say `self`, Java and TypeScript say
    /// `this`. Go says whatever the author called it, and the receiver is not in the parameter
    /// list to be renamed with the rest. Recording the word here lets a writer spell it its own
    /// way. Without it every translated method kept its source's word and referred to a name
    /// the output never binds. `this.cache` inside a Rust `impl` is not a typo, it is a file
    /// that cannot compile.
    pub receiver_binding: Option<String>,
    pub params: Vec<Param>,
    pub returns: Option<Type>,
    pub body: Vec<Stmt>,
    pub exported: bool,
    /// Reported and not translated: a Rust `async fn` written as Python must say
    /// so, and a Go one cannot be written at all.
    pub is_async: bool,
    /// Is this method read as data at its use sites?
    ///
    /// Python's `@property` and TypeScript's `get` both declare a method whose
    /// callers write `it.total`, no parentheses. Read as an ordinary method,
    /// the declaration crossed while the accessors kept their spelling. In the
    /// target `it.total` was the function object itself, and every comparison
    /// against it was quietly false. Targets with the idiom keep it; targets without one
    /// write a method and spell the accessors as the calls they become.
    pub is_property: bool,
    /// Does this function make a value of its type?
    ///
    /// Three of these six languages have a constructor and three have a convention. Java names
    /// it after the class, Python calls it `__init__`, TypeScript calls it `constructor`. Rust,
    /// Go and Zig write `new`, `NewThing` and `init` by habit. Which of those a target writes
    /// is a fact about the target, so the *name* is not what carries. This is. Without it a
    /// Java constructor was a class member nothing recognised, and it was dropped.
    pub is_constructor: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
    pub kind: ParamKind,
}

/// How a parameter is passed, which is part of the signature and not decoration.
///
/// Python writes `def f(*, session, user)` to make everything after the `*`
/// keyword-only, and `*args` / `**kwargs` to take the rest. Reading those as ordinary
/// parameters produced `export function createUser(*: unknown, ...)`, a file
/// TypeScript will not parse, found by the translator's own parse check. Reading them
/// as *nothing* would be worse: the signature would look carried when the way callers
/// must invoke it had changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamKind {
    #[default]
    Normal,
    /// `*args`, the rest, positionally.
    VarArgs,
    /// `**kwargs`, the rest, by name.
    KeywordArgs,
    /// A bare `*` or `/`: not a parameter, a rule about the ones around it.
    Marker,
}

/// A struct, class, dataclass or interface, a named product of fields.
#[derive(Debug, Clone)]
pub struct Record {
    pub doc: Vec<String>,
    pub name: String,
    pub fields: Vec<Field>,
    /// The type it inherits from, where the source has inheritance.
    ///
    /// Three of these six languages do and three do not, so this is carried where it can be and
    /// *reported* where it cannot. Dropping it silently made `class JsonPrimitive extends
    /// JsonElement` into a class that extends nothing, which is a different type. The output
    /// said nothing about it.
    pub extends: Option<String>,
    pub exported: bool,
    /// Methods declared on it. Rust and Go declare them apart from the type; Python
    /// and TypeScript declare them inside. The IR keeps them with the type, which is
    /// what lets one shape become the other.
    pub methods: Vec<Function>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub doc: Vec<String>,
    pub name: String,
    pub ty: Option<Type>,
    /// The value the field starts with, where the source gave one.
    ///
    /// `rows: T[] = [];` lost its initializer, so the dataclass it became had a
    /// required argument nothing passes and construction raised. A target that
    /// cannot write a default says so in the report instead of dropping it.
    pub default: Option<Expr>,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub doc: Vec<String>,
    pub name: String,
    pub ty: Option<Type>,
    pub value: Expr,
    pub exported: bool,
}

/// A distinct type over an existing one, worth one line in every language here.
///
/// Python spells it `Pence = NewType("Pence", int)`, and TypeScript as a brand.
/// Rust makes a tuple struct, Go a defined type, Java a one-component record.
/// Read as a [`Constant`], the source language's incantation crossed into the
/// target as a value: output that parses and means nothing.
#[derive(Debug, Clone)]
pub struct Newtype {
    pub doc: Vec<String>,
    pub name: String,
    pub base: Type,
    pub exported: bool,
}

/// A closed choice: a value is exactly one of the named variants, and each variant
/// may carry its own fields.
///
/// Rust spells it `enum` with payloads, and Zig `union(enum)`. TypeScript writes a
/// union of object types told apart by a literal field; Python a union of
/// dataclasses. Java spells it `sealed interface` over records. Go has no closed
/// choice and uses the marker-interface convention. Read as anything less, the
/// variants collapse. A reader that flattened one into a record lost *which*
/// variant a value was, the whole meaning of the type.
#[derive(Debug, Clone)]
pub struct Sum {
    pub doc: Vec<String>,
    pub name: String,
    pub variants: Vec<Variant>,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub doc: Vec<String>,
    pub name: String,
    /// The discriminator literal the source wrote, where the language writes
    /// one: `kind: "idle"` on an interface named `FIdle`. Deriving the tag
    /// from the name spelled it `f_idle`, and every consumer comparing
    /// against `"idle"` missed. `None` for the languages that discriminate by
    /// type instead of by field.
    pub tag: Option<String>,
    /// Empty for a bare tag like `None` or `Empty`.
    pub fields: Vec<Field>,
}

/// An import taken apart: where it points and which names it binds.
///
/// Only Python and TypeScript fill this in, and only for the named form. That
/// form is `from m import a, b as c` or `import { a, b as c } from "./m"`. A
/// directory sweep uses it to turn an import of a sibling file into a real
/// import of the sibling's translation. Everything else keeps the text.
#[derive(Debug, Clone)]
pub struct ImportTarget {
    /// The module path as the source wrote it: `helpers`, `.models`, `./m`.
    pub module: String,
    /// Whether the path is relative, a leading dot in Python or `./` here.
    pub relative: bool,
    /// The named bindings, each with the alias the body uses, where one is given.
    pub names: Vec<ImportedName>,
    /// The sibling file stem this import points at, when it points inside a sweep.
    ///
    /// Filled by the directory sweep and by nothing else. A reader cannot know
    /// which files travel together; only the sweep holds the whole set. The
    /// writers emit a real import when this is set and a comment when it is not.
    pub resolved: Option<String>,
}

/// One name an import binds, with its alias where the source gave one.
#[derive(Debug, Clone)]
pub struct ImportedName {
    pub name: String,
    pub alias: Option<String>,
}

/// One arm of a [`Stmt::MatchVariants`]: the variant it selects, the payload
/// fields the body reads (field name, local name), and the body itself.
#[derive(Debug, Clone)]
pub struct VariantArm {
    pub variant: String,
    pub bindings: Vec<(String, String)>,
    pub body: Vec<Stmt>,
}

/// Something with no counterpart, carried whole so nothing is lost.
#[derive(Debug, Clone)]
pub struct Unsupported {
    /// What the source called it, for the report: `impl_item`, `decorated_definition`.
    pub construct: String,
    /// The original source, verbatim.
    pub source: String,
    pub line: usize,
}

/// A type, as far as one can be carried between languages.
///
/// The scalars and the two containers are the part that genuinely corresponds.
/// Anything else is [`Type::Named`] and is written through unchanged with a note,
/// because renaming a type this tool does not understand is how a signature quietly
/// stops meaning what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Float,
    String,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Optional(Box<Type>),
    /// `(int, error)`, `tuple[int, str]`, `[number, string]`: several types as one.
    ///
    /// Go's multiple return is why this exists. Its result type has to cross as
    /// the pair it is; flattening it to a name produced `Unwritable_int__error`
    /// in a signature every target could spell.
    Tuple(Vec<Type>),
    /// A type the reader recognised the shape of but not the meaning.
    ///
    /// Structured and not opaque text, because generic syntax differs: writing
    /// Rust's `Result<(), String>` into a Python annotation produced a file Python
    /// cannot parse. Keeping the arguments apart lets each writer spell them its own
    /// way, and lets a writer that cannot spell them at all say so.
    Named {
        name: String,
        args: Vec<Type>,
    },
}

impl Type {
    /// A named type with no arguments, the common case.
    pub fn named(name: impl Into<String>) -> Type {
        Type::Named {
            name: name.into(),
            args: Vec::new(),
        }
    }

    /// Can this name be written as a type at all, in any of these languages?
    ///
    /// A tuple, a reference, a closure or a trait object has no spelling outside the
    /// language that owns it. Saying so beats emitting something that will not parse.
    pub fn is_writable_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
            && name
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Unit => write!(f, "unit"),
            Type::Bool => write!(f, "bool"),
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::String => write!(f, "string"),
            Type::List(inner) => write!(f, "list<{inner}>"),
            Type::Map(k, v) => write!(f, "map<{k}, {v}>"),
            Type::Optional(inner) => write!(f, "optional<{inner}>"),
            Type::Tuple(parts) => {
                let rendered: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
                write!(f, "tuple<{}>", rendered.join(", "))
            }
            Type::Named { name, args } if args.is_empty() => write!(f, "{name}"),
            Type::Named { name, args } => {
                let rendered: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{name}<{}>", rendered.join(", "))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Return(Option<Expr>),
    /// A new binding. `mutable` is recorded because Rust needs it and the others do
    /// not; going the other way it is assumed.
    Let {
        name: String,
        ty: Option<Type>,
        value: Option<Expr>,
        mutable: bool,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    If {
        condition: Expr,
        then: Vec<Stmt>,
        otherwise: Vec<Stmt>,
    },
    /// `if let Some(x) = e`, `if (e) |x|`: test an optional and bind its payload.
    ///
    /// Rust and Zig bind in the condition. Python and TypeScript spell an
    /// optional as a nullable value, so their writers name the value first and
    /// test it against null. That says the same thing. Java's `Optional` and
    /// Go's pointer cannot unwrap in place. Those writers hold the value in a
    /// second binding, named after the first, and unwrap it into the payload
    /// name inside the branch.
    IfPresent {
        binding: String,
        value: Expr,
        then: Vec<Stmt>,
        otherwise: Vec<Stmt>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    /// `for i, x in enumerate(xs)`, `for (xs, 0..) |x, i|`: each element beside
    /// its position, counted from zero.
    ///
    /// Go and Zig write it in the loop header, Python through `enumerate`,
    /// Rust through `.iter().enumerate()`. TypeScript and Java have no indexed
    /// form over an arbitrary iterable, so both count alongside. A counter is
    /// declared before the loop and stepped at its end.
    ForEachIndexed {
        index: String,
        binding: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    /// Run a body when the enclosing scope exits: Go's and Zig's `defer`.
    ///
    /// Go and Zig keep the word. Python, TypeScript and Java say the same thing
    /// with `try`/`finally`: what follows the defer goes in the `try`, its body
    /// in the `finally`. Stacked defers nest, which keeps their last-in,
    /// first-out order. Rust has no scope-exit hook short of inventing a guard
    /// type, so it carries the body as a comment.
    Defer(Vec<Stmt>),
    /// Zig's `errdefer`: run only when the scope is left on the failure path.
    ///
    /// The exception languages say the same thing with a catch that cleans up
    /// and rethrows, wrapped around the rest of the scope. It crosses to
    /// Python, TypeScript and Java as that shape. Go has no failure path a block can
    /// watch, and Rust would need a guard type; both carry it, visibly.
    ErrDefer(Vec<Stmt>),
    /// One value branched against literal alternatives.
    ///
    /// Rust and Zig spell it with arrows, the C family with `case`, Python with
    /// `match`. Only arms selected by literals cross. A pattern with structure
    /// in it, a binding, a range, a payload, is a match in the full sense and
    /// carries whole. Rust and Zig demand exhaustiveness, so their writers emit
    /// an empty final arm when the source had no default.
    Switch {
        subject: Expr,
        /// Each arm: the literals that select it, and its body.
        arms: Vec<(Vec<Expr>, Vec<Stmt>)>,
        /// The `_` / `else` / `default` body; empty when the source had none.
        default: Vec<Stmt>,
    },
    /// One sum value branched by variant, each arm's payload bound by name.
    ///
    /// The construction crossed a pass before the consumption did: `s.kind ==
    /// "circle"` and `s.radius` went to Rust verbatim, against an enum that
    /// declares neither, while the header said every signature carried. Each
    /// language asks the question its own way. TypeScript compares the
    /// discriminator, Python asks `isinstance`, Rust and Zig match, Go switches
    /// on type, Java tests `instanceof`. The bindings are the payload fields an
    /// arm actually reads, bound to plain locals so every writer can spell the
    /// narrowing without knowing the others' idioms.
    MatchVariants {
        subject: Expr,
        sum: String,
        arms: Vec<VariantArm>,
        /// The `else` / `default` body; empty when the source had none.
        default: Vec<Stmt>,
    },
    /// `while let Some(x) = e`, `while (e) |x|`: loop while the optional holds a
    /// payload, re-evaluating it each pass.
    ///
    /// Native in Rust and Zig. The other four have no binding form in a loop
    /// header. Their writers open an unconditional loop, take the value, and
    /// break when it is empty, which is the same loop said longhand.
    WhilePresent {
        binding: String,
        value: Expr,
        body: Vec<Stmt>,
    },
    /// `for x in xs`, the shape every language here shares. A C-style `for` is not
    /// this, and is carried as unsupported.
    ForEach {
        binding: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Expr(Expr),
    /// `assert c, "m"`: check a condition and stop the program when it fails.
    ///
    /// Python spells it as a statement, Rust as the `assert!` family, Zig as
    /// `std.debug.assert`. Read as an unknown construct, every check carried as
    /// a comment. A translated test file then printed "all tests passed", the
    /// worst kind of green. The targets without an assert say the same thing
    /// longhand: test the condition and throw or panic.
    Assert {
        condition: Expr,
        /// The words the failure prints, where the source gave any.
        message: Option<Expr>,
    },
    /// A comment on its own line.
    ///
    /// Every language here has one and they differ only in the marker. Treating a comment as an
    /// untranslatable construct, which happened before this existed, put `// Validate
    /// the route params.` in the output under a "not translated" marker. Inflated the count of
    /// real gaps with things that were never gaps.
    Comment(String),
    /// `raise e` / `throw e`.
    Throw(Expr),
    /// `try { } catch { } finally { }`, and Python's `try/except/finally`.
    ///
    /// Half of these languages have it. Rust and Zig model failure in the return type and Go
    /// returns an error value. None of the three has any general translation of a catch block,
    /// so those writers carry it, so the original text travels with it. Python, TypeScript and
    /// Java agree closely enough to translate. A typed `except` becomes an `instanceof` test
    /// inside one `catch`, which is how the same intent is written in the other two.
    Try {
        body: Vec<Stmt>,
        catches: Vec<Catch>,
        finally: Vec<Stmt>,
        /// The original, for the two writers that have no counterpart for this.
        source: String,
        line: usize,
    },
    Break,
    Continue,
    Unsupported(Unsupported),
}

/// One `except` or `catch` clause.
#[derive(Debug, Clone)]
pub struct Catch {
    /// The name the error is bound to, where one is written.
    pub binding: Option<String>,
    /// The exception type it selects on, where the language has typed clauses.
    pub ty: Option<Type>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(String),
    Float(String),
    Str(String),
    Bool(bool),
    Null,
    Name(String),
    Field {
        of: Box<Expr>,
        name: String,
    },
    Index {
        of: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    /// `await x`, `x.await`.
    ///
    /// Three of these languages have it and mean the same thing by it: suspend
    /// until this resolves. Only the spelling differs, prefix in Python and
    /// TypeScript, postfix in Rust. Go has no counterpart and says so instead of
    /// dropping the keyword, which would turn a suspension point into a plain call.
    Await(Box<Expr>),
    /// Evaluate, and on failure leave the function with the failure: Rust's `x?`,
    /// Zig's `try x`.
    ///
    /// Python, TypeScript and Java do the same thing with no spelling at all:
    /// an exception propagates unless something catches it. Their writers emit
    /// the expression bare and say so once. Go has no propagation and carries
    /// it, because a dropped `?` would turn an early return into a plain call.
    Propagate(Box<Expr>),
    /// `name=value` in an argument list.
    ///
    /// Python has these and the other three do not. So a writer without them carries the call
    /// instead of dropping the name and hoping the position is right.
    Keyword {
        name: String,
        value: Box<Expr>,
    },
    /// `(T) x`, `x as T`, `@as(T, x)`: the value reasserted as a type.
    ///
    /// Every language here can spell it. They disagree about what it does: a Java
    /// cast checks at run time, a TypeScript `as` checks nothing, a Rust `as`
    /// converts. The source already settled that; the translation keeps the
    /// assertion where it stood.
    Cast {
        ty: Box<Expr>,
        value: Box<Expr>,
    },
    /// `x instanceof T`, `isinstance(x, T)`.
    ///
    /// The same question in both, spelled as an operator in one and a builtin in the other, so
    /// it is a node and not a call: a reader that emitted `isinstance(...)` would be writing
    /// Python inside the TypeScript reader.
    InstanceOf {
        value: Box<Expr>,
        ty: Box<Expr>,
    },
    /// `new Thing(a, b)`.
    ///
    /// Kept apart from [`Expr::Call`] because the languages disagree about whether construction
    /// is a call. Python and Go say yes, TypeScript needs the keyword, and Rust has no
    /// universal spelling at all.
    New {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `Counter { value: 0, step }`, a record built by naming its fields.
    ///
    /// Distinct from [`Expr::New`], which passes arguments in an order the callee
    /// decides. Four of these languages construct a record this way and two do not, so
    /// the fields have to stay named for as long as it takes to find out which target
    /// is being written, a positional list assembled here would be in the source's
    /// declaration order, which is a fact about the source and not about the
    /// constructor anyone will call.
    RecordLit {
        ty: String,
        fields: Vec<(String, Expr)>,
    },
    /// `a ?? b`, `a orelse b`, the value unless it is absent, and then the fallback.
    ///
    /// Its own node instead of a [`BinaryOp`], because it is not an operator on values in most
    /// of these languages: Zig spells it `orelse`, Rust reaches for `Option::unwrap_or`, Java
    /// for a static method, and Go has nothing at all. What is shared is the *question*, is
    /// this absent, and what then, and that is what crosses.
    ///
    /// The catch is that three of the six can only say it by naming the value twice. A value
    /// that is a call cannot be named twice without calling it twice. So those writers say so
    /// instead of changing how many times the program does something.
    Coalesce {
        value: Box<Expr>,
        fallback: Box<Expr>,
    },
    /// `a ? b : c`, `b if a else c`, `if a { b } else { c }`.
    ///
    /// One expression that chooses between two, and five of these six languages have it, only
    /// Go does not, and Go says so instead of inventing a statement out of an expression. It is
    /// a node and not an [`Stmt::If`] because it *is* a value: reading it as a branch would
    /// need somewhere to put the result. There is no such place inside an argument list.
    Ternary {
        condition: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// One variant of a closed choice, made: `Shape::Circle { radius }`,
    /// `.{ .one = n }`, `{ kind: "circle", radius }`.
    ///
    /// The types crossed for eleven passes while every value of one carried, in
    /// every direction at once. Each language builds the same thing its own way.
    /// Rust names the path, Zig the dot-literal, TypeScript writes the
    /// discriminator field, Python and Java call the variant's own constructor,
    /// Go builds the variant struct. Fields are empty for a bare tag.
    Variant {
        sum: String,
        name: String,
        fields: Vec<(String, Expr)>,
    },
    /// `(a, b)`: several values travelling as one, without a name for the whole.
    ///
    /// Go returns them, and dropping the payload of `return a, b` turned a two-value
    /// return into a bare `return` with nothing said. This node ends that silent
    /// wrong answer. Rust and Python write tuples anywhere; TypeScript
    /// spells the value as an array. Java has no spelling and says so.
    Tuple(Vec<Expr>),
    /// `[a, b, c]`
    ListLit(Vec<Expr>),
    /// `{"a": 1}` in Python, `{ a: 1 }` in TypeScript, a map literal in Go.
    MapLit(Vec<(Expr, Expr)>),
    /// An interpolated string: `f"Hi {name}"`, `` `Hi ${name}` ``.
    ///
    /// Kept as parts and not text because flattening it loses the expressions,
    /// which is exactly the silent wrong answer this had before it existed.
    Template(Vec<TemplatePart>),
    /// `lambda x: e`, `(x) => e`, `|x| e`: a nameless function of one expression.
    ///
    /// Only the single-expression shape crosses, because it is the shape all four
    /// languages that have one agree on. A block body is a function that wants a
    /// name and stays carried. Go and Zig cannot write a closure without types,
    /// so their writers carry this too, visibly. Before this existed, every
    /// `sorted(key=...)` callback crossed as a runnable `null`.
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
    /// `[f(x) for x in xs if p(x)]`, and `xs.filter(p).map(f)`.
    ///
    /// The same idea spelled two ways: Python builds it with a comprehension,
    /// TypeScript with a chain. Modelling it lets each write its own.
    Comprehension {
        element: Box<Expr>,
        binding: String,
        iterable: Box<Expr>,
        condition: Option<Box<Expr>>,
    },
    Unsupported(Unsupported),
}

/// One piece of an interpolated string.
#[derive(Debug, Clone)]
pub enum TemplatePart {
    Text(String),
    Expr(Expr),
}

/// The operators that mean the same thing in every language here.
///
/// Notably absent: `==` on anything but scalars, which is reference equality in some
/// of these and structural in others. The reader emits it; the writer notes it where
/// the semantics differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    /// `a // b`: division that rounds toward negative infinity.
    ///
    /// Only Python spells it as an operator, and reading it as an unknown one left
    /// a runnable `null` where a number belonged. The other five say it with a
    /// library call, `Math.floor`, `div_euclid`, `Math.floorDiv`, `math.Floor`,
    /// `@divFloor`, and each writer reaches for its own.
    FloorDiv,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    /// The spelling shared by Rust, Go and TypeScript.
    pub fn c_like(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            // No C-family language spells floor division as an operator. Every
            // writer says it with its own library call before reaching for this
            // table. Asking here is a bug in the writer, said out loud.
            BinaryOp::FloorDiv => unreachable!("floor division has no shared operator spelling"),
            BinaryOp::Rem => "%",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        }
    }

    pub fn python(self) -> &'static str {
        match self {
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
            BinaryOp::FloorDiv => "//",
            other => other.c_like(),
        }
    }

    /// How tightly this operator binds. Higher binds tighter.
    ///
    /// One table for every target, because every target orders these the same way,
    /// multiplication before addition, arithmetic before comparison, comparison before `and`,
    /// `and` before `or`. Python spells two of them with words and agrees about all of it.
    ///
    /// This exists because the writers rendered `left op right` and nothing else. So a group
    /// the source wrote was a group the translation lost. `(a + b) * c` came out as `a + b * c`
    /// in all six languages, which is a different number.
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::FloorDiv | BinaryOp::Rem => 6,
            BinaryOp::Add | BinaryOp::Sub => 5,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 4,
            BinaryOp::Eq | BinaryOp::Ne => 3,
            BinaryOp::And => 2,
            BinaryOp::Or => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

/// What a translation managed and what it did not.
///
/// The point of the exercise. A translated file is a draft, and the only way to use a
/// draft responsibly is to know where it stops being one.
#[derive(Debug, Default, Clone)]
pub struct Fidelity {
    pub functions: usize,
    pub records: usize,
    pub constants: usize,
    /// Signatures carried across with every parameter and the return type intact.
    pub signatures_complete: usize,
    /// Signatures where some type had no counterpart and was written through by name.
    pub signatures_with_foreign_types: usize,
    /// Statements and expressions carried verbatim because nothing corresponds.
    pub carried_verbatim: usize,
    /// Imports listed and not translated. Counted apart because they are not a
    /// failure to translate anything.
    pub imports_listed: usize,
    /// Distinct types carried across: a `NewType`, a brand.
    pub newtypes: usize,
    /// Closed choices carried across: an enum with payloads, a tagged union, a
    /// discriminated union.
    pub sums: usize,
    /// Signatures whose *types* carried but whose calling convention did not: a
    /// keyword-only marker, `*args` or `**kwargs` with no counterpart in the target.
    /// A caller of the translated function writes the call differently.
    pub signatures_with_changed_calls: usize,
    /// One line per thing that did not translate, with where it was.
    pub notes: Vec<String>,
}

impl Fidelity {
    /// Did everything cross intact?
    ///
    /// A translation that read *nothing* is not a complete one. Without the first
    /// clause an empty file reported "every signature carried across with its types
    /// intact", which is true and utterly misleading.
    pub fn is_complete(&self) -> bool {
        self.translated() > 0
            && self.carried_verbatim == 0
            && self.signatures_with_foreign_types == 0
            && self.signatures_with_changed_calls == 0
    }

    /// How many declarations came across at all.
    pub fn translated(&self) -> usize {
        self.functions + self.records + self.constants + self.newtypes + self.sums
    }
}
