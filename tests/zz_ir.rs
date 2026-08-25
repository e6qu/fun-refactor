#[test]
fn dump() {
    let src = "const ParseError = error{Empty};\n\nfn parseLen(s: []const u8) ParseError!usize {\n    if (s.len == 0) return ParseError.Empty;\n    return s.len;\n}\n";
    let parsers = fun_refactor::parse::Parsers::new();
    let parsed = parsers
        .parse(fun_refactor::lang::Language::Zig, src)
        .unwrap();
    let module =
        fun_refactor::transpile::debug_read(fun_refactor::lang::Language::Zig, src, parsed.root())
            .unwrap();
    for item in &module.items {
        if let fun_refactor::transpile::ir::Item::Function(f) = item {
            println!("fn {} => {:?}", f.name, f.body.first());
        }
    }
}
