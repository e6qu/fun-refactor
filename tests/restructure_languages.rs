//! Exploration scaffold (temporary).

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;
use fun_refactor::refactor::restructure;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn try_apply(lang: Language, file: &str, src: &str, pat: &str, tpl: &str) {
    let (tmp, index) = workspace(&[(file, src)]);
    match restructure::apply(&index, lang, pat, tpl) {
        Ok(plan) => {
            let path = tmp.path().join(file);
            let original = std::fs::read_to_string(&path).unwrap();
            let out = match plan.edits.edits_for(&path) {
                Some(e) => fun_refactor::edit::apply_to_string(&original, e).unwrap(),
                None => original,
            };
            println!("=== {lang} '{pat}' -> '{tpl}': {} matches\n{out}", plan.matches.len());
        }
        Err(e) => println!("=== {lang} '{pat}' -> '{tpl}': ERR {e}"),
    }
}

fn ranges(lang: Language, src: &str) {
    let p = Parsers::new().parse(lang, src).unwrap();
    println!("--- {lang} {src:?} errors={}", p.has_errors());
    let mut stack = vec![(p.root(), 0usize)];
    while let Some((n, d)) = stack.pop() {
        println!("{:indent$}{} {}..{}", "", n.kind(), n.start_byte(), n.end_byte(), indent = d * 2);
        let mut c = n.walk();
        let kids: Vec<_> = n.named_children(&mut c).collect();
        for k in kids.into_iter().rev() {
            stack.push((k, d + 1));
        }
    }
}

#[test]
fn explore() {
    try_apply(Language::Bash, "a.sh", "curl https://x\ncurl https://y\n", "curl $U", "wget $U");
    try_apply(Language::Hcl, "a.tf", "x = var.n\n", "var.$X", "local.$X");
    try_apply(Language::Yaml, "a.yaml", "image: nginx\n", "image: $X", "img: $X");
    try_apply(Language::Css, "a.css", ".b { color: red; }\n", "color: $X", "colour: $X");
    try_apply(Language::Html, "a.html", "<div class=\"a\">hi</div>\n", "<div class=\"$C\">$T</div>", "<span class=\"$C\">$T</span>");
    try_apply(Language::Xml, "a.xml", "<r><c id=\"a\">t</c></r>\n", "<c id=\"$I\">$T</c>", "<d id=\"$I\">$T</d>");
    try_apply(Language::Markdown, "a.md", "## Install\n", "## $T", "### $T");

    ranges(Language::Css, "__fr_pattern { color: FrMetaX; }");
    ranges(Language::Markdown, "[FrMetaT](old/url)");
    ranges(Language::Markdown, "# Title\n\nsee [docs](old/url)\n\n```\n[docs](old/url)\n```\n");
    ranges(Language::Xml, "<c id=\"FrMetaX\">FrMetaT</c>");
    ranges(Language::Bash, "curl FrMetaURL");
    ranges(Language::Yaml, "image: FrMetaX\n");
    ranges(Language::Hcl, "__fr_pattern = var.FrMetaX\n");
    ranges(Language::Helm, "spec:\n  image: {{ .Values.image }}\n  name: web\n");
    ranges(Language::Helm, "a: {{ .V.x }}\nb: plain\n");
}
