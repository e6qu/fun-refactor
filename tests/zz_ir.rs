#[test]
fn dump() {
    let path = std::path::Path::new("tests/corpus/zls/DocumentStore.zig");
    let module = fun_refactor::transpile::read_file(path).unwrap();
    let text =
        fun_refactor::transpile::debug_write(fun_refactor::lang::Language::Rust, &module).unwrap();
    let parsers = fun_refactor::parse::Parsers::new();
    let parsed = parsers
        .parse(fun_refactor::lang::Language::Rust, &text)
        .unwrap();
    fn errs(node: tree_sitter::Node, text: &str, out: &mut Vec<(usize, String)>) {
        if node.is_error() || node.is_missing() {
            let line = text[..node.start_byte()].lines().count();
            out.push((
                line,
                text[node.start_byte()..node.end_byte().min(node.start_byte() + 90)].to_string(),
            ));
        }
        let mut c = node.walk();
        for child in node.children(&mut c) {
            errs(child, text, out);
        }
    }
    let mut found = Vec::new();
    errs(parsed.root(), &text, &mut found);
    for (line, snip) in found.iter().take(6) {
        println!("ERR line {}: {:?}", line, snip);
    }
    for (n, l) in text.lines().enumerate() {
        if found.iter().any(|(fl, _)| (n + 1).abs_diff(*fl) <= 1) {
            println!("{:5} {}", n + 1, l);
        }
    }
}
