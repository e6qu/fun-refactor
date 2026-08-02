//! fun-refactor — multi-language refactoring and code intelligence on tree-sitter.
//!
//! Layering (see PLAN.md):
//! - [`span`] / [`lang`]: byte-native positions and language identity
//! - [`parse`]: tree-sitter parsing for 12 languages
//! - [`scan`]: workspace discovery
//! - [`edit`]: lossless byte-splice edit engine
//! - [`cli`]: command surface

pub mod analysis;
pub mod cli;
pub mod edit;
pub mod extract;
pub mod index;
pub mod lang;
pub mod model;
pub mod parse;
pub mod refactor;
pub mod scan;
pub mod span;
