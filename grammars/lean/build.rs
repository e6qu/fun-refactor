//! Compile the grammar beside this file.

fn main() {
    let src = std::path::Path::new("src");
    for file in ["parser.c", "scanner.c"] {
        println!("cargo:rerun-if-changed={}", src.join(file).display());
    }
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(src.join("parser.c"))
        .file(src.join("scanner.c"))
        .compile("tree-sitter-lean");
}
