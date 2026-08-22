//! What is known about a symbol's type: what the source declared, and what follows from what
//! the source declared.
//!
//! Two answers, kept apart. **Declared** is what somebody wrote down. **Inferred** is what this
//! module worked out, carrying the evidence that produced it: the literal, the class
//! constructed, the return type of the function called.
//!
//! Where the chain reaches outside the workspace, a library call, an unnamed object literal,
//! the answer is that nothing is known, distinct from `Any`.
//!
//! What counts as the type depends on the symbol. A binding has one. A callable has a
//! signature, so that is what this reports for one: the parameter types the source wrote. A
//! marker where it wrote none.

use crate::index::Index;
use crate::lang::Language;
use crate::model::{Symbol, SymbolId, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::Result;
use serde::Serialize;
use tree_sitter::Node;

/// Why an inferred type is believed.
///
/// Every inference is one short step from something the source stated, and the step is
/// named so a reader can follow it. A chain of these is a proof; a type with no basis
/// would be an assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Basis {
    /// The value is a literal: `5`, `"a"`, `True`, `[]`.
    Literal,
    /// A class in this workspace was constructed.
    Construction,
    /// A function in this workspace was called, and it declares what it returns.
    ReturnOfCall,
    /// The value is another binding whose type is known.
    SameBinding,
    /// A field of a record whose declaration names a type.
    FieldOfRecord,
    /// The binding is a loop variable, and the sequence it walks names what it holds.
    ElementOfIterable,
    /// The value is an arithmetic expression, and both its operands have the same type.
    BothOperands,
}

impl Basis {
    /// Prose for a reader, not an identifier. This carries a different name from the `as_str`
    /// several enums here use for their stable spelling. Conflating the two made `SymbolKind`
    /// print `"type"` in JSON and then refuse to read it back.
    pub fn describe(&self) -> &'static str {
        match self {
            Basis::Literal => "from the literal",
            Basis::Construction => "from the class constructed here",
            Basis::ReturnOfCall => "from the declared return type of the call",
            Basis::SameBinding => "from the binding it was assigned from",
            Basis::FieldOfRecord => "from the field's declaration",
            Basis::ElementOfIterable => "from the sequence's element type",
            Basis::BothOperands => "from the operands, which share a type",
        }
    }
}

/// A type this worked out, and the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inferred {
    pub ty: String,
    pub basis: Basis,
    /// The symbol the evidence came from, where the evidence is a symbol.
    pub from: Option<SymbolId>,
}

/// What is known about a symbol's type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Declared {
    /// The symbol asked about.
    pub symbol: SymbolId,
    pub name: String,
    /// The type as written, or `None` where the source wrote none.
    ///
    /// An `Option` instead of an empty string, because "no type here" is an answer and
    /// the caller has to be made to handle it.
    pub declared: Option<String>,
    /// What follows from what the source declared, where the source declared nothing.
    ///
    /// Only ever consulted when `declared` is `None`: an annotation is a contract and this is a
    /// derivation. Where both exist the contract is the answer. Where they disagree that is a
    /// defect in the code and not a choice for this to make.
    pub inferred: Option<Inferred>,
    /// For a callable, each parameter's declared type in order, `None` where absent.
    pub parameters: Vec<(String, Option<String>)>,
    /// Where the type itself is defined, when it names something in this workspace.
    ///
    /// A type that resolves nowhere is a type from outside the tree, `int`, `str`,
    /// `Promise`, and that is not a gap in the answer.
    pub defined_at: Option<SymbolId>,
}

impl Declared {
    /// The type, however it was arrived at.
    pub fn ty(&self) -> Option<&str> {
        self.declared
            .as_deref()
            .or(self.inferred.as_ref().map(|i| i.ty.as_str()))
    }

    /// How this reads in a sentence.
    pub fn describe(&self) -> String {
        match (&self.declared, &self.inferred) {
            (Some(ty), _) => ty.clone(),
            (None, Some(inferred)) => format!("{} ({})", inferred.ty, inferred.basis.describe()),
            (None, None) => "no type written down".to_string(),
        }
    }
}

