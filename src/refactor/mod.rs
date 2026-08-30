//! Refactorings.

pub mod cascade;
pub mod delete;
pub mod extract;
pub mod imports;
pub mod inline;
pub mod move_symbol;
pub mod rename;
pub mod restructure;
pub mod rewrite;
pub mod signature;

use serde::Serialize;
use std::path::PathBuf;

/// Reports whether this node kind is a container whose children are statements.
pub(crate) fn is_statement_container(kind: &str) -> bool {
    kind.contains("block")
        || kind.contains("body")
        || kind == "statement_list"
        || kind == "source_file"
        || kind == "module"
        || kind == "program"
        || kind == "compound_statement"
        || kind == "subshell"
}

/// Reports whether this node kind holds members in its body rather than statements.
pub(crate) fn is_type_definition(kind: &str) -> bool {
    matches!(
        kind,
        "class_definition"
            | "class_declaration"
            | "abstract_class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "impl_item"
            | "trait_item"
    )
}

/// Reports whether this node kind is a function, whose body scopes the names inside it.
pub(crate) fn is_function_definition(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "function_declaration"
            | "function_item"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "arrow_function"
            | "method_definition"
            | "method_declaration"
            | "constructor_declaration"
            | "func_literal"
    )
}

/// Something a refactoring found and deliberately did not act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub kind: WarningKind,
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningKind {
    /// A reference matched by name but resolved too weakly to rewrite.
    WeaklyResolved,
    /// The old name appears in a string literal, comment or template.
    TextualOccurrence,
    /// Some of the file never reached the index, so this missed the uses hiding there.
    IncompleteFacts,
    /// A dispatch site renamed with the method family it could reach.
    DispatchCandidate,
}

impl WarningKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            WarningKind::WeaklyResolved => "weakly-resolved",
            WarningKind::TextualOccurrence => "textual-occurrence",
            WarningKind::IncompleteFacts => "incomplete-facts",
            WarningKind::DispatchCandidate => "dispatch-candidate",
        }
    }
}

/// One place a refusal is about, carried as data beside the prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefusalSite {
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
}

/// Describes why a refactoring refused to proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The new name would collide with an existing one.
    NameCollision { existing: String, file: PathBuf },
    /// A name inside the value means something else at the destination.
    NameCaptured { name: String, file: PathBuf },
    /// The rename would move a use under a different declaration of the same name.
    ScopeCaptured {
        name: String,
        file: PathBuf,
        line: usize,
        detail: String,
    },
    /// The requested name is not a valid identifier for the language.
    InvalidName { name: String, reason: String },
    /// The operation has no meaning in this language, so the tool does not implement it.
    Unsupported {
        operation: String,
        language: crate::lang::Language,
        /// Why *the language* cannot, a property of the language and of nothing else.
        because: &'static str,
    },
    /// These files block the operation, for a reason the language does not own.
    NotHere { operation: String, detail: String },
    /// Resolution was too weak to act on safely.
    TooWeak {
        confidence: crate::model::ResolvedConfidence,
        detail: String,
    },
    /// Two definitions answer to one name, so no call site can be attributed to either.
    AmbiguousDefinition { name: String, file: PathBuf },
    /// References still resolve to the symbol, so deleting it would break them.
    StillUsed {
        detail: String,
        references: Vec<RefusalSite>,
    },
    /// Something the tool cannot establish at all.
    Unknowable { detail: String },
    /// A decline the refusing site worded in full. Prefer a variant above where one fits;
    /// each carries structure a caller reads, and this carries only the sentence.
    Declined { detail: String },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NameCollision { existing, file } => write!(
                f,
                "'{existing}' is already defined in {}; introducing that name here would \
                 shadow or collide with it",
                file.display()
            ),
            Refusal::AmbiguousDefinition { name, file } => write!(
                f,
                "'{name}' is also defined in {}; a call names one of the two and nothing \
                 here says which, so nothing can update the call sites",
                file.display()
            ),
            Refusal::NameCaptured { name, file } => write!(
                f,
                "the value uses `{name}`, which means something else at the destination in {}; substituting it would change what the code does",
                file.display()
            ),
            Refusal::ScopeCaptured {
                name,
                file,
                line,
                detail,
            } => write!(
                f,
                "renaming to `{name}` would silently change which declaration a use binds \
                 to: {detail} at {}:{line}. The code would still compile, doing something \
                 else.",
                file.display()
            ),
            Refusal::InvalidName { name, reason } => {
                write!(f, "'{name}' is not a valid name here: {reason}")
            }
            Refusal::Unsupported {
                operation,
                language,
                because,
            } => match because.is_empty() {
                true => write!(f, "{operation} is not supported for {}", language.name()),
                false => write!(
                    f,
                    "{operation} is not supported for {}: {because}",
                    language.name()
                ),
            },
            Refusal::TooWeak { confidence, detail } => write!(
                f,
                "resolution is only '{}'. {detail}. Refusing to rewrite what nothing can verify",
                confidence.get().as_str()
            ),
            Refusal::NotHere { operation, detail } => write!(f, "{operation}: {detail}"),
            // `detail` holds the whole message, worded at the site that knows the
            // blocking sites.
            Refusal::StillUsed { detail, .. } => f.write_str(detail),
            Refusal::Unknowable { detail } => {
                write!(f, "{detail}. Refusing to change what nothing can check")
            }
            Refusal::Declined { detail } => f.write_str(detail),
        }
    }
}

