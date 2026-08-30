//! The Lean 4 grammar, compiled from the copy in this directory.
//!
//! `PROVENANCE.toml` names the upstream release. Nothing here is patched: the parser is
//! the published one, regenerated so its ABI matches the tree-sitter this build links.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_lean() -> *const ();
}

/// The tree-sitter language for Lean 4.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_lean) };
