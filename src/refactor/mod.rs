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

use crate::model::Confidence;
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
/// Warnings are the honest half of the output: they say what the tool saw, why it
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
    /// A file could not be parsed cleanly, so its facts may be incomplete.
    ParseErrors,
}

impl WarningKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            WarningKind::WeaklyResolved => "weakly-resolved",
            WarningKind::TextualOccurrence => "textual-occurrence",
            WarningKind::ParseErrors => "parse-errors",
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
    Unsupported { operation: String, language: String },
    /// Resolution was too weak to act on safely.
    TooWeak {
        confidence: Confidence,
        detail: String,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NameCollision { existing, file } => write!(
                f,
                "'{existing}' is already defined in {}; renaming would shadow or collide with it",
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
            } => write!(f, "{operation} is not supported for {language}"),
            Refusal::TooWeak { confidence, detail } => write!(
                f,
                "resolution is only '{}' — {detail}. Refusing to rewrite what cannot be verified",
                confidence.as_str()
            ),
        }
    }
}

impl std::error::Error for Refusal {}
