//! Print a parse tree with field names.
//!
//! Every per-language bug in `src/refactor/` so far has come from a grammar naming
//! something differently than expected — Zig calling an `if`'s consequence `body`,
//! the C family folding the `:` into a type annotation, Go slipping a
//! `statement_list` between a block and its statements. None of that is visible in
//! the source; it is visible here.
//!
//!     cargo run --example dump -- path/to/file.ts
use fun_refactor::parse::Parsers;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: dump <file>");
    let source = std::fs::read_to_string(&path)?;
    let language = fun_refactor::lang::detect(std::path::Path::new(&path))
        .expect("unknown language for that extension");
    let parsed = Parsers::new().parse(language, &source)?;
    let mut cursor = parsed.tree.walk();
    let mut depth = 0usize;
    loop {
        let node = cursor.node();
        if node.is_named() {
            let field = cursor
                .field_name()
                .map(|f| format!("{f}: "))
                .unwrap_or_default();
            println!("{:indent$}{field}{}", "", node.kind(), indent = depth * 2);
        }
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(());
            }
            depth -= 1;
        }
    }
}