/// What the source declared about `symbol`. Does this language have anywhere to write a type
/// down?
///
/// The question this answers is "what did the source say", so it is yes wherever a language has
/// a place to say it. Bash has no type syntax at all; markup and configuration have values and
/// not declarations. A key in a YAML file is not annotated with anything.
///
/// The list lived in the capability matrix and nowhere else. The matrix said `n/a` for nine
/// languages while [`of`] answered for all of them. Its empty answer means "the source wrote
/// nothing here", and that differs from "nowhere here to write".
pub fn supports_declared_type(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::Java
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
    )
}

pub fn of(index: &Index, symbol: SymbolId) -> Result<Declared> {
    if let Some(language) = index.symbol(symbol).map(|s| s.language) {
        crate::capabilities::record(crate::capabilities::Capability::DeclaredType, language);
        if !supports_declared_type(language) {
            return Err(crate::refactor::Refusal::Unsupported {
                operation: "reading a declared type".into(),
                language,
                because: "this language has nowhere to write a type down, so there is \
                          nothing here for the source to have said",
            }
            .into());
        }
    }
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("no symbol with that id"))?;
    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    let declared = match sym.kind.is_callable() {
        true => signature(&parsed, &source, sym),
        // `var` and `auto` are the keyword for "not stated". So a binding written with one has
        // no declared type and falls through to what can be worked out. Reporting the keyword
        // answered the question with the question.
        false => binding_type(&parsed, &source, sym.full_span)
            .filter(|written| !crate::parse::is_an_inferred_type(written)),
    };
    let parameters = match sym.kind.is_callable() {
        true => parameters_of(&parsed, &source, sym),
        false => Vec::new(),
    };
    // The named type, where the answer is one name and not a signature.
    let named = declared.as_deref().and_then(bare_name);
    let defined_at = named.and_then(|name| type_named(index, name, sym));

    // Only where the source said nothing. An annotation is a contract and an inference is a
    // derivation; where both exist the contract is the answer. A disagreement between them is a
    // defect in the code and not a choice for this to make.
    let inferred = match (&declared, sym.kind.is_callable()) {
        (None, false) => infer(index, sym, &parsed, &source, 0),
        _ => None,
    };

    Ok(Declared {
        symbol,
        name: sym.name.clone(),
        declared,
        inferred,
        parameters,
        defined_at,
    })
}

/// What a name holds where it is read, over every assignment to it in its scope.
///
/// [`of`] answers about one declaration, which is the right answer to its own question.
/// A use site asks a different one. `b = B()` above `b = A()` puts two types
/// into one name. Either initializer states what that name holds on its own
/// line, and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// The source states this, and every assignment in scope agrees.
    Settled(String),
    /// The name is assigned more than once, and the assignments disagree.
    Reassigned,
    /// Nothing in scope writes down what the name holds.
    Unwritten,
}

/// What the source says this binding holds, over the whole scope.
pub fn held_by(index: &Index, symbol: SymbolId) -> Held {
    let Some(sym) = index.symbol(symbol) else {
        return Held::Unwritten;
    };
    let Ok(answer) = of(index, symbol) else {
        return Held::Unwritten;
    };
    // An annotation is a contract over the name, and a later assignment that
    // disagrees with it is a defect in the code. So it holds for the whole scope.
    if let Some(declared) = answer.declared {
        return Held::Settled(declared);
    }
    let Some(inferred) = answer.inferred else {
        return Held::Unwritten;
    };
    let Ok(source) = crate::vfs::read_to_string(&sym.file) else {
        return Held::Unwritten;
    };
    let scope = scope_span(index, sym);
    let Ok(parsed) = Parsers::new().parse(sym.language, &source) else {
        return Held::Unwritten;
    };
    let mut bound = Vec::new();
    collect_assignments(parsed.root(), sym, &source, scope, &mut bound);
    let types: Vec<Option<String>> = bound
        .iter()
        .map(|b| infer_bound(index, sym, &source, *b, 0).map(|i| i.ty))
        .collect();
    match types.split_first() {
        None | Some((_, [])) => Held::Settled(inferred.ty),
        Some((first, rest)) => match first {
            Some(ty) if rest.iter().all(|other| other.as_ref() == Some(ty)) => {
                Held::Settled(ty.clone())
            }
            _ => Held::Reassigned,
        },
    }
}

/// The span of the scope a symbol was declared in.
fn scope_span(index: &Index, sym: &Symbol) -> Span {
    index
        .file(&sym.file)
        .and_then(|info| info.scopes.iter().find(|s| s.id == sym.scope))
        .map(|s| s.span)
        .unwrap_or(sym.full_span)
}

