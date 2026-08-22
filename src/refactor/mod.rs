//! Refactorings.
//!
//! Every refactoring returns a *plan*: an [`crate::edit::EditSet`] plus whatever it
//! could not do. The caller renders a diff or commits, so no refactoring touches a
//! file itself. A plan never half-applies, and the tool rewrites nothing it could not
//! verify (PLAN.md D8).

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
///
/// Several refactorings ask whether a node is the last statement in its block. A
/// wrapper node counted as a statement makes a block of many look like a block of
/// one. Go's `statement_list` sits between a block and its statements, so counting it
/// hoists a guard clause out from under its condition.
///
/// Shell function bodies are `compound_statement`, which no other grammar in the set
/// uses. The list therefore covers more kinds than extraction alone needs.
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
///
/// A function spliced into one of these becomes a method, reachable only through a
/// receiver, so a plain call to it does not resolve. Extraction hoists past every one
/// of these instead of writing the definition where a member would go.
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
///
/// Hoisting stops at one of these. A definition moved past a function loses the
/// enclosing locals it reads, and nothing puts them back.
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
///
/// A warning says what the tool saw, why it declined, and where a human should look.
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
    /// Some of the file did not reach the index, so uses hidden there were not seen.
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
///
/// An ambiguity's rival definitions ride in the JSON error as `candidates`. These
/// sites ride the same way, so an agent reads them without parsing the prose back
/// apart.
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
    /// A name inside the value means something else where the value would be moved to.
    ///
    /// A collision concerns the name being introduced. This concerns a name being
    /// *carried*: substituting `price_of(order)` into a scope where `order` is a
    /// different binding changes what the code does.
    NameCaptured { name: String, file: PathBuf },
    /// The rename would move a use under a different declaration of the same name.
    ///
    /// Nothing collides here, and no value travels. Both declarations stay put, and
    /// only the binding a use resolves to changes. An inner `let temp` can stand
    /// between an outer `value` and its use. Renaming `value` to `temp` turns that use
    /// into a read of the inner binding, and the file still compiles.
    ScopeCaptured {
        name: String,
        file: PathBuf,
        line: usize,
        detail: String,
    },
    /// The requested name is not a valid identifier for the language.
    InvalidName { name: String, reason: String },
    /// The operation has no meaning in this language, so the tool does not implement it.
    ///
    /// `language` holds the language and nothing else. Put the reason in `because`.
    Unsupported {
        operation: String,
        language: crate::lang::Language,
        /// Why *the language* cannot, a property of the language and of nothing else.
        /// The type is `&'static str` so the rule holds. A reason about one particular
        /// input interpolates a path or a name into itself, and no interpolated string
        /// fits here. A fault that belongs to the files goes to [`Refusal::NotHere`].
        because: &'static str,
    },
    /// These files block the operation, for a reason the language does not own.
    ///
    /// Two paths in different crates, a directory that is its own Terraform module, a
    /// relative import that would climb out of its root. The tool declined on purpose
    /// and wrote nothing, so each of these counts as a considered refusal. Naming the
    /// language instead would contradict the capability matrix.
    NotHere { operation: String, detail: String },
    /// Resolution was too weak to act on safely.
    TooWeak {
        confidence: crate::model::ResolvedConfidence,
        detail: String,
    },
    /// Two definitions answer to one name, so no call site can be attributed to either.
    ///
    /// Both definitions were there before and the operation introduces nothing, which
    /// separates this from [`Refusal::NameCollision`]. Bash resolves the name at run
    /// time by whichever definition ran last, and no static reading predicts that.
    AmbiguousDefinition { name: String, file: PathBuf },
    /// References still resolve to the symbol, so deleting it would break them.
    ///
    /// `detail` carries the full listing, one blocking site per line, and `references`
    /// carries the same listing as data for the JSON error object. The refusal has its
    /// own type because the exit code comes from the error's type, and `fr --help`
    /// promises every considered refusal exits 5.
    StillUsed {
        detail: String,
        references: Vec<RefusalSite>,
    },
    /// Something the tool cannot establish at all.
    ///
    /// [`Refusal::TooWeak`] covers a resolution that exists and is too weak to act on.
    /// Here no resolution exists at all. A grammar may never expose a call as a call,
    /// or a shell script may source a path computed at run time. Reporting these as a
    /// confidence produced the self-contradicting "resolution is only 'exact'".
    Unknowable { detail: String },
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
                 here says which, so the call sites cannot be updated",
                file.display()
            ),
            Refusal::NameCaptured { name, file } => write!(
                f,
                "the value uses `{name}`, which means something else where it would be \
                 moved to in {}; substituting it would change what the code does",
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
                "resolution is only '{}'. {detail}. Refusing to rewrite what cannot be verified",
                confidence.get().as_str()
            ),
            Refusal::NotHere { operation, detail } => write!(f, "{operation}: {detail}"),
            // `detail` holds the whole message, worded at the site that knows the
            // blocking sites.
            Refusal::StillUsed { detail, .. } => f.write_str(detail),
            Refusal::Unknowable { detail } => {
                write!(f, "{detail}. Refusing to change what cannot be checked")
            }
        }
    }
}

