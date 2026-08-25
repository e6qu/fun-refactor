#[test]
fn dump() {
    let src = "pub fn main() void {\n    {\n        const a = 1;\n        _ = a;\n    }\n    const x = blk: {\n        if (c) break :blk 1;\n        break :blk 2;\n    };\n    outer: while (true) {\n        break :outer;\n    }\n}\n";
    let parsers = fun_refactor::parse::Parsers::new();
    let parsed = parsers
        .parse(fun_refactor::lang::Language::Zig, src)
        .unwrap();
    fn walk(node: tree_sitter::Node, depth: usize, src: &str) {
        let text: String = src[node.byte_range()].chars().take(34).collect();
        println!(
            "{}{} {:?}",
            "  ".repeat(depth),
            node.kind(),
            text.replace('\n', " ")
        );
        let mut c = node.walk();
        for child in node.children(&mut c) {
            walk(child, depth + 1, src);
        }
    }
    walk(parsed.root(), 0, src);
}
