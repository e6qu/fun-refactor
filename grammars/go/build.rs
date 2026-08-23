//! Compile the grammar beside this file.

fn main() {
    let src = std::path::Path::new("src");
    let mut build = cc::Build::new();
    build.std("c11").include(src);
    for file in ["parser.c", "scanner.c"] {
        let path = src.join(file);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
            build.file(&path);
        }
    }
    build.compile("tree-sitter-go");
}