/// Every assignment to this name inside the scope, in source order.
///
/// A nested function is not this scope: its `b` is another name that reads the same.
/// So the walk stops at one.
fn collect_assignments<'a>(
    node: Node<'a>,
    sym: &Symbol,
    source: &str,
    scope: Span,
    out: &mut Vec<Bound<'a>>,
) {
    let span = Span::from(node);
    if span.end <= scope.start || span.start >= scope.end {
        return;
    }
    let nested_body = span.start > scope.start
        && span.end <= scope.end
        && ["function", "class", "method", "lambda", "closure"]
            .iter()
            .any(|shape| node.kind().contains(shape));
    if nested_body {
        return;
    }
    if let Some(target) = assigned_name(node) {
        let target_span = Span::from(target);
        if target_span.text(source).trim() == sym.name {
            if let Some(bound) = bound_expression(sym.language, node, target_span) {
                out.push(bound);
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_assignments(child, sym, source, scope, out);
    }
}

/// The name a node assigns to, where it assigns to one name.
fn assigned_name<'a>(node: Node<'a>) -> Option<Node<'a>> {
    ["left", "name", "pattern"]
        .iter()
        .find_map(|field| node.child_by_field_name(field))
        .or_else(|| {
            node.child_by_field_name("declarator")
                .and_then(|d| d.child_by_field_name("name"))
        })
}

/// How far a chain of derivations is followed.
///
/// A binding assigned from a binding assigned from a call is three steps and readable. Beyond
/// that, a reader can no longer check the answer at a glance. This function gives no other
/// kind.
const MAX_CHAIN: usize = 4;

/// What follows from what the source declared, for a binding it left unannotated.
fn infer(
    index: &Index,
    sym: &Symbol,
    parsed: &Parsed,
    source: &str,
    depth: usize,
) -> Option<Inferred> {
    if depth >= MAX_CHAIN {
        return None;
    }
    let bound = assigned_value(parsed, sym.language, sym.full_span, sym.name_span)?;
    infer_bound(index, sym, source, bound, depth)
}

/// What a declaration binds: a value, or one element of a sequence.
///
/// `for box in boxes` binds `box` to an element, and the expression the grammar hands
/// over is the whole sequence. Reading that expression as the value gave `box` the type
/// `list`, and a member call on it was then attributed to the container.
#[derive(Debug, Clone, Copy)]
enum Bound<'a> {
    Value(Node<'a>),
    Element(Node<'a>),
}

/// The type of whatever a declaration bound.
fn infer_bound(
    index: &Index,
    sym: &Symbol,
    source: &str,
    bound: Bound<'_>,
    depth: usize,
) -> Option<Inferred> {
    match bound {
        Bound::Value(node) => infer_expression(index, sym, source, node, depth),
        Bound::Element(node) => {
            let sequence = infer_expression(index, sym, source, node, depth)?;
            // A sequence whose element type is not written down says nothing about
            // the loop variable. Its own name is the container's and never the
            // element's.
            let ty = element_type(&sequence.ty)?;
            Some(Inferred {
                ty,
                basis: Basis::ElementOfIterable,
                from: sequence.from,
            })
        }
    }
}

/// The expression a definition binds, where the grammar names one.
///
/// Read the declaration itself first, through the one reader that knows every grammar's
/// shape, and only then look outwards. A Python `x: int = 1` hangs the value off the
/// assignment rather than off `x`. So the name alone is not always the declaration.
fn assigned_value<'a>(
    parsed: &'a Parsed,
    language: Language,
    declaration: Span,
    name: Span,
) -> Option<Bound<'a>> {
    let node = parsed
        .root()
        .descendant_for_byte_range(declaration.start, declaration.end)?;
    let mut current = Some(node);
    for _ in 0..4 {
        let here = current?;
        if let Some(bound) = bound_expression(language, here, name) {
            let node = match bound {
                Bound::Value(node) | Bound::Element(node) => node,
            };
            if Span::from(node) != name {
                return Some(bound);
            }
        }
        current = here.parent();
    }
    None
}

