//! The Zig grammar, compiled from the copy in this directory.
//!
//! `PROVENANCE.toml` names the upstream release and the patch applied to it. The
//! published parser rejects `struct {}`, which Zig accepts.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_zig() -> *const ();
}

/// The tree-sitter language for Zig.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_zig) };
