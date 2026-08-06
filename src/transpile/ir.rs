//! What a file means, said in a way no one language owns.
//!
//! Translation goes source → IR → source, so adding a language costs one reader and
//! one writer rather than a pair for every language already here. Six languages is
//! thirty ordered pairs and twelve files.
//!
//! # What is deliberately in it
//!
//! Declarations, and the parts of a body that mean the same thing everywhere: a
//! return, a binding, a branch, a loop over a collection, a call. Those carry across
//! because every one of these languages has them and they agree about what they do.
//!
//! # What is deliberately not
//!
//! Everything whose meaning is the language: ownership, goroutines, decorators,
//! generators, comprehensions, pattern matching, error propagation. There is no
//! honest general translation of `?` into Python or of a channel into TypeScript, and
//! a guess would be worse than a gap.
//!
//! Those become [`Stmt::Unsupported`] or [`Expr::Unsupported`], which carry **the
//! original text**. The writer emits them as a comment beside a marker, so the result
//! is a file you finish rather than a file you have to diff against the original to
//! discover what was dropped. Nothing is ever silently omitted; the count of what was
//! carried this way is the headline of the report.

use std::fmt;

/// One translated file.
#[derive(Debug, Default, Clone)]
pub struct Module {
    /// The file-level doc comment, where the language has one.
    pub doc: Vec<String>,
    /// What the file is called, where a language needs to know.
    ///
    /// Only Java does: it has no top level below the type, so every function has to be
    /// written inside a class, and a public class must be named after its file. The
    /// other three writers ignore this.
    pub name: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    Record(Record),
    Constant(Constant),
    /// An import.
    ///
    /// Not [`Item::Unsupported`], because an import is not a construct that failed to
    /// translate — it is a dependency declaration, and every one of these languages
    /// resolves dependencies differently. Counting them as failures made a perfect
    /// translation report one, which is the sort of noise that stops anyone reading
    /// the number at all.
    Import {
        text: String,
        line: usize,
    },
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
    /// The six languages disagree — Rust, Python and Zig say `self`, Java and
    /// TypeScript say `this`, and Go says whatever the author called it — and the
    /// receiver is not in the parameter list to be renamed with the rest. Recording
    /// the word here lets a writer spell it its own way; without it every translated
    /// method kept its source's word and referred to a name the output never binds.
    /// `this.cache` inside a Rust `impl` is not a typo, it is a file that cannot
    /// compile.
    pub receiver_binding: Option<String>,
    pub params: Vec<Param>,
    pub returns: Option<Type>,
    pub body: Vec<Stmt>,
    pub exported: bool,
    /// Reported rather than translated: a Rust `async fn` written as Python must say
    /// so, and a Go one cannot be written at all.
    pub is_async: bool,
    /// Does this function make a value of its type?
    ///
    /// Three of these six languages have a constructor and three have a convention:
    /// Java names it after the class, Python calls it `__init__`, TypeScript calls it
    /// `constructor`, and Rust, Go and Zig write `new`, `NewThing` and `init` by habit.
    /// Which of those a target writes is a fact about the target, so the *name* is not
    /// what carries — this is. Without it a Java constructor was a class member nothing
    /// recognised, and it was dropped.
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
/// parameters produced `export function createUser(*: unknown, ...)` — a file
/// TypeScript will not parse, found by the translator's own parse check. Reading them
/// as *nothing* would be worse: the signature would look carried when the way callers
/// must invoke it had changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamKind {
    #[default]
    Normal,
    /// `*args` — the rest, positionally.
    VarArgs,
    /// `**kwargs` — the rest, by name.
    KeywordArgs,
    /// A bare `*` or `/`: not a parameter, a rule about the ones around it.
    Marker,
}

/// A struct, class, dataclass or interface — a named product of fields.
#[derive(Debug, Clone)]
pub struct Record {
    pub doc: Vec<String>,
    pub name: String,
    pub fields: Vec<Field>,
    /// The type it inherits from, where the source has inheritance.
    ///
    /// Three of these six languages do and three do not, so this is carried where it
    /// can be and *reported* where it cannot. Dropping it silently made
    /// `class JsonPrimitive extends JsonElement` into a class that extends nothing —
    /// which is a different type, and the output said nothing about it.
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
    /// A type the reader recognised the shape of but not the meaning.
    ///
    /// Structured rather than opaque text, because generic syntax differs: writing
    /// Rust's `Result<(), String>` into a Python annotation produced a file Python
    /// cannot parse. Keeping the arguments apart lets each writer spell them its own
    /// way, and lets a writer that cannot spell them at all say so.
    Named {
        name: String,
        args: Vec<Type>,
    },
}