/// What this node binds, for a node that binds anything.
///
/// A loop over a sequence is answered as an element, and only where the loop binds
/// this one name whole. `for k, v in pairs` takes the pair apart, and which piece
/// each name gets is a question this does not answer.
fn bound_expression<'a>(language: Language, node: Node<'a>, name: Span) -> Option<Bound<'a>> {
    if let Some(sequence) = iterated_sequence(language, node) {
        let target = ["left", "name", "pattern"]
            .iter()
            .find_map(|field| node.child_by_field_name(field))?;
        return (Span::from(target) == name).then_some(Bound::Element(sequence));
    }
    crate::parse::declaration_value(node).map(Bound::Value)
}

/// The sequence a loop walks, for the loop forms that bind a name to each element.
///
/// By language, because one spelling is two statements. A TypeScript `for_statement`
/// is the three-part C loop, whose initializer binds a value and not an element.
fn iterated_sequence<'a>(language: Language, node: Node<'a>) -> Option<Node<'a>> {
    let kind = node.kind();
    let walks = match language {
        Language::Python => matches!(kind, "for_statement" | "for_in_clause"),
        Language::TypeScript | Language::Tsx => kind == "for_in_statement",
        Language::Java => kind == "enhanced_for_statement",
        Language::Go => kind == "range_clause",
        Language::Rust => kind == "for_expression",
        _ => false,
    };
    if !walks {
        return None;
    }
    ["right", "value"]
        .iter()
        .find_map(|field| node.child_by_field_name(field))
}

/// The names of the sequence types whose type argument is the element's.
///
/// A closed list, because being wrong here is not a missing answer: it hands a member
/// call the type of something else and rewrites it. A map's type argument is its value
/// and its iteration yields its keys, so no map is here.
const SEQUENCES: &[&str] = &[
    "list",
    "List",
    "Sequence",
    "Iterable",
    "Iterator",
    "Collection",
    "ArrayList",
    "LinkedList",
    "set",
    "Set",
    "HashSet",
    "TreeSet",
    "BTreeSet",
    "frozenset",
    "FrozenSet",
    "Array",
    "ReadonlyArray",
    "Vec",
    "VecDeque",
];

/// What one element of this sequence type is, where the type names it.
fn element_type(written: &str) -> Option<String> {
    let text = written.trim();
    // Go's slice and array write the element last, `[]Box` and `[3]Box`; Rust's
    // slice writes it inside, `[Box]`.
    if let Some(rest) = text.strip_prefix('[') {
        let (inside, after) = rest.rsplit_once(']')?;
        let element = match after.trim().is_empty() {
            true => inside,
            false => after,
        };
        return bare_name(element).map(str::to_string);
    }
    // TypeScript writes it first: `Box[]`.
    if let Some(element) = text.strip_suffix("[]") {
        return bare_name(element).map(str::to_string);
    }
    // Rust's slice arrives behind a borrow: `&[Box]`.
    if let Some(rest) = text.strip_prefix('&') {
        return element_type(rest.trim_start_matches("mut ").trim());
    }
    let at = text.find(['[', '<'])?;
    let container = last_segment(&text[..at]);
    let inner = text
        .strip_suffix(']')
        .or_else(|| text.strip_suffix('>'))?
        .get(at + 1..)?;
    if !SEQUENCES.contains(&container) || inner.contains(',') {
        return None;
    }
    bare_name(inner).map(str::to_string)
}

