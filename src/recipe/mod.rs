//! Refactoring recipes: a refactoring written down.

mod format;
mod lex;
mod parse;
mod run;
mod vocabulary;

pub use format::{file as format_file, source as format_source};
pub use parse::{
    parse, Comparison, Expect, File, OnRefusal, Operation, Predicate, Recipe, Requirement, Step,
    StepMeasure, StepTarget, RESERVED, SCHEMA,
};
// Only the CLI's did-you-mean asks for edit distance; without that feature the
// re-export is an unused import the wasm build refuses.
#[cfg(feature = "cli")]
pub(crate) use run::distance;
pub use run::{
    run, run_file, ExpectReport, Options, Refusal, Report, Sources, StepReport, StepWarning,
    WorkspaceReport, FILE_PREDICATES, PREDICATES,
};
pub use vocabulary::{render, vocabulary, Verb, Vocabulary, EXPECTATIONS, REQUIREMENTS};
