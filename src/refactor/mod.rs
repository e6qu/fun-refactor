//! Refactorings.
//!
//! Every refactoring returns a *plan* — an [`crate::edit::EditSet`] plus whatever it
//! could not do — instead of touching files. The caller renders a diff or commits.
//! Nothing is ever half-applied, and nothing that could not be verified is rewritten
//! silently (PLAN.md D8).

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

/// Is this node kind a container whose children are statements?
///
/// Shared because getting it wrong is not a cosmetic matter: several refactorings
/// ask "is this the last statement in its block", and a wrapper node mistaken for a
/// statement makes a block of many look like a block of one. Go's `statement_list`
/// sits between a block and its statements and did exactly that, which let a guard
/// clause hoist code out from under the condition that guarded it.
///
/// Shell function bodies are `compound_statement`, which no other grammar in the set
/// uses, so the list is not the same as the one extraction alone would need.
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

/// Something a refactoring found but deliberately did not act on.
///
/// Warnings say what the tool saw, why it
/// declined, and where a human should look.
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
}

impl WarningKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            WarningKind::WeaklyResolved => "weakly-resolved",
            WarningKind::TextualOccurrence => "textual-occurrence",
            WarningKind::IncompleteFacts => "incomplete-facts",
        }
    }
}

/// Describes why a refactoring refused to proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The new name would collide with an existing one.
    NameCollision { existing: String, file: PathBuf },
    /// A name inside the value means something else where the value would be moved to.
    ///
    /// Distinct from a collision, which is about the name being introduced. This is
    /// about a name being *carried*: substituting `price_of(order)` into a scope where
    /// `order` is a different binding changes what the code does, and saying "renaming
    /// would shadow or collide with it" describes neither the operation nor the fault.
    NameCaptured { name: String, file: PathBuf },
    /// The requested name is not a valid identifier for the language.
    InvalidName { name: String, reason: String },
    /// The operation is not implemented for this language.
    /// The operation has no meaning in this language.
    ///
    /// `because` exists so that `language` can be the language. Without it, ten of the
    /// fifteen sites raising this wrote `format!("{lang} — {reason}")` into a field named
    /// `language`, and one wrote "a variable is not a flag", which is not a language at
    /// all. The field's name and type had stopped being true.
    Unsupported {
        operation: String,
        language: crate::lang::Language,
        /// Why *the language* cannot, which is a property of the language and of nothing
        /// else. `&'static str` and not `String`, because that is what makes the rule
        /// hold: a reason about this particular input has a path or a name interpolated
        /// into it, and an interpolated reason will not fit here.
        ///
        /// Adding `because` at all was the previous attempt. It made the mistake easier to
        /// avoid and did not stop it: `fr move` went on to tell a Rust user that Rust was
        /// unsupported when the fault was a destination outside `src/`, and four more
        /// sites were doing the same with crate roots and relative paths. A field that can
        /// be filled in correctly is not a field that cannot be filled in wrongly.
        because: &'static str,
    },
    /// The operation cannot be performed on *these* files, for a reason that is not the
    /// language's.
    ///
    /// Two paths in different crates, a directory that is its own Terraform module, a
    /// relative import that would climb out of its root. Each of these was once a
    /// [`Refusal::Unsupported`] naming the language, which said the opposite of what the
    /// capability matrix says and of what the command does elsewhere. They are still
    /// considered refusals — the tool declined on purpose and wrote nothing — and this is
    /// where a considered refusal goes when the language is not what is at fault.
    NotHere { operation: String, detail: String },
    /// Resolution was too weak to act on safely.
    TooWeak {
        confidence: crate::model::ResolvedConfidence,
        detail: String,
    },
    /// Two definitions answer to one name, so no call site can be attributed to either.
    ///
    /// Not a [`Refusal::NameCollision`]: nothing is being introduced, and both
    /// definitions were there before. Bash resolves this at run time by whichever
    /// definition ran last, which no static reading can predict.
    AmbiguousDefinition { name: String, file: PathBuf },
    /// Something the tool cannot establish at all.
    ///
    /// Distinct from [`Refusal::TooWeak`], which is about a resolution that exists and
    /// is not strong enough to act on. These are the cases where there is nothing to be
    /// weak about: a grammar that does not expose a call as a call, a shell script that
    /// sources a path computed at run time. Reporting them as a confidence produced
    /// "resolution is only 'exact'", which is a sentence that contradicts itself — and
    /// leaves the reader looking for a resolution problem that is not there.
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
                "resolution is only '{}' — {detail}. Refusing to rewrite what cannot be verified",
                confidence.get().as_str()
            ),
            Refusal::NotHere { operation, detail } => write!(f, "{operation}: {detail}"),
            Refusal::Unknowable { detail } => {
                write!(f, "{detail}. Refusing to change what cannot be checked")
            }
        }
    }
}

impl std::error::Error for Refusal {}
