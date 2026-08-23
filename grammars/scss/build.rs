//! Compile the grammar beside this file.

fn main() {
    let src = std::path::Path::new("src");
    let mut build = cc::Build::new();
    // The scanner entry points take the payload, buffer and length tree-sitter passes
    // every scanner, and this one keeps no state to use them for.
    build
        .std("c11")
        .include(src)
        .flag_if_supported("-Wno-unused-parameter");
    for file in ["parser.c", "scanner.c"] {
        let path = src.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        build.file(&path);
    }
    build.compile("tree-sitter-scss");
}