impl Type {
    /// A named type with no arguments — the common case.
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
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    /// `for x in xs` — the shape every language here shares. A C-style `for` is not
    /// this, and is carried as unsupported.
    ForEach {
        binding: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Expr(Expr),
    /// A comment on its own line.
    ///
    /// Every language here has one and they differ only in the marker. Treating a
    /// comment as an untranslatable construct — which is what happened before this
    /// existed — put `// Validate the route params.` in the output under a "not
    /// translated" marker, and inflated the count of real gaps with things that were
    /// never gaps.
    Comment(String),
    /// `raise e` / `throw e`.
    Throw(Expr),
    /// `try { } catch { } finally { }`, and Python's `try/except/finally`.
    ///
    /// Half of these languages have it. Rust and Zig model failure in the return type
    /// and Go returns an error value, and none of the three has any general translation
    /// of a catch block — so those writers carry it, which is why the original text
    /// travels with it. Python, TypeScript and Java agree closely enough to translate:
    /// a typed `except` becomes an `instanceof` test inside one `catch`, which is
    /// exactly how the same intent is written in the other two.
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
    /// until this resolves. Only the spelling differs — prefix in Python and
    /// TypeScript, postfix in Rust. Go has no counterpart and says so rather than
    /// dropping the keyword, which would turn a suspension point into a plain call.
    Await(Box<Expr>),
    /// `name=value` in an argument list.
    ///
    /// Python has these and the other three do not, so a writer without them carries
    /// the call rather than dropping the name and hoping the position is right.
    Keyword {
        name: String,
        value: Box<Expr>,
    },
    /// `x instanceof T`, `isinstance(x, T)`.
    ///
    /// The same question in both, spelled as an operator in one and a builtin in the
    /// other, which is why it is a node rather than a call: a reader that emitted
    /// `isinstance(...)` would be writing Python inside the TypeScript reader.
    InstanceOf {
        value: Box<Expr>,
        ty: Box<Expr>,
    },
    /// `new Thing(a, b)`.
    ///
    /// Kept apart from [`Expr::Call`] because the languages disagree about whether
    /// construction is a call: Python and Go say yes, TypeScript needs the keyword,
    /// and Rust has no universal spelling at all.
    New {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `Counter { value: 0, step }` — a record built by naming its fields.
    ///
    /// Distinct from [`Expr::New`], which passes arguments in an order the callee
    /// decides. Four of these languages construct a record this way and two do not, so
    /// the fields have to stay named for as long as it takes to find out which target
    /// is being written — a positional list assembled here would be in the source's
    /// declaration order, which is a fact about the source and not about the
    /// constructor anyone will call.
    RecordLit {
        ty: String,
        fields: Vec<(String, Expr)>,
    },
    /// `a ?? b`, `a orelse b` — the value unless it is absent, and then the fallback.
    ///
    /// Its own node rather than a [`BinaryOp`], because it is not an operator on values
    /// in most of these languages: Zig spells it `orelse`, Rust reaches for
    /// `Option::unwrap_or`, Java for a static method, and Go has nothing at all. What is
    /// shared is the *question* — is this absent, and what then — and that is what
    /// crosses.
    ///
    /// The catch is that three of the six can only say it by naming the value twice. A
    /// value that is a call cannot be named twice without calling it twice, so those
    /// writers say so rather than changing how many times the program does something.
    Coalesce {
        value: Box<Expr>,
        fallback: Box<Expr>,
    },
    /// `a ? b : c`, `b if a else c`, `if a { b } else { c }`.
    ///
    /// One expression that chooses between two, and five of these six languages have
    /// it — only Go does not, and Go says so rather than inventing a statement out of
    /// an expression. It is a node rather than an [`Stmt::If`] because it *is* a value:
    /// reading it as a branch would need somewhere to put the result, and there is no
    /// such place inside an argument list.
    Ternary {
        condition: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// `[a, b, c]`
    ListLit(Vec<Expr>),
    /// `{"a": 1}` in Python, `{ a: 1 }` in TypeScript, a map literal in Go.
    MapLit(Vec<(Expr, Expr)>),
    /// An interpolated string: `f"Hi {name}"`, `` `Hi ${name}` ``.
    ///
    /// Kept as parts rather than text because flattening it loses the expressions —
    /// which is exactly the silent wrong answer this had before it existed.
    Template(Vec<TemplatePart>),
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
            other => other.c_like(),
        }
    }

    /// How tightly this operator binds. Higher binds tighter.
    ///
    /// One table for every target, because every target orders these the same way —
    /// multiplication before addition, arithmetic before comparison, comparison before
    /// `and`, `and` before `or`. Python spells two of them with words and agrees about
    /// all of it.
    ///
    /// This exists because the writers rendered `left op right` and nothing else, so a
    /// group the source wrote was a group the translation lost: `(a + b) * c` came out
    /// as `a + b * c` in all six languages, which is a different number.
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 6,
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
/// draft responsibly is to know exactly where it stops being one.
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
    /// Imports listed rather than translated. Counted apart because they are not a
    /// failure to translate anything.
    pub imports_listed: usize,
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
        self.functions + self.records + self.constants
    }
}