impl Refusal {
    /// The positions this refusal is about, where it knows them exactly.
    pub fn references(&self) -> &[RefusalSite] {
        match self {
            Refusal::StillUsed { references, .. } => references,
            _ => &[],
        }
    }
}

impl std::error::Error for Refusal {}

/// The refusal in an error's chain, if one stopped the operation.
pub fn refusal_in(error: &anyhow::Error) -> Option<&Refusal> {
    error.chain().find_map(|c| c.downcast_ref::<Refusal>())
}

/// The declared type of a reference's receiver, where the source wrote one.
pub(crate) fn receiver_declared_type(
    index: &crate::index::Index,
    reference: &crate::model::Reference,
) -> Option<String> {
    match receiver_type(index, reference) {
        ReceiverType::Settled(ty) => Some(ty),
        ReceiverType::Reassigned | ReceiverType::Unwritten => None,
    }
}

/// What the source says a reference's receiver holds, `this` and `self` apart.
pub(crate) enum ReceiverType {
    /// The source states the type, and every assignment in scope agrees.
    Settled(String),
    /// More than one assignment in scope writes the receiver, to types that disagree.
    Reassigned,
    /// Nothing in scope says what the receiver holds.
    Unwritten,
}

pub(crate) fn receiver_type(
    index: &crate::index::Index,
    reference: &crate::model::Reference,
) -> ReceiverType {
    let Some(receiver) = reference.receiver.as_deref() else {
        return ReceiverType::Unwritten;
    };
    if matches!(receiver, "this" | "self") {
        return ReceiverType::Unwritten;
    }
    receiver_binding_type(index, reference, receiver)
}

/// The type a reference's receiver is known to be, `this`/`self` included.
pub(crate) fn receiver_known_type(
    index: &crate::index::Index,
    reference: &crate::model::Reference,
) -> Option<String> {
    thread_local! {
        /// One answer per call site per index.
        static RECEIVERS: std::cell::RefCell<
            std::collections::HashMap<(u64, std::path::PathBuf, usize), Option<String>>,
        > = std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let key = (
        index.generation,
        reference.file.clone(),
        reference.span.start,
    );
    if let Some(hit) = RECEIVERS.with(|cache| cache.borrow().get(&key).cloned()) {
        return hit;
    }
    let answer = receiver_known_type_uncached(index, reference);
    RECEIVERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 65536 {
            cache.clear();
        }
        cache.insert(key, answer.clone());
    });
    answer
}

fn receiver_known_type_uncached(
    index: &crate::index::Index,
    reference: &crate::model::Reference,
) -> Option<String> {
    let receiver = reference.receiver.as_deref()?;
    if matches!(receiver, "this" | "self") {
        let info = index.file(&reference.file)?;
        return info
            .symbols
            .iter()
            .filter_map(|id| index.symbol(*id))
            .filter(|s| s.full_span.contains(reference.span) && s.qualifier.is_some())
            .min_by_key(|s| s.full_span.end - s.full_span.start)
            .and_then(|s| s.qualifier.clone());
    }
    match receiver_binding_type(index, reference, receiver) {
        ReceiverType::Settled(ty) => Some(ty),
        ReceiverType::Reassigned | ReceiverType::Unwritten => None,
    }
}

fn receiver_binding_type(
    index: &crate::index::Index,
    reference: &crate::model::Reference,
    receiver: &str,
) -> ReceiverType {
    let Some(info) = index.file(&reference.file) else {
        return ReceiverType::Unwritten;
    };
    let chain = info.scope_chain(reference.scope);
    let binding = info
        .symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| {
            s.name == receiver
                && matches!(
                    s.kind,
                    crate::model::SymbolKind::Variable
                        | crate::model::SymbolKind::Parameter
                        | crate::model::SymbolKind::Constant
                )
        })
        .filter(|s| chain.contains(&s.scope))
        .min_by_key(|s| {
            chain
                .iter()
                .position(|sc| *sc == s.scope)
                .unwrap_or(usize::MAX)
        });
    let Some(binding) = binding else {
        return ReceiverType::Unwritten;
    };
    // `var b = new B()` writes the type on the right of the `=`, which counts as the source's
    // own words.
    let written = match crate::analysis::types::held_by(index, binding.id) {
        crate::analysis::types::Held::Settled(ty) => ty,
        crate::analysis::types::Held::Reassigned => return ReceiverType::Reassigned,
        crate::analysis::types::Held::Unwritten => return ReceiverType::Unwritten,
    };
    // A receiver declared `&Facts` reaches what one declared `Facts` reaches, so the
    // generics, sigils and path come off and the bare name answers.
    ReceiverType::Settled(crate::analysis::types::base_type_name(&written))
}
