//! What is known about a symbol's type: what the source declared, and what follows from what
//! the source declared.

use crate::index::Index;
use crate::lang::Language;
use crate::model::{Symbol, SymbolId, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::Result;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tree_sitter::Node;

/// One entry of the parse cache: a file's text and its tree, shared.
type SharedParse = Rc<(String, Parsed)>;

thread_local! {
    /// One parse per file per index generation per thread.
    static PARSES: RefCell<HashMap<(u64, PathBuf), SharedParse>> =
        RefCell::new(HashMap::new());
    /// [`held_by`]'s answers, for the same reason.
    static HELD: RefCell<HashMap<(u64, SymbolId), Held>> = RefCell::new(HashMap::new());
}

/// The file's source and tree as this index read them, from the cache or parsed into it.
fn parsed_source(index: &Index, file: &Path, language: Language) -> Option<SharedParse> {
    let key = (index.generation, file.to_path_buf());
    PARSES.with(|cache| {
        if let Some(hit) = cache.borrow().get(&key) {
            return Some(hit.clone());
        }
        let source = crate::vfs::read_to_string(file).ok()?;
        let parsed = Parsers::new().parse(language, &source).ok()?;
        let entry = Rc::new((source, parsed));
        let mut cache = cache.borrow_mut();
        // A long session builds many indexes, and every generation has its own entries.
        if cache.len() >= 512 {
            cache.clear();
        }
        cache.insert(key, entry.clone());
        Some(entry)
    })
}

/// Why an inferred type is believed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Basis {
    /// The value is a literal: `5`, `"a"`, `True`, `[]`.
    Literal,
    /// A class in this workspace was constructed.
    Construction,
    /// A call reaches a function in this workspace, and it declares what it answers.
    ReturnOfCall,
    /// The value is another binding whose type is known.
    SameBinding,
    /// A field of a record whose declaration names a type.
    FieldOfRecord,
    /// The binding is a loop variable, and the sequence it walks names what it holds.
    ElementOfIterable,
    /// The value is an arithmetic expression, and both its operands have the same type.
    BothOperands,
    /// The value is `self` or `this`, and the enclosing declaration names its type.
    EnclosingType,
    /// The value picks one of two branches, and both branches have the same type.
    AgreeingBranches,
}

impl Basis {
    /// Prose for a reader, not an identifier.
    pub fn describe(&self) -> &'static str {
        match self {
            Basis::Literal => "from the literal",
            Basis::Construction => "from the class constructed here",
            Basis::ReturnOfCall => "from the declared return type of the call",
            Basis::SameBinding => "from the binding it was assigned from",
            Basis::FieldOfRecord => "from the field's declaration",
            Basis::ElementOfIterable => "from the sequence's element type",
            Basis::BothOperands => "from the operands, which share a type",
            Basis::EnclosingType => "from the declaration enclosing self",
            Basis::AgreeingBranches => "from the branches, which share a type",
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
    pub declared: Option<String>,
    /// What follows from what the source declared, where the source declared nothing.
    pub inferred: Option<Inferred>,
    /// For a callable, each parameter's declared type in order, `None` where absent.
    pub parameters: Vec<(String, Option<String>)>,
    /// Where the type itself is defined, when it names something in this workspace.
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

/// What the source declared about `symbol`.
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
    of_at(index, symbol, 0)
}

/// [`of`], from partway along a derivation chain.
fn of_at(index: &Index, symbol: SymbolId, depth: usize) -> Result<Declared> {
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
    let shared = parsed_source(index, &sym.file, sym.language)
        .ok_or_else(|| anyhow::anyhow!("the file would not read or parse"))?;
    let (source, parsed) = (&shared.0, &shared.1);

    let declared = match sym.kind.is_callable() {
        true => signature(parsed, source, sym),
        // `var` and `auto` are the keyword for "not stated".
        false => binding_type(parsed, source, sym.full_span)
            .filter(|written| !crate::parse::is_an_inferred_type(written)),
    };
    let parameters = match sym.kind.is_callable() {
        true => parameters_of(parsed, source, sym),
        false => Vec::new(),
    };
    // The named type, where the answer is one name and not a signature.
    let named = declared.as_deref().and_then(bare_name);
    let defined_at = named.and_then(|name| type_named(index, name, sym));

    // Only where the source said nothing.
    let inferred = match (&declared, sym.kind.is_callable()) {
        (None, false) => infer(index, sym, parsed, source, depth),
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

/// What a name holds at its uses, over every assignment in its scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// The source states this, and every assignment in scope agrees.
    Settled(String),
    /// More than one assignment writes the name, and they disagree.
    Reassigned,
    /// Nothing in scope writes down what the name holds.
    Unwritten,
}

/// What the source says this binding holds, over the whole scope.
pub fn held_by(index: &Index, symbol: SymbolId) -> Held {
    let key = (index.generation, symbol);
    if let Some(hit) = HELD.with(|cache| cache.borrow().get(&key).cloned()) {
        return hit;
    }
    let answer = held_by_uncached(index, symbol);
    HELD.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(key, answer.clone());
    });
    answer
}

