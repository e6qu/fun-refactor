//! What a file means, said in a way no one language owns.
//!
//! Translation goes source → IR → source, so adding a language costs one reader and
//! one writer rather than a pair for every language already here. Four languages is
//! twelve ordered pairs and eight files.
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
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    Record(Record),
    Constant(Constant),
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
    pub params: Vec<Param>,
    pub returns: Option<Type>,
    pub body: Vec<Stmt>,
    pub exported: bool,
    /// Reported rather than translated: a Rust `async fn` written as Python must say
    /// so, and a Go one cannot be written at all.
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
}

/// A struct, class, dataclass or interface — a named product of fields.
#[derive(Debug, Clone)]
pub struct Record {
    pub doc: Vec<String>,
    pub name: String,
    pub fields: Vec<Field>,
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
    /// `for x in xs` — the shape all four languages share. A C-style `for` is not
    /// this, and is carried as unsupported.
    ForEach {
        binding: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Expr(Expr),
    Break,
    Continue,
    Unsupported(Unsupported),
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
    /// `[a, b, c]`
    ListLit(Vec<Expr>),
    Unsupported(Unsupported),
}

/// The operators that mean the same thing in all four languages.
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
    /// One line per thing that did not translate, with where it was.
    pub notes: Vec<String>,
}

impl Fidelity {
    pub fn is_complete(&self) -> bool {
        self.carried_verbatim == 0 && self.signatures_with_foreign_types == 0
    }
}
