//! Refactorings.
//!
//! Every refactoring returns a *plan* — an [`crate::edit::EditSet`] plus whatever it
//! could not do — instead of touching files. The caller renders a diff or commits.
//! Nothing is ever half-applied, and nothing that could not be verified is rewritten
//! silently (PLAN.md D8).

pub mod delete;
pub mod extract;
pub mod imports;
pub mod inline;
pub mod move_symbol;
pub mod rename;
pub mod signature;

use crate::model::Confidence;
use serde::Serialize;
use std::path::PathBuf;

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
