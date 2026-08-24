//! Compile the grammar beside this file.

fn main() {
    let src = std::path::Path::new("src");
    let mut build = cc::Build::new();
    // The scanner entry points take the payload tree-sitter passes every scanner, and
    // some of them keep no state to use it for.
    build
        .std("c11")
        .include(src)
        .flag_if_supported("-Wno-unused-parameter");
    for file in ["parser.c", "scanner.c"] {
        let path = src.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        build.file(&path);
    }
    build.compile("tree-sitter-sass");
}
