//! The SCSS grammar, compiled from the copy in this directory.
//!
//! `PROVENANCE.toml` names the upstream release and the patches applied to it.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_scss() -> *const ();
}

/// The tree-sitter language for SCSS.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_scss) };
