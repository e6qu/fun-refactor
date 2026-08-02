use fun_refactor::{lang::Language, parse::Parsers};

fn dump(node: tree_sitter::Node, src: &str, depth: usize) {
    let text = &src[node.start_byte()..node.end_byte()];
    let text = if text.len() > 40 { &text[..40] } else { text };
    println!(
        "{}{} [{}..{}] {:?}",
        "  ".repeat(depth),
        node.kind(),
        node.start_byte(),
        node.end_byte(),
        text
    );
    let mut c = node.walk();
    for child in node.children(&mut c) {
        dump(child, src, depth + 1);
    }
}

#[test]
fn scratch_dump() {
    let src = r#"variable "region" {
  type    = string
  default = "eu-west-1"
}

module "thing" {
  source = "./modules/thing"
  region = "eu"
  size   = 3
}
"#;
    let p = Parsers::new().parse(Language::Hcl, src).unwrap();
    dump(p.root(), src, 0);
}
