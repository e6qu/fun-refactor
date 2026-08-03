//! fun-refactor — multi-language refactoring and code intelligence on tree-sitter.
//!
//! Layering (see PLAN.md):
//! - [`span`] / [`lang`]: byte-native positions and language identity
//! - [`parse`]: tree-sitter parsing for 12 languages
//! - [`scan`]: workspace discovery
//! - [`edit`]: lossless byte-splice edit engine
//! - [`cli`]: command surface

pub mod analysis;
#[cfg(feature = "cli")]
pub mod cache;
pub mod capabilities;
#[cfg(feature = "cli")]
pub mod cli;
pub mod vfs;
// The C the grammars call, which `wasm32-unknown-unknown` does not supply. Linked
// for its symbols, never called from Rust.
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
extern crate fun_refactor_wasm_libc;

pub mod edit;
pub mod extract;
pub mod helm;
pub mod index;
pub mod lang;
pub mod model;
pub mod navigate;
pub mod parse;
pub mod refactor;
#[cfg(feature = "cli")]
pub mod scan;
pub mod span;
#[cfg(feature = "wasm")]
pub mod wasm;