/// The type of an expression, where one follows from something the source stated.
fn infer_expression(
    index: &Index,
    from: &Symbol,
    source: &str,
    node: Node<'_>,
    depth: usize,
) -> Option<Inferred> {
    let language = from.language;
    let kind = node.kind();
    let text = Span::from(node).text(source).trim();

    // A literal states its own type, in every language that has literals.
    if let Some(ty) = literal_type(language, kind, text) {
        return Some(Inferred {
            ty,
            basis: Basis::Literal,
            from: None,
        });
    }

    match kind {
        // `avg * 2`, where both sides are the same type. Arithmetic in every language here
        // preserves that type, so the result is it. Where the two sides disagree, or either
        // is unknown, nothing follows and nothing is claimed.
        "binary_expression" | "binary_operator" => {
            let operator = node.child_by_field_name("operator")?;
            if !arithmetic_operator(Span::from(operator).text(source).trim()) {
                return None;
            }
            let left = node.child_by_field_name("left")?;
            let right = node.child_by_field_name("right")?;
            // The depth counts hops from binding to binding. Both operands belong to
            // this one expression, so the count does not grow here.
            let left = infer_expression(index, from, source, left, depth)?;
            let right = infer_expression(index, from, source, right, depth)?;
            (left.ty == right.ty).then_some(Inferred {
                ty: left.ty,
                basis: Basis::BothOperands,
                from: None,
            })
        }
        // `Money(0, USD)` in Python, `new Money(...)` in TypeScript. The callee decides which
        // of the two this is: a class is constructed, a function is called. Java spells a
        // call `method_invocation` and a construction `object_creation_expression`. The same
        // omission once made `fr signature` refuse at every Java call site.
        "call"
        | "call_expression"
        | "new_expression"
        | "method_invocation"
        | "object_creation_expression" => {
            // Each grammar names the callee differently: `function` in most, `constructor` for
            // a TypeScript `new`. In Java `name` for a call and `type` for a construction.
            let callee = ["function", "constructor", "name", "type"]
                .iter()
                .find_map(|field| node.child_by_field_name(field))?;
            let name = last_segment(Span::from(callee).text(source).trim());
            let target = resolve_in_workspace(index, from, name)?;
            let resolved = index.symbol(target)?;
            if is_type_like(resolved.kind) {
                return Some(Inferred {
                    ty: resolved.name.clone(),
                    basis: Basis::Construction,
                    from: Some(target),
                });
            }
            // A function states what it returns, or it does not and nothing follows.
            let returns = of(index, target).ok()?.parameters_free_return()?;
            Some(Inferred {
                ty: returns,
                basis: Basis::ReturnOfCall,
                from: Some(target),
            })
        }
        // `y = x`: whatever `x` is, and it is `x`'s declaration that says so.
        "identifier" => {
            let target = resolve_in_workspace(index, from, text)?;
            if target == from.id {
                return None;
            }
            let other = index.symbol(target)?;
            let answer = of(index, target).ok()?;
            let ty = answer.declared.or_else(|| {
                // Follow one more link, bounded, and only through the same file: a
                // chain that leaves the file is a chain a reader cannot see.
                (other.file == from.file)
                    .then(|| {
                        let other_source = crate::vfs::read_to_string(&other.file).ok()?;
                        let other_parsed =
                            Parsers::new().parse(other.language, &other_source).ok()?;
                        infer(index, other, &other_parsed, &other_source, depth + 1).map(|i| i.ty)
                    })
                    .flatten()
            })?;
            Some(Inferred {
                ty,
                basis: Basis::SameBinding,
                from: Some(target),
            })
        }
        // `payment.amount`, where the record declares what `amount` holds.
        "attribute" | "member_expression" | "field_expression" | "selector_expression" => {
            let field = node
                .child_by_field_name("attribute")
                .or_else(|| node.child_by_field_name("property"))
                .or_else(|| node.child_by_field_name("field"))?;
            let field_name = Span::from(field).text(source).trim();
            let target = index
                .symbols
                .iter()
                .find(|s| {
                    s.name == field_name
                        && s.language == language
                        && matches!(s.kind, SymbolKind::Field | SymbolKind::Property)
                })
                .map(|s| s.id)?;
            let ty = of(index, target).ok()?.declared?;
            Some(Inferred {
                ty,
                basis: Basis::FieldOfRecord,
                from: Some(target),
            })
        }
        _ => None,
    }
}

impl Declared {
    /// The return type alone, for a callable whose signature this rendered.
    fn parameters_free_return(&self) -> Option<String> {
        let declared = self.declared.as_deref()?;
        let at = declared.rfind("-> ")?;
        let returns = declared[at + 3..].trim();
        (!returns.is_empty() && returns != "?").then(|| returns.to_string())
    }
}

