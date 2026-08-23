//! The TypeScript and TSX grammars, compiled from the copies in this directory.
//!
//! `PROVENANCE.toml` names the upstream release and the patches applied to it.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_typescript() -> *const ();
    fn tree_sitter_tsx() -> *const ();
}

/// The tree-sitter language for TypeScript.
pub const LANGUAGE_TYPESCRIPT: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_typescript) };

/// The tree-sitter language for TSX.
pub const LANGUAGE_TSX: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_tsx) };