fn held_by_uncached(index: &Index, symbol: SymbolId) -> Held {
    let Some(sym) = index.symbol(symbol) else {
        return Held::Unwritten;
    };
    let Ok(answer) = of(index, symbol) else {
        return Held::Unwritten;
    };
    // An annotation is a contract over the name, and a later assignment that disagrees with it
    // is a defect in the code.
    if let Some(declared) = answer.declared {
        return Held::Settled(declared);
    }
    let Some(inferred) = answer.inferred else {
        return Held::Unwritten;
    };
    let Some(shared) = parsed_source(index, &sym.file, sym.language) else {
        return Held::Unwritten;
    };
    let (source, parsed) = (&shared.0, &shared.1);
    let scope = scope_span(index, sym);
    let mut bound = Vec::new();
    collect_assignments(parsed.root(), sym, source, scope, &mut bound);
    let types: Vec<Option<String>> = bound
        .iter()
        .map(|b| infer_bound(index, sym, source, *b, 0).map(|i| i.ty))
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

/// The span of the scope holding this symbol's declaration.
fn scope_span(index: &Index, sym: &Symbol) -> Span {
    index
        .file(&sym.file)
        .and_then(|info| info.scopes.iter().find(|s| s.id == sym.scope))
        .map(|s| s.span)
        .unwrap_or(sym.full_span)
}

/// Every assignment to this name inside the scope, in source order.
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

/// How far this follows a chain of derivations.
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
            // Zig's `0..` capture counts as it goes, and the counter is a `usize` by
            // the language's own rule.
            if sym.language == Language::Zig && node.kind() == "range_expression" {
                return Some(Inferred {
                    ty: "usize".to_string(),
                    basis: Basis::ElementOfIterable,
                    from: None,
                });
            }
            let sequence = infer_expression(index, sym, source, node, depth)?;
            // A sequence declaring no element type says nothing about the loop
            // variable.
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
fn bound_expression<'a>(language: Language, node: Node<'a>, name: Span) -> Option<Bound<'a>> {
    // Zig binds loop names in a payload, `for (xs, ys) |x, y|`, one name per sequence
    // in order, so the name's own position picks its sequence.
    if language == Language::Zig && node.kind() == "for_statement" {
        return zig_for_element(node, name);
    }
    if let Some(sequence) = iterated_sequence(language, node) {
        let target = ["left", "name", "pattern"]
            .iter()
            .find_map(|field| node.child_by_field_name(field))?;
        return (Span::from(target) == name).then_some(Bound::Element(sequence));
    }
    crate::parse::declaration_value(node).map(Bound::Value)
}

/// The sequence a Zig `for` walks with this name, matched by position in the payload.
fn zig_for_element<'a>(node: Node<'a>, name: Span) -> Option<Bound<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
    let payload = children.iter().find(|c| c.kind() == "payload")?;
    let sequences: Vec<Node<'a>> = children
        .iter()
        .take_while(|c| c.kind() != "payload")
        .copied()
        .collect();
    let mut inside = payload.walk();
    let bindings: Vec<Node<'a>> = payload
        .named_children(&mut inside)
        .filter(|c| c.kind() == "identifier")
        .collect();
    let at = bindings.iter().position(|b| Span::from(*b) == name)?;
    (sequences.len() == bindings.len())
        .then(|| sequences.get(at).copied())
        .flatten()
        .map(Bound::Element)
}

