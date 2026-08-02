//! PROBE (temporary): establish what the index really gives us for config langs.

use fun_refactor::{
    index::Index,
    lang::Language,
    model::*,
    parse::Parsers,
    scan::{scan, ScanOptions},
};
use std::path::Path;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

#[test]
fn probe_hcl() {
    let (tmp, index) = workspace(&[
        (
            "variables.tf",
            "variable \"region\" {\n  type = string\n  default = \"us-east-1\"\n}\n\nvariable \"env\" {\n  type = string\n}\n",
        ),
        (
            "main.tf",
            "locals {\n  prefix = \"app-${var.env}\"\n  name   = \"${local.prefix}-${var.region}\"\n}\n\noutput \"n\" {\n  value = local.name\n}\n",
        ),
        ("terraform.tfvars", "env = \"prod\"\n"),
    ]);
    for (path, info) in index.files() {
        println!("FILE {:?} lang={:?}", path.strip_prefix(tmp.path()), info.language);
        for id in &info.symbols {
            let s = index.symbol(*id).unwrap();
            println!("  SYM {:?} {:?} qual={:?}", s.name, s.kind, s.qualifier);
        }
        for i in &info.references {
            let r = &index.references[*i];
            println!(
                "  REF {:?} kind={:?} conf={:?} target={:?}",
                r.name,
                r.kind,
                r.confidence,
                r.target.and_then(|t| index.symbol(t)).map(|s| (s.name.clone(), s.file.file_name().unwrap().to_owned()))
            );
        }
    }
    // tfvars tree shape
    let src = "env = \"prod\"\nother = 1\n";
    let parsed = Parsers::new().parse(Language::Hcl, src).unwrap();
    fn dump(node: tree_sitter::Node, src: &str, depth: usize) {
        println!("{}{} {:?}", "  ".repeat(depth), node.kind(), &src[node.byte_range()]);
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            dump(ch, src, depth + 1);
        }
    }
    dump(parsed.root(), src, 0);
}

#[test]
fn probe_helm() {
    let (tmp, index) = workspace(&[
        ("chart/Chart.yaml", "name: parent\nversion: 0.1.0\n"),
        ("chart/values.yaml", "image:\n  tag: \"1.0\"\nmysql:\n  image:\n    tag: \"8.0\"\n"),
        ("chart/charts/mysql/Chart.yaml", "name: mysql\nversion: 1.0.0\n"),
        ("chart/charts/mysql/values.yaml", "image:\n  tag: \"5.7\"\n  repository: mysql\n"),
        (
            "chart/templates/deployment.yaml",
            "spec:\n  image: {{ .Values.image.tag }}\n  replicas: {{ .Values.replicaCount | default 3 }}\n",
        ),
        ("chart/values-prod.yaml", "image:\n  tag: \"2.0\"\n"),
    ]);
    for (path, info) in index.files() {
        println!("FILE {:?} lang={:?}", path.strip_prefix(tmp.path()), info.language);
        for id in &info.symbols {
            let s = index.symbol(*id).unwrap();
            println!("  SYM {:?} {:?} qual={:?} container={:?}", s.name, s.kind, s.qualifier, s.container);
        }
        for i in &info.references {
            let r = &index.references[*i];
            println!("  REF {:?} {:?} {:?}", r.name, r.kind, r.confidence);
        }
    }
}

#[test]
fn probe_yaml_anchor() {
    let (tmp, index) = workspace(&[("a.yaml", "defaults: &base\n  retries: 3\nuse:\n  <<: *base\nref: *base\n")]);
    let path = tmp.path().join("a.yaml");
    let info = index.file(&path).unwrap();
    for i in &info.references {
        let r = &index.references[*i];
        println!(
            "REF {:?} target={:?} conf={:?}",
            r.name,
            r.target.and_then(|t| index.symbol(t)).map(|s| (s.name.clone(), s.kind)),
            r.confidence
        );
    }
}

#[test]
fn probe_css() {
    let src = concat!(
        "@layer base {\n  .btn { color: green; }\n}\n",
        ":root { --brand: red; --accent: var(--brand); }\n",
        "#main .btn { color: blue; }\n",
        ".btn { color: red !important; }\n",
        "button.btn:hover { color: pink; }\n",
    );
    let parsed = Parsers::new().parse(Language::Css, src).unwrap();
    fn dump(node: tree_sitter::Node, src: &str, depth: usize) {
        println!(
            "{}{} [{:?}]",
            "  ".repeat(depth),
            node.kind(),
            &src[node.byte_range()].replace('\n', "\\n")
        );
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            dump(ch, src, depth + 1);
        }
    }
    dump(parsed.root(), src, 0);
    let (_tmp, index) = workspace(&[("a.css", src)]);
    for (_p, info) in index.files() {
        for id in &info.symbols {
            let s = index.symbol(*id).unwrap();
            println!("SYM {:?} {:?} span={:?}", s.name, s.kind, s.full_span);
        }
    }
    let _ = Path::new("x");
    let _ = SymbolKind::Key;
}