/// The type a literal states about itself.
///
/// An object literal is deliberately absent. `{"amount": 100}` is a `dict`, and saying so
/// tells the reader nothing. This whole analysis exists because a dictionary sits where a
/// type should have been. A tool that answers `dict` has agreed with the code and not
/// described it. A list literal is the same shape of non-answer.
///
/// Go and Java are here because each fixes the type at the declaration. `total := 0` is an
/// `int` and `var s = "a"` is a `String`, whatever the code does later. Rust is absent for the
/// opposite reason. There `let x = 0;` takes its type from a later use, so `i32` would be a
/// guess dressed as an answer. Zig's `0` is a `comptime_int`. No parameter can be written
/// with that.
fn literal_type(language: Language, kind: &str, text: &str) -> Option<String> {
    let python = matches!(language, Language::Python);
    let ts = matches!(language, Language::TypeScript | Language::Tsx);
    if let Some(fixed) = fixed_literal_type(language, kind) {
        return Some(fixed.to_string());
    }
    if !python && !ts {
        return None;
    }
    let answer = match kind {
        "integer" => "int",
        "float" => "float",
        "number" => {
            // TypeScript has one numeric type and calls it `number`.
            return Some("number".to_string());
        }
        "string" | "concatenated_string" | "template_string" => {
            if python {
                "str"
            } else {
                "string"
            }
        }
        "true" | "false" => {
            if python {
                "bool"
            } else {
                "boolean"
            }
        }
        "none" => "None",
        "null" => "null",
        "identifier" if python && matches!(text, "True" | "False") => "bool",
        "identifier" if python && text == "None" => "None",
        _ => return None,
    };
    Some(answer.to_string())
}

/// The type a Go or Java literal has by the language's own default rule.
fn fixed_literal_type(language: Language, kind: &str) -> Option<&'static str> {
    match language {
        Language::Go => match kind {
            "int_literal" => Some("int"),
            "float_literal" => Some("float64"),
            "interpreted_string_literal" | "raw_string_literal" => Some("string"),
            "rune_literal" => Some("rune"),
            "true" | "false" => Some("bool"),
            _ => None,
        },
        Language::Java => match kind {
            "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal" => {
                Some("int")
            }
            "decimal_floating_point_literal" => Some("double"),
            "string_literal" => Some("String"),
            "character_literal" => Some("char"),
            "true" | "false" => Some("boolean"),
            _ => None,
        },
        _ => None,
    }
}

/// The operators whose result has the type its two operands share.
///
/// Comparison and logical operators are absent: their result is a boolean and says nothing
/// about the operands. Reading `a < b` as an `int` is how an inference stops being one.
fn arithmetic_operator(operator: &str) -> bool {
    matches!(
        operator,
        "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>" | "&^"
    )
}

/// The last segment of a dotted or qualified name.
fn last_segment(text: &str) -> &str {
    text.rsplit(['.', ':']).next().unwrap_or(text).trim()
}

/// A symbol of this name that the workspace defines, in the asking symbol's language.
///
/// Same file first. A name that resolves to several things in one language is ambiguous, so
/// it resolves to none of them here. Every other lookup in this file gives the same reason.
/// An answer picked by indexing order is not an answer.
fn resolve_in_workspace(index: &Index, from: &Symbol, name: &str) -> Option<SymbolId> {
    let candidates: Vec<&Symbol> = index
        .symbols
        .iter()
        .filter(|s| s.name == name && s.language == from.language && s.id != from.id)
        .collect();
    if let Some(here) = candidates.iter().find(|s| s.file == from.file) {
        return Some(here.id);
    }
    match candidates.as_slice() {
        [only] => Some(only.id),
        _ => None,
    }
}

/// The definition of a type of this name, where one can be justified.
///
/// Same file first, then the same language. Never another language: a Python class
/// called `Money` and a TypeScript interface called `Money` are two types that share a
/// spelling, and the first written version of this pointed a TypeScript binding at the
/// Python one, a `find` over every symbol in the workspace, answering with whichever
/// happened to be indexed first.
///
/// Several in one language is ambiguous, and nothing is reported instead of picking.
/// A definition the reader is sent to is a claim, and a coin toss is not one.
fn type_named(index: &Index, name: &str, from: &Symbol) -> Option<SymbolId> {
    let candidates: Vec<&Symbol> = index
        .symbols
        .iter()
        .filter(|s| s.name == name && is_type_like(s.kind) && s.language == from.language)
        .collect();
    if let Some(here) = candidates.iter().find(|s| s.file == from.file) {
        return Some(here.id);
    }
    match candidates.as_slice() {
        [only] => Some(only.id),
        _ => None,
    }
}

/// Does this kind of symbol name a type?
fn is_type_like(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::TypeAlias | SymbolKind::Enum
    )
}

/// The outermost name in a type expression, where there is exactly one.
///
/// `PaymentId` names something. `list[PaymentId]`, `Money | None` and `(int, str) -> Money` do
/// not name *one* thing. Picking a piece of them would be answering a question nobody asked.
fn bare_name(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let plain = trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    if !plain || trimmed.is_empty() {
        return None;
    }
    // A dotted path names its last segment: `models.PaymentId` is `PaymentId`.
    trimmed.rsplit('.').next()
}

