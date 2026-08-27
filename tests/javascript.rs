//! JavaScript, which the TypeScript grammar reads and the extension table did not name.

use fun_refactor::extract::Extractor;
use fun_refactor::index::Index;
use fun_refactor::lang::{detect, Language};
use fun_refactor::parse::Parsers;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::Path;

fn facts(source: &str, lang: Language) -> fun_refactor::model::FileFacts {
    let parsers = Parsers::new();
    let parsed = parsers.parse(lang, source).expect("the grammar loads");
    assert!(
        !parsed.has_errors(),
        "unexpected parse errors: {:?}",
        parsed.error_spans()
    );
    Extractor::new()
        .extract(&parsed, Path::new("t"), source)
        .expect("extraction")
}

#[test]
fn every_javascript_extension_names_a_language() {
    for name in ["a.js", "a.mjs", "a.cjs"] {
        assert_eq!(
            detect(Path::new(name)),
            Some(Language::TypeScript),
            "{name}"
        );
    }
    assert_eq!(detect(Path::new("a.jsx")), Some(Language::Tsx));
}

#[test]
fn a_module_yields_its_definitions_and_uses() {
    let f = facts(
        "import { load } from './store.js';\n\
         export function greet(name) {\n  return load(name);\n}\n",
        Language::TypeScript,
    );
    let symbols: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
    let references: Vec<&str> = f.references.iter().map(|r| r.name.as_str()).collect();
    assert!(symbols.contains(&"greet"), "{symbols:?}");
    assert!(references.contains(&"load"), "{references:?}");
    assert_eq!(f.imports.len(), 1, "{:?}", f.imports);
}

#[test]
fn commonjs_is_read_as_readily_as_modules() {
    // `require` and `module.exports` are not syntax the TypeScript grammar treats specially.
    let f = facts(
        "const fs = require('fs');\nfunction read(p) { return fs.readFileSync(p); }\nmodule.exports = { read };\n",
        Language::TypeScript,
    );
    let symbols: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(symbols.contains(&"read"), "{symbols:?}");
}

#[test]
fn jsx_needs_the_tsx_grammar() {
    let f = facts(
        "export const Card = () => <div className=\"btn\">hi</div>;\n",
        Language::Tsx,
    );
    let symbols: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(symbols.contains(&"Card"), "{symbols:?}");
}

#[test]
fn a_class_used_only_from_javascript_is_not_reported_dead() {
    // The defect that made this worth fixing instead of noting: `fr delete` acts on
    // what `fr unused` says.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("s.css"),
        ".used-from-js { color: red; }\n.nobody-uses-me { color: blue; }\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("app.js"),
        "el.classList.add(\"used-from-js\");\n",
    )
    .unwrap();

    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    assert_eq!(scanned.files.len(), 2, "the .js file must be scanned");

    let index = Index::build_from_scan(&scanned).unwrap();
    let entrypoints = fun_refactor::analysis::entrypoints::Entrypoints::default();
    let dead: Vec<&str> = fun_refactor::refactor::delete::find_unused(&index, &entrypoints)
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(!dead.contains(&"used-from-js"), "{dead:?}");
    assert!(dead.contains(&"nobody-uses-me"), "{dead:?}");
}
