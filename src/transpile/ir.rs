//! What a file means, said in a way no one language owns.

use std::fmt;

/// One translated file.
#[derive(Debug, Default, Clone)]
pub struct Module {
    /// The file-level doc comment, where the language has one.
    pub doc: Vec<String>,
    /// The file's own name, where a language needs it.
    pub name: Option<String>,
    pub items: Vec<Item>,
    /// What a directory sweep had to change about this file, for its header.
    pub sweep_notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    Record(Record),
    Constant(Constant),
    Newtype(Newtype),
    Sum(Sum),
    /// An import.
    Import {
        text: String,
        line: usize,
        /// What the line says, taken apart, when the reader could take it apart.
        target: Option<ImportTarget>,
    },
    /// A named test, declared in the source file beside the code it checks.
    Test {
        doc: Vec<String>,
        name: String,
        body: Vec<Stmt>,
    },
    /// A statement at the top of the file: `main();`, the program's own entry.
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
    pub receiver_binding: Option<String>,
    pub params: Vec<Param>,
    pub returns: Option<Type>,
    pub body: Vec<Stmt>,
    pub exported: bool,
    /// Report rather than translate. Python says so, and Go cannot say it.
    pub is_async: bool,
    /// Is this method read as data at its use sites?
    pub is_property: bool,
    /// Does this function make a value of its type?
    pub is_constructor: bool,
    /// Did the source say `private` in so many words?
    pub is_private: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
    pub kind: ParamKind,
}

/// How a parameter arrives, which belongs to the signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamKind {
    #[default]
    Normal,
    /// `*args`, the rest, positionally.
    VarArgs,
    /// `**kwargs`, the rest, by name.
    KeywordArgs,
    /// A bare `*` or `/`, a rule about the parameters around it.
    Marker,
}

/// A struct, class, dataclass or interface, a named product of fields.
#[derive(Debug, Clone)]
pub struct Record {
    pub doc: Vec<String>,
    pub name: String,
    pub fields: Vec<Field>,
    /// The type it inherits from, where the source has inheritance.
    pub extends: Option<String>,
    pub exported: bool,
    /// Methods declared on it.
    pub methods: Vec<Function>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub doc: Vec<String>,
    pub name: String,
    pub ty: Option<Type>,
    /// The value the field starts with, where the source gave one.
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
#[derive(Debug, Clone)]
pub struct Newtype {
    pub doc: Vec<String>,
    pub name: String,
    pub base: Type,
    pub exported: bool,
}

/// A closed choice: a value is exactly one of the named variants, and each variant may carry
/// its own fields.
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
    /// The discriminator literal the source wrote, where the language writes one: `kind:
    /// "idle"` on an interface named `FIdle`.
    pub tag: Option<String>,
    /// Empty for a bare tag like `None` or `Empty`.
    pub fields: Vec<Field>,
}

/// An import taken apart: where it points and which names it binds.
#[derive(Debug, Clone)]
pub struct ImportTarget {
    /// The module path as the source wrote it: `helpers`, `.models`, `./m`.
    pub module: String,
    /// Whether the path is relative, a leading dot in Python or `./` here.
    pub relative: bool,
    /// The named bindings, each with the alias the body uses, where it has one.
    pub names: Vec<ImportedName>,
    /// The sibling file stem this import points at, when it points inside a sweep.
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

/// A type, as far as one crosses between languages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Float,
    String,
    List(Box<Type>),
    /// `set[str]`, `HashSet<String>`, `Set<string>`: membership without order.
    Set(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Optional(Box<Type>),
    /// `(int, error)`, `tuple[int, str]`, `[number, string]`: several types as one.
    Tuple(Vec<Type>),
    /// A type the reader recognised the shape of but not the meaning.
    Named {
        name: String,
        args: Vec<Type>,
    },
    /// `(n: number) => number`, `func(int) int`, `Callable[[int], int]`.
    Fn {
        params: Vec<Type>,
        returns: Box<Type>,
    },
}

impl Expr {
    /// Can this stand on the left of `=` in any of these languages?
    pub fn is_assignable(&self) -> bool {
        match self {
            Expr::Name(_) => true,
            Expr::Field { of, .. } | Expr::Index { of, .. } => of.is_assignable(),
            _ => false,
        }
    }
}

impl Type {
    /// A named type with no arguments, the common case.
    pub fn named(name: impl Into<String>) -> Type {
        Type::Named {
            name: name.into(),
            args: Vec::new(),
        }
    }

