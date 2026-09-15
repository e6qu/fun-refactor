# tree-sitter-language compatibility bridge

This crate preserves the public API and headers from `tree-sitter-language` 0.1.8.
Its `wctype.h` also declares the standard character type required by current XML scanners.
Tree-sitter 0.27 supplies its WASM libc from the application library. Several current
grammar crates still compile the three source paths exposed before 0.1.8, so the bridge
keeps those paths as empty translation units. Remove the patch when those grammars stop
requesting `wasm-src`.
