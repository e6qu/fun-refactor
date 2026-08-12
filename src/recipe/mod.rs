//! Refactoring recipes: a refactoring written down.
//!
//! A recipe is a file that says what to find, what to do to it, and what must be true
//! afterwards. It is reviewable, re-runnable, and it fails loudly.
//!
//! Every command in this tool acts on *one* target, which is right for a person at a terminal
//! and wrong for the work people have. "retire this flag, delete what it was guarding. Tidy the
//! imports that leaves behind" is three commands in order, each depending on the last. Written
//! as a shell loop the refusals scroll past, the ordering is implicit. The thing you did is
//! written down nowhere a reviewer can read. A recipe makes the *plan* the artifact; the diff
//! is what it produces.
//!
//! It deliberately is not a programming language, no loops, no arithmetic, no conditionals, and
//! it does not extend what the tool can do. If a step could not be typed as an `fr` command, it
//! is not a step. See RECIPES.md for the design and what it argues about.

mod lex;
mod parse;
mod run;

pub use parse::{
    parse, Comparison, Expect, File, OnRefusal, Operation, Predicate, Recipe, Requirement, Step,
    RESERVED, SCHEMA,
};
pub use run::{run, ExpectReport, Options, Refusal, Report, StepReport, PREDICATES};
