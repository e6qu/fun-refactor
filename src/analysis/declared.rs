//! The type a symbol was declared with, as the source wrote it.
//!
//! Nothing here is inferred. `x = 5` has no declared type and this says so, rather than
//! answering `int` — because the two are different claims, and the difference is the
//! whole subject of adding annotations to a codebase that has none. A tool that quietly
//! fills the gap in cannot show you the gap closing.
//!
//! What counts as "the type" differs by what the symbol is. A binding has one. A
//! callable has a *signature*, which is what a caller has to satisfy, so that is what is
//! reported for one — with the parameter types the source wrote and a marker where it
//! wrote none.

use crate::index::Index;
use crate::model::{Symbol, SymbolId, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::Result;
use serde::Serialize;
use tree_sitter::Node;

/// What the source says about a symbol's type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Declared {
    /// The symbol asked about.
    pub symbol: SymbolId,
    pub name: String,
    /// The type as written, or `None` where the source wrote none.
    ///
    /// An `Option` rather than an empty string, because "no type here" is an answer and
    /// the caller has to be made to handle it.
    pub declared: Option<String>,
    /// For a callable, each parameter's declared type in order, `None` where absent.
    pub parameters: Vec<(String, Option<String>)>,
    /// Where the type itself is defined, when it names something in this workspace.
    ///
    /// A type that resolves nowhere is a type from outside the tree — `int`, `str`,
    /// `Promise` — and that is not a gap in the answer.
    pub defined_at: Option<SymbolId>,
}

impl Declared {
    /// How this reads in a sentence.
    pub fn describe(&self) -> String {
        match &self.declared {
            Some(ty) => ty.clone(),
            None => "no type written down".to_string(),
        }
    }
}

/// What the source declared about `symbol`.
pub fn of(index: &Index, symbol: SymbolId) -> Result<Declared> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("no symbol with that id"))?;
    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    let declared = match sym.kind.is_callable() {
        true => signature(&parsed, &source, sym),
        false => binding_type(&parsed, &source, sym.full_span),
    };
    let parameters = match sym.kind.is_callable() {
        true => parameters_of(&parsed, &source, sym),
        false => Vec::new(),
    };
    // The named type, where the answer is one name rather than a signature.
    let named = declared.as_deref().and_then(bare_name);
    let defined_at = named.and_then(|name| type_named(index, name, sym));

    Ok(Declared {
        symbol,
        name: sym.name.clone(),
        declared,
        parameters,
        defined_at,
    })
}

/// The definition of a type of this name, where one can be justified.
///
/// Same file first, then the same language. Never another language: a Python class
/// called `Money` and a TypeScript interface called `Money` are two types that share a
/// spelling, and the first written version of this pointed a TypeScript binding at the
/// Python one — a `find` over every symbol in the workspace, answering with whichever
/// happened to be indexed first.
///
/// Several in one language is ambiguous, and nothing is reported rather than picking.
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
/// `PaymentId` names something. `list[PaymentId]`, `Money | None` and
/// `(int, str) -> Money` do not name *one* thing, and picking a piece of them would be
/// answering a question nobody asked.
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
fn type_text(node: Node<'_>, source: &str) -> Option<String> {
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