/// The type written on a binding: `x: int`, `let x: i64`, `var x int`.
fn binding_type(parsed: &Parsed, source: &str, declaration: Span) -> Option<String> {
    let node = parsed
        .root()
        .descendant_for_byte_range(declaration.start, declaration.end)?;
    // Look outwards, because the name is often a child of the node carrying the type. A
    // Python `x: int = 1` hangs the type off the assignment rather than off `x`.
    let mut current = Some(node);
    for _ in 0..4 {
        let here = current?;
        if let Some(ty) = here.child_by_field_name("type") {
            if let Some(text) = type_text(ty, source) {
                return Some(text);
            }
        }
        let parent = here.parent()?;
        // The walk stops at the construct that holds statements. A Zig `const width = 3;`
        // states no type. Climbing out of its block reached `fn run() void` and read
        // the function's return type as the binding's.
        if holds_statements(parent) {
            return None;
        }
        current = Some(parent);
    }
    None
}

/// Does this node hold statements, rather than being part of one declaration?
fn holds_statements(node: Node<'_>) -> bool {
    let kind = node.kind();
    [
        "block",
        "body",
        "function",
        "method",
        "class",
        "module",
        "source_file",
        "statement_list",
        "declaration_list",
    ]
    .iter()
    .any(|shape| kind.contains(shape))
}

/// A type node's text, with the punctuation that introduces it taken off.
pub(crate) fn type_text(node: Node<'_>, source: &str) -> Option<String> {
    let text = Span::from(node).text(source).trim();
    let bare = text
        .strip_prefix(':')
        .unwrap_or(text)
        .trim()
        .strip_prefix("->")
        .unwrap_or_else(|| text.strip_prefix(':').unwrap_or(text).trim())
        .trim();
    (!bare.is_empty()).then(|| bare.to_string())
}

/// A callable's signature, as the source wrote it.
fn signature(parsed: &Parsed, source: &str, sym: &Symbol) -> Option<String> {
    let parameters = parameters_of(parsed, source, sym);
    let returns = return_type(parsed, source, sym.full_span);
    if parameters.is_empty() && returns.is_none() {
        return None;
    }
    let rendered: Vec<String> = parameters
        .iter()
        .map(|(name, ty)| match ty {
            Some(ty) => format!("{name}: {ty}"),
            None => format!("{name}: ?"),
        })
        .collect();
    Some(format!(
        "({}) -> {}",
        rendered.join(", "),
        returns.unwrap_or_else(|| "?".to_string())
    ))
}

fn return_type(parsed: &Parsed, source: &str, declaration: Span) -> Option<String> {
    let node = parsed
        .root()
        .descendant_for_byte_range(declaration.start, declaration.end)?;
    for field in ["return_type", "result", "type"] {
        if let Some(ty) = node.child_by_field_name(field) {
            if let Some(text) = type_text(ty, source) {
                return Some(text);
            }
        }
    }
    None
}

/// Each parameter's name and declared type, in the order a caller must supply them.
fn parameters_of(parsed: &Parsed, source: &str, sym: &Symbol) -> Vec<(String, Option<String>)> {
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(sym.full_span.start, sym.full_span.end)
    else {
        return Vec::new();
    };
    let mut finder = node.walk();
    let named: Vec<Node<'_>> = node.named_children(&mut finder).collect();
    let Some(list) = node.child_by_field_name("parameters").or_else(|| {
        named
            .iter()
            .copied()
            .find(|c| c.kind().contains("parameter"))
    }) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut cursor = list.walk();
    for parameter in list.named_children(&mut cursor) {
        if parameter.kind().contains("comment") {
            continue;
        }
        let ty = parameter
            .child_by_field_name("type")
            .and_then(|t| type_text(t, source));
        let name = parameter
            .child_by_field_name("name")
            .or_else(|| parameter.child_by_field_name("pattern"))
            .map(|n| Span::from(n).text(source).to_string())
            .unwrap_or_else(|| {
                // A parameter the grammar does not break up is written whole; its name
                // is the first identifier in it.
                let text = Span::from(parameter).text(source);
                text.split([':', ' ']).next().unwrap_or(text).to_string()
            });
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        out.push((name, ty));
    }
    out
}
