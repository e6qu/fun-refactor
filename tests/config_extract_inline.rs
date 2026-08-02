//! scratch exploration (temporary)
use fun_refactor::{extract::Extractor, lang::Language, parse::Parsers, span::Span};
use std::path::Path;

fn walk(node: tree_sitter::Node<'_>, src: &str, depth: usize) {
    println!(
        "{:indent$}{} [{}..{}] {:?}",
        "",
        node.kind(),
        node.start_byte(),
        node.end_byte(),
        &src[node.start_byte()..node.end_byte()],
        indent = depth * 2
    );
    let mut c = node.walk();
    for child in node.children(&mut c) {
        walk(child, src, depth + 1);
    }
}

fn dump(lang: Language, src: &str) {
    let p = Parsers::new().parse(lang, src).unwrap();
    println!("=== {lang} ===");
    walk(p.root(), src, 0);
    let f = Extractor::new().extract(&p, Path::new("t"), src).unwrap();
    for s in &f.symbols {
        println!(
            "SYM {:?} {:?} name={:?} full={:?}",
            s.kind,
            s.name,
            s.name_span.text(src),
            s.full_span.text(src)
        );
    }
    for r in &f.references {
        println!("REF {:?} {:?} at {:?}", r.kind, r.name, r.span.text(src));
    }
    println!();
    let _ = Span::new(0, 0);
}

#[test]
fn explore() {
    dump(
        Language::Markdown,
        "See [the docs](https://example.com/d) and [x][lbl] and [short] and [coll][].\n\n[lbl]: /a\n",
    );
    dump(Language::Yaml, "a: nginx\nc: &n 7\nd: *n\n");
}