/// The sequence a loop walks, for the loop forms that bind a name to each element.
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
        // Zig writes the pointer's constness before the element, `[]const u8`; the
        // element is `u8` and the `const` binds to the slice.
        let element = element.trim();
        let element = element.strip_prefix("const ").unwrap_or(element);
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

    // `self` and `this` are the one value whose type is never a guess: the declaration
    // this code speaks names it.
    if matches!(kind, "self" | "this") || (kind == "identifier" && matches!(text, "self" | "this"))
    {
        let (ty, owner) = enclosing_type(index, from)?;
        return Some(Inferred {
            ty,
            basis: Basis::EnclosingType,
            from: owner,
        });
    }

    match kind {
        // `avg * 2`, where both sides are the same type.
        "binary_expression" | "binary_operator" => {
            let operator = node.child_by_field_name("operator")?;
            if !arithmetic_operator(Span::from(operator).text(source).trim()) {
                return None;
            }
            let left = node.child_by_field_name("left")?;
            let right = node.child_by_field_name("right")?;
            // The depth counts hops from binding to binding.
            let left = infer_expression(index, from, source, left, depth)?;
            let right = infer_expression(index, from, source, right, depth)?;
            (left.ty == right.ty).then_some(Inferred {
                ty: left.ty,
                basis: Basis::BothOperands,
                from: None,
            })
        }
        // `Money(0, USD)` in Python, `new Money(...)` in TypeScript.
        "call"
        | "call_expression"
        | "new_expression"
        | "method_invocation"
        | "object_creation_expression" => {
            // Each grammar names the callee differently: `function` in most, `constructor` for
            // a TypeScript `new`.
            let callee = ["function", "constructor", "name", "type"]
                .iter()
                .find_map(|field| node.child_by_field_name(field))?;
            let name = last_segment(Span::from(callee).text(source).trim());
            let target = resolve_in_workspace(index, from, name)
                .or_else(|| method_of_receiver(index, from, source, node, callee, name, depth))?;
            let resolved = index.symbol(target)?;
            if is_type_like(resolved.kind) {
                return Some(Inferred {
                    ty: resolved.name.clone(),
                    basis: Basis::Construction,
                    from: Some(target),
                });
            }
            // A function states what it returns, or it does not and nothing follows.
            let returns = of_at(index, target, depth + 1)
                .ok()?
                .parameters_free_return()?;
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
            let answer = of_at(index, target, depth + 1).ok()?;
            // This follows a derived answer through one file and no further: a chain
            // that leaves the file is a chain a reader cannot see.
            let ty = match answer.declared {
                Some(ty) => Some(ty),
                None => (other.file == from.file)
                    .then_some(answer.inferred.map(|i| i.ty))
                    .flatten(),
            }?;
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
            let candidates: Vec<&Symbol> = index
                .find_symbols(field_name, None)
                .into_iter()
                .filter(|s| {
                    s.language == language
                        && matches!(s.kind, SymbolKind::Field | SymbolKind::Property)
                })
                .collect();
            // Several records may declare a field of this name, and the receiver's type says
            // whose field this is.
            let target = match candidates.as_slice() {
                [] => return None,
                [only] => only.id,
                _ => {
                    let object = ["object", "value", "operand"]
                        .iter()
                        .find_map(|f| node.child_by_field_name(f))?;
                    let receiver = infer_expression(index, from, source, object, depth)?;
                    let owner = base_type_name(&receiver.ty);
                    let matching: Vec<&&Symbol> = candidates
                        .iter()
                        .filter(|s| s.qualifier.as_deref() == Some(owner.as_str()))
                        .collect();
                    match matching.as_slice() {
                        [only] => only.id,
                        _ => return None,
                    }
                }
            };
            let ty = of_at(index, target, depth + 1).ok()?.declared?;
            Some(Inferred {
                ty,
                basis: Basis::FieldOfRecord,
                from: Some(target),
            })
        }
        // `flag ?
        "ternary_expression" | "conditional_expression" => {
            let (then, otherwise) = ternary_branches(language, node)?;
            let then = infer_expression(index, from, source, then, depth)?;
            let otherwise = infer_expression(index, from, source, otherwise, depth)?;
            (then.ty == otherwise.ty).then_some(Inferred {
                ty: then.ty,
                basis: Basis::AgreeingBranches,
                from: None,
            })
        }
        _ => None,
    }
}

/// The two branch expressions of a conditional expression.
fn ternary_branches<'a>(language: Language, node: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    if let (Some(then), Some(otherwise)) = (
        node.child_by_field_name("consequence"),
        node.child_by_field_name("alternative"),
    ) {
        return Some((then, otherwise));
    }
    let mut cursor = node.walk();
    let parts: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
    match (language, parts.as_slice()) {
        (Language::Python, [then, _, otherwise]) => Some((*then, *otherwise)),
        (_, [_, then, otherwise]) => Some((*then, *otherwise)),
        _ => None,
    }
}