impl Refusal {
    /// The positions this refusal is about, where it knows them exactly.
    ///
    /// Variants that carry a file without a position stay out. Inventing a line for
    /// them would report a measurement nobody took.
    pub fn references(&self) -> &[RefusalSite] {
        match self {
            Refusal::StillUsed { references, .. } => references,
            _ => &[],
        }
    }
}

impl std::error::Error for Refusal {}

/// The refusal in an error's chain, if one stopped the operation.
///
/// One lookup shared by the exit-code choice, the JSON error object and the
/// recipe report. The three cannot disagree about what counts as a refusal.
pub fn refusal_in(error: &anyhow::Error) -> Option<&Refusal> {
    error.chain().find_map(|c| c.downcast_ref::<Refusal>())
}

/// The declared type of a reference's receiver, where the source wrote one.
///
/// `b.size(2)` with `B b = ...` above it names the type outright; the nearest
/// binding of the receiver's name in scope carries it. `this` and `self` are the
/// enclosing instance and answer a different question.
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
///
/// Three answers, because the two silences differ. A receiver nothing describes
/// and a receiver assigned two types are both unsafe to rewrite. A reader told
/// the first about the second hunts for an annotation that is already there.
pub(crate) enum ReceiverType {
    /// The source states the type, and every assignment in scope agrees.
    Settled(String),
    /// The receiver is assigned more than once in scope, to types that disagree.
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
///
/// The enclosing instance is the one receiver whose type is never a guess: the
/// class this code is written in. [`receiver_declared_type`] stays separate
/// because the warnings built on it say "declared", and `self` takes its type
/// another way.
pub(crate) fn receiver_known_type(
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
    // `var b = new B()` writes the type on the right of the `=`, which counts as
    // the source's own words. A name the scope assigns again holds two things, and
    // this site stops treating it as evidence.
    let written = match crate::analysis::types::held_by(index, binding.id) {
        crate::analysis::types::Held::Settled(ty) => ty,
        crate::analysis::types::Held::Reassigned => return ReceiverType::Reassigned,
        crate::analysis::types::Held::Unwritten => return ReceiverType::Unwritten,
    };
    // `List<Order>` names `List`, and the last plain segment is the type's own
    // name. `&Facts`, `*Buffer` and `?Handle` name the types their sigils borrow,
    // point at or make optional. A receiver declared `&Facts` reaches what one
    // declared `Facts` reaches, so strip the sigil.
    let base = written.split(['<', '[']).next().unwrap_or(&written).trim();
    let last = base.rsplit(['.', ':']).next().unwrap_or(base);
    let bare = last
        .trim_start_matches(['&', '*', '?'])
        .trim_start_matches("mut ")
        .trim();
    ReceiverType::Settled(bare.to_string())
}
