//! Compile the grammar beside this file.

fn main() {
    let src = std::path::Path::new("src");
    for file in ["parser.c", "scanner.c"] {
        println!("cargo:rerun-if-changed={}", src.join(file).display());
    }
    let parser = std::fs::read_to_string(src.join("parser.c")).unwrap();
    let scanner = std::fs::read_to_string(src.join("scanner.c")).unwrap();
    assert_eq!(
        external_tokens(&parser, "enum ts_external_scanner_symbol_identifiers {"),
        external_tokens(&scanner, "enum TokenType {"),
        "Lean parser and scanner disagree; regenerate the Lean parser before building"
    );
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(src.join("parser.c"))
        .file(src.join("scanner.c"))
        .compile("tree-sitter-lean");
}

fn external_tokens(source: &str, declaration: &str) -> Vec<String> {
    source
        .split_once(declaration)
        .expect("external token enum is missing")
        .1
        .split_once("};")
        .expect("external token enum is incomplete")
        .0
        .lines()
        .map(|line| line.split("//").next().unwrap().trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.trim_end_matches(',')
                .split('=')
                .next()
                .unwrap()
                .trim()
                .trim_start_matches("ts_external_token__")
                .to_ascii_uppercase()
        })
        .collect()
}
