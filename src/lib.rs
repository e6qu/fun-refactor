//! fun-refactor, multi-language refactoring and code intelligence on tree-sitter.

pub mod analysis;
#[cfg(feature = "cli")]
pub mod cache;
pub mod capabilities;
#[cfg(feature = "cli")]
pub mod cli;
pub mod vfs;
// The C the grammars call, which `wasm32-unknown-unknown` does not supply.
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
extern crate fun_refactor_wasm_libc;

pub mod edit;
pub mod extract;
pub mod helm;
pub mod index;
pub mod lang;
pub mod mentions;
pub mod model;
pub mod navigate;
pub mod openapi;
pub mod parse;
pub mod recipe;
pub mod refactor;
#[cfg(feature = "cli")]
pub mod scan;
pub mod span;
#[cfg(test)]
pub mod testing;
pub mod translate;
pub mod transpile;
#[cfg(feature = "wasm")]
pub mod wasm;
