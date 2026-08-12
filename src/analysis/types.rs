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
}

impl Basis {
    /// Prose for a reader, not an identifier. Named apart from the `as_str` that
    /// several enums here use for their stable spelling, because conflating the two is
    /// how `SymbolKind` came to print `"type"` in JSON and refuse to read it back.
    pub fn describe(&self) -> &'static str {
        match self {
            Basis::Literal => "from the literal",
            Basis::Construction => "from the class constructed here",
            Basis::ReturnOfCall => "from the declared return type of the call",
            Basis::SameBinding => "from the binding it was assigned from",
            Basis::FieldOfRecord => "from the field's declaration",
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
/// The list lived in the capability matrix and nowhere else. So the matrix said `n/a` for nine
/// languages while [`of`] answered for all of them, with the empty answer that means "the
/// source wrote nothing here", which is a different statement from "there is nowhere here to
/// write".
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

/// How far a chain of derivations is followed.
///
/// A binding assigned from a binding assigned from a call is three steps and readable.
/// Beyond that the answer stops being something a reader can check at a glance, which is
/// the only kind of answer this is willing to give.
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
    let value = assigned_value(parsed, sym.full_span, sym.name_span)?;
    infer_expression(index, sym, source, value, depth)
}

/// The expression a definition binds, where the grammar names one.
///
/// The declaration itself first, through the one reader that knows every grammar's shape, and
/// only then outwards: a Python `x: int = 1` hangs the value off the assignment and not off
/// `x`. So the name alone is not always the declaration.
fn assigned_value<'a>(parsed: &'a Parsed, declaration: Span, name: Span) -> Option<Node<'a>> {
    let node = parsed
        .root()
        .descendant_for_byte_range(declaration.start, declaration.end)?;
    let mut current = Some(node);
    for _ in 0..4 {
        let here = current?;
        if let Some(value) = crate::parse::declaration_value(here) {
            if Span::from(value) != name {
                return Some(value);
            }
        }
        current = here.parent();
    }
    None
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
        // `Money(0, USD)` in Python, `new Money(...)` in TypeScript. The callee decides
        // which of the two this is: a class is constructed, a function is called.
        // Java spells a call `method_invocation` and a construction
        // `object_creation_expression`, which is the same omission that once made
        // `fr signature` refuse at every Java call site there has ever been.
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
/// An object literal is deliberately absent. `{"amount": 100}` is a `dict` and saying so is
/// true and useless, the whole subject of this is that a dictionary is where a type should have
/// been. A tool that answers `dict` has agreed with the code and not described it. A list
/// literal is the same shape of non-answer.
fn literal_type(language: Language, kind: &str, text: &str) -> Option<String> {
    let python = matches!(language, Language::Python);
    let ts = matches!(language, Language::TypeScript | Language::Tsx);
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

/// The last segment of a dotted or qualified name.
fn last_segment(text: &str) -> &str {
    text.rsplit(['.', ':']).next().unwrap_or(text).trim()
}

/// A symbol of this name that the workspace defines, in the asking symbol's language.
///
/// Same file first. A name that resolves to several things in one language is ambiguous and
/// resolves to none of them here, for the reason every other lookup in this file gives. An
/// answer picked by indexing order is not an answer.
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
    // Outwards, because the name is often a child of the node carrying the type: a
    // Python `x: int = 1` hangs the type off the assignment, not off `x`.
    let mut current = Some(node);
    for _ in 0..4 {
        let here = current?;
        if let Some(ty) = here.child_by_field_name("type") {
            if let Some(text) = type_text(ty, source) {
                return Some(text);
            }
        }
        current = here.parent();
    }
    None
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
