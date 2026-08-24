//! The indented Sass grammar, compiled from the copy in this directory.
//!
//! `PROVENANCE.toml` names the upstream commit and the patches applied to it.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_sass() -> *const ();
}

/// The tree-sitter language for the indented Sass syntax.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_sass) };