    /// Can any of these languages spell this name as a type?
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
            Type::Set(inner) => write!(f, "set<{inner}>"),
            Type::Map(k, v) => write!(f, "map<{k}, {v}>"),
            Type::Optional(inner) => write!(f, "optional<{inner}>"),
            Type::Tuple(parts) => {
                let rendered: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
                write!(f, "tuple<{}>", rendered.join(", "))
            }
            Type::Fn { params, returns } => {
                let rendered: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "fn<({}) -> {returns}>", rendered.join(", "))
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
    /// A new binding.
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
    /// `a, b = b, a`, `x, err := f()`: several names settled at once.
    TupleAssign {
        /// The names on the left, in order.
        names: Vec<String>,
        value: Expr,
        declares: bool,
        /// The original, for the two writers with no form for this.
        source: String,
        line: usize,
    },
    If {
        condition: Expr,
        then: Vec<Stmt>,
        otherwise: Vec<Stmt>,
    },
    /// `if let Some(x) = e`, `if (e) |x|`: test an optional and bind its payload.
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
    /// `for i := 0; i < n; i++`: a header that starts a counter, tests it before each pass and
    /// steps it after one.
    CountedFor {
        /// What runs once before the first test.
        init: Option<Box<Stmt>>,
        /// Tested before each pass.
        condition: Option<Expr>,
        /// What runs after each pass, before the next test.
        update: Option<Box<Stmt>>,
        body: Vec<Stmt>,
        /// The original, for a writer that cannot spell this loop.
        source: String,
        line: usize,
    },
    /// `for i, x in enumerate(xs)`, `for (xs, 0..) |x, i|`: each element beside its position,
    /// counted from zero.
    ForEachIndexed {
        index: String,
        binding: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    /// Run a body when the enclosing scope exits: Go's and Zig's `defer`.
    Defer(Vec<Stmt>),
    /// Zig's `errdefer`: run only where the scope exits on the failure path.
    ErrDefer(Vec<Stmt>),
    /// One value branched against literal alternatives.
    Switch {
        subject: Expr,
        /// Each arm: the literals that select it, and its body.
        arms: Vec<(Vec<Expr>, Vec<Stmt>)>,
        /// The `_` / `else` / `default` body; empty when the source had none.
        default: Vec<Stmt>,
    },
    /// One sum value branched by variant, each arm's payload bound by name.
    MatchVariants {
        subject: Expr,
        sum: String,
        arms: Vec<VariantArm>,
        /// The `else` / `default` body; empty when the source had none.
        default: Vec<Stmt>,
    },
    /// `while let Some(x) = e`, `while (e) |x|`: loop while the optional holds a payload,
    /// re-evaluating it each pass.
    WhilePresent {
        binding: String,
        value: Expr,
        body: Vec<Stmt>,
    },
    /// `for x in xs`, the shape every language here shares.
    ForEach {
        binding: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Expr(Expr),
    /// `assert c, "m"`: check a condition and stop the program when it fails.
    Assert {
        condition: Expr,
        /// The words the failure prints, where the source gave any.
        message: Option<Expr>,
    },
    /// A comment on its own line.
    Comment(String),
    /// A function declared inside another: Zig's `const f = struct { fn f… }.f;` idiom,
    /// Python's nested `def`.
    LocalFunction(Box<Function>),
    /// A braced block: its statements, scoped where the target scopes blocks.
    Block(Vec<Stmt>),
    /// `raise e` / `throw e`.
    Throw(Expr),
    /// `try { } catch { } finally { }`, and Python's `try/except/finally`.
    Try {
        body: Vec<Stmt>,
        catches: Vec<Catch>,
        finally: Vec<Stmt>,
        /// The original, for the two writers that have no counterpart for this.
        source: String,
        line: usize,
    },
    Break,
    /// `break :label value`: leave the labeled block, with a value when the block produces one.
    BreakWith {
        label: String,
        value: Option<Box<Expr>>,
    },
    Continue,
    Unsupported(Unsupported),
}

/// One `except` or `catch` clause.
#[derive(Debug, Clone)]
pub struct Catch {
    /// The name holding the error, where the source gives one.
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
    Await(Box<Expr>),
    /// Evaluate, and on failure leave the function with the failure: Rust's `x?`, Zig's `try
    /// x`.
    Propagate(Box<Expr>),
    /// `name=value` in an argument list.
    Keyword {
        name: String,
        value: Box<Expr>,
    },
    /// `(T) x`, `x as T`, `@as(T, x)`: the value reasserted as a type.
    Cast {
        ty: Box<Expr>,
        value: Box<Expr>,
    },
    /// `x instanceof T`, `isinstance(x, T)`.
    InstanceOf {
        value: Box<Expr>,
        ty: Box<Expr>,
    },
    /// `new Thing(a, b)`.
    New {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `Counter { value: 0, step }`, a record built by naming its fields.
    RecordLit {
        ty: String,
        fields: Vec<(String, Expr)>,
    },
    /// `a ??
    Coalesce {
        value: Box<Expr>,
        fallback: Box<Expr>,
    },
    /// `a ?
    Ternary {
        condition: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// One variant of a closed choice, made: `Shape::Circle { radius }`, `.{ .one = n }`, `{
    /// kind: "circle", radius }`.
    Variant {
        sum: String,
        name: String,
        fields: Vec<(String, Expr)>,
    },
    /// `(a, b)`: several values travelling as one, without a name for the whole.
    Tuple(Vec<Expr>),
    /// `[a, b, c]`
    ListLit(Vec<Expr>),
    /// `{"a": 1}` in Python, `{ a: 1 }` in TypeScript, a map literal in Go.
    MapLit(Vec<(Expr, Expr)>),
    /// An interpolated string: `f"Hi {name}"`, `` `Hi ${name}` ``.
    Template(Vec<TemplatePart>),
    /// `lambda x: e`, `(x) => e`, `|x| e`: a nameless function of one expression.
    Lambda {
        /// The same [`Param`] a declaration uses, so a lambda whose parameters the source typed
        /// keeps those types.
        params: Vec<Param>,
        /// What it answers, where the source said.
        returns: Option<Type>,
        body: Box<Expr>,
    },
    /// `{a, b}`, `set()`, `new Set()`, `HashSet::new()`: a set built in place.
    SetLit(Vec<Expr>),
    /// `[f(x) for x in xs if p(x)]`, and `xs.filter(p).map(f)`.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    /// `a // b`: division that rounds toward negative infinity.
    FloorDiv,
    /// `a / b` in Python: division that yields a float whatever the operands are.
    TrueDiv,
    Rem,
    /// `%` in Python: the remainder that goes with division rounding toward negative infinity.
    FloorRem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    /// `a ^ b`, the exclusive or, which every target spells the same way.
    Xor,
}

impl BinaryOp {
    /// The spelling shared by Rust, Go and TypeScript.
    pub fn c_like(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            // No C-family language spells floor division as an operator.
            BinaryOp::FloorDiv => unreachable!("floor division has no shared operator spelling"),
            // Only the languages whose `/` is already a float division reach this.
            BinaryOp::TrueDiv => "/",
            BinaryOp::Rem => "%",
            // Only Python's `%` floors, and Python is not a C-family language.
            BinaryOp::FloorRem => {
                unreachable!("a floor remainder has no shared operator spelling")
            }
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::Xor => "^",
        }
    }

    pub fn python(self) -> &'static str {
        match self {
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
            BinaryOp::FloorDiv => "//",
            // The remainder Python's `%` already gives.
            BinaryOp::FloorRem => "%",
            other => other.c_like(),
        }
    }

    /// How tightly this operator binds.
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::FloorDiv
            | BinaryOp::TrueDiv
            | BinaryOp::Rem
            | BinaryOp::FloorRem => 6,
            BinaryOp::Add | BinaryOp::Sub => 5,
            // C gives xor its own tier between arithmetic and comparison;
            // parenthesised operands keep every target agreeing.
            BinaryOp::Xor => 4,
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
    /// Zig's `x.?`, TypeScript's `x!`: the value is there, and saying so is an assertion.
    Unwrap,
}

/// What a translation managed and what it did not.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Fidelity {
    pub functions: usize,
    pub records: usize,
    pub constants: usize,
    /// Signatures carried across with every parameter and the return type intact.
    pub signatures_complete: usize,
    /// Signatures naming a type with no counterpart, carried through by name.
    pub signatures_with_foreign_types: usize,
    /// Statements and expressions carried verbatim because nothing corresponds.
    pub carried_verbatim: usize,
    /// Imports listed and not translated.
    pub imports_listed: usize,
    /// Distinct types carried across: a `NewType`, a brand.
    pub newtypes: usize,
    /// Closed choices carried across: an enum with payloads, a tagged union, a
    /// discriminated union.
    pub sums: usize,
    /// Signatures with a parameter or a return the source never typed.
    pub signatures_untyped: usize,
    /// Signatures whose *types* carried but whose calling convention did not: a keyword-only
    /// marker, `*args` or `**kwargs` with no counterpart in the target.
    pub signatures_with_changed_calls: usize,
    /// One line per thing that did not translate, with where it was.
    pub notes: Vec<String>,
}

impl Fidelity {
    /// Did everything cross with a defined lowering?
    pub fn is_complete(&self) -> bool {
        self.translated() > 0 && self.carried_verbatim == 0
    }

    /// How many declarations came across at all.
    pub fn translated(&self) -> usize {
        self.functions + self.records + self.constants + self.newtypes + self.sums
    }
}