/// The type of the declaration enclosing this symbol: what `self` and `this` hold there.
fn enclosing_type(index: &Index, from: &Symbol) -> Option<(String, Option<SymbolId>)> {
    let info = index.file(&from.file)?;
    let method = info
        .symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.full_span.contains(from.full_span) && s.qualifier.is_some())
        .min_by_key(|s| s.full_span.end - s.full_span.start)?;
    Some((method.qualifier.clone()?, Some(method.id)))
}

/// The method a member call reaches, found through its receiver's type.
fn method_of_receiver(
    index: &Index,
    from: &Symbol,
    source: &str,
    call: Node<'_>,
    callee: Node<'_>,
    name: &str,
    depth: usize,
) -> Option<SymbolId> {
    // The receiver hangs off the callee in most grammars, `a.b` being the callee of `a.b()`.
    let object = ["object", "value", "operand"]
        .iter()
        .find_map(|field| callee.child_by_field_name(field))
        .or_else(|| {
            ["object", "value", "operand"]
                .iter()
                .find_map(|field| call.child_by_field_name(field))
        })?;
    let receiver = infer_expression(index, from, source, object, depth)?;
    let owner = base_type_name(&receiver.ty);
    let candidates: Vec<&Symbol> = index
        .find_symbols(name, None)
        .into_iter()
        .filter(|s| {
            s.language == from.language
                && s.kind.is_callable()
                && s.qualifier.as_deref() == Some(owner.as_str())
        })
        .collect();
    match candidates.as_slice() {
        [only] => Some(only.id),
        _ => None,
    }
}

/// The bare name of a written type: generics, sigils and the qualifying path taken off.
pub(crate) fn base_type_name(written: &str) -> String {
    let base = written.split(['<', '[']).next().unwrap_or(written).trim();
    let last = base.rsplit(['.', ':']).next().unwrap_or(base);
    // `&dyn Shape` and `impl Shape` both name `Shape`: the keyword says how the
    // value arrives, the name says what answers.
    last.trim_start_matches(['&', '*', '?'])
        .trim_start_matches("mut ")
        .trim()
        .trim_start_matches("dyn ")
        .trim_start_matches("impl ")
        .trim()
        .to_string()
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

/// The type a literal has by the language's own fixed rule.
fn fixed_literal_type(language: Language, kind: &str) -> Option<&'static str> {
    match language {
        Language::Rust => match kind {
            "string_literal" | "raw_string_literal" => Some("&str"),
            "boolean_literal" => Some("bool"),
            "char_literal" => Some("char"),
            _ => None,
        },
        Language::Zig => match kind {
            "boolean" | "true" | "false" => Some("bool"),
            _ => None,
        },
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
fn resolve_in_workspace(index: &Index, from: &Symbol, name: &str) -> Option<SymbolId> {
    let candidates: Vec<&Symbol> = index
        .find_symbols(name, None)
        .into_iter()
        .filter(|s| s.language == from.language && s.id != from.id)
        .collect();
    let here: Vec<&&Symbol> = candidates.iter().filter(|s| s.file == from.file).collect();
    match here.as_slice() {
        [only] => return Some(only.id),
        [] => {}
        // Two methods of this name in one file are as ambiguous as two anywhere.
        _ => return None,
    }
    match candidates.as_slice() {
        [only] => Some(only.id),
        _ => None,
    }
}

/// The definition of a type of this name, where one can be justified.
fn type_named(index: &Index, name: &str, from: &Symbol) -> Option<SymbolId> {
    let candidates: Vec<&Symbol> = index
        .find_symbols(name, None)
        .into_iter()
        .filter(|s| is_type_like(s.kind) && s.language == from.language)
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
    // Look outwards, because the name is often a child of the node carrying the type.
    let mut current = Some(node);
    for _ in 0..4 {
        let here = current?;
        if let Some(ty) = here.child_by_field_name("type") {
            if let Some(text) = type_text(ty, source) {
                return Some(text);
            }
        }
        let parent = here.parent()?;
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

pub(crate) fn return_type(parsed: &Parsed, source: &str, declaration: Span) -> Option<String> {
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
                // A parameter the grammar leaves whole keeps its text; its name
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
