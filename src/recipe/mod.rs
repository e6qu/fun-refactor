//! Refactoring recipes: a refactoring written down.

mod lex;
mod parse;
mod run;
mod vocabulary;

pub use parse::{
    parse, Comparison, Expect, File, OnRefusal, Operation, Predicate, Recipe, Requirement, Step,
    RESERVED, SCHEMA,
};
// Only the CLI's did-you-mean asks for edit distance; without that feature the
// re-export is an unused import the wasm build refuses.
#[cfg(feature = "cli")]
pub(crate) use run::distance;
pub use run::{
    run, ExpectReport, Options, Refusal, Report, StepReport, StepWarning, FILE_PREDICATES,
    PREDICATES,
};
pub use vocabulary::{render, vocabulary, Verb, Vocabulary};
