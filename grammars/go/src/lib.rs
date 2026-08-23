//! The Go grammar, compiled from the copy in this directory.
//!
//! `PROVENANCE.toml` names the upstream release and the patch applied to it.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_go() -> *const ();
}

/// The tree-sitter language for Go.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_go) };
