fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap();
    let lang = args.next().unwrap_or_else(|| "java".into());
    let source = std::fs::read_to_string(&path).unwrap();
    let language = fun_refactor::lang::Language::from_name(&lang).unwrap();
    let parsers = fun_refactor::parse::Parsers::new();
    let parsed = parsers.parse(language, &source).unwrap();
    fn walk(n: tree_sitter::Node, src: &str, depth: usize, out: &mut Vec<String>) {
        let text = n.utf8_text(src.as_bytes()).unwrap_or("");
        let short: String = text.chars().take(28).collect();
        out.push(format!(
            "{}{}{}",
            "  ".repeat(depth),
            n.kind(),
            if n.child_count() == 0 {
                format!("  {:?}", short)
            } else {
                String::new()
            }
        ));
        let mut c = n.walk();
        for child in n.children(&mut c) {
            walk(child, src, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(parsed.root(), &source, 0, &mut out);
    println!("{}", out.join("\n"));
}
