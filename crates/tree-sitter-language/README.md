# Build support for the bundled parsers

Use this page when a bundled parser cannot compile for the browser.
A crate is a Rust library or program package. A header declares the C types and functions used while compiling the parsers.
This crate supplies build files and headers required by the current third-party parser packages.

For example, an XML scanner needs a standard character type declared by `wctype.h`.
Omitting that declaration stops the scanner's build before it can read an XML file.
The public types and headers come from `tree-sitter-language` 0.1.8; the local header includes the required declaration.

WASM, or WebAssembly, is the compiled format used in the browser.
Tree-sitter 0.27 gets its browser C runtime from the application library.
Several grammar packages still ask their build scripts to compile three earlier `wasm-src` file paths.
This crate supplies empty C files at those paths so the current dependencies compile without duplicating the runtime implementation.
Remove the patch once those dependencies stop requesting the files.
