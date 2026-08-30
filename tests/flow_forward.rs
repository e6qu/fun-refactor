//! Where a value goes, asked of the same program in four languages.

use fun_refactor::analysis::flow;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn forward(file: &str, source: &str, symbol: &str) -> Vec<String> {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(tmp.path().join(file), source).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == symbol)
        .unwrap_or_else(|| panic!("no `{symbol}`"))
        .id;
    flow::forward(&index, id, 20)
        .expect("a flow")
        .steps
        .iter()
        .map(|step| step.text.clone())
        .collect()
}

#[test]
fn a_value_is_followed_to_the_end_of_its_chain() {
    for (file, source) in [
        (
            "a.py",
            "def load(raw):\n    cleaned = raw.strip()\n    parsed = int(cleaned)\n    \
             doubled = parsed * 2\n    return doubled\n",
        ),
        (
            "a.rs",
            "pub fn load(raw: String) -> i64 {\n    let cleaned = raw.len();\n    \
             let parsed = cleaned as i64;\n    let doubled = parsed * 2;\n    \
             return doubled;\n}\n",
        ),
        (
            "a.ts",
            "export function load(raw: string): number {\n    const cleaned = raw.length;\n    \
             const parsed = cleaned + 1;\n    const doubled = parsed * 2;\n    \
             return doubled;\n}\n",
        ),
        (
            "a.go",
            "package main\n\nfunc load(raw string) int {\n\tcleaned := len(raw)\n\t\
             parsed := cleaned + 1\n\tdoubled := parsed * 2\n\treturn doubled\n}\n",
        ),
    ] {
        let steps = forward(file, source, "raw");
        for expected in ["cleaned", "parsed", "doubled", "return doubled"] {
            assert!(
                steps.iter().any(|s| s.contains(expected)),
                "{file} never reached `{expected}`: {steps:#?}"
            );
        }
    }
}

#[test]
fn the_value_flows_into_the_binding_and_not_the_function_around_it() {
    // The candidate search accepted any symbol whose span *contained* the assigned name, and
    // took the first in declaration order, which is the enclosing function, whose span contains
    // everything.
    let steps = forward(
        "a.py",
        "def load(raw):\n    cleaned = raw.strip()\n    parsed = int(cleaned)\n    \
         return parsed\n",
        "cleaned",
    );
    assert!(
        steps.iter().any(|s| s.contains("parsed = int(cleaned)")),
        "{steps:#?}"
    );
    assert!(
        !steps.iter().any(|s| s.starts_with("def load")),
        "the report has the value flowing into the function around it: {steps:#?}"
    );
}

#[test]
fn a_hop_is_reported_once() {
    // The use and the binding it initialises are the same line.
    let steps = forward(
        "a.py",
        "def load(raw):\n    cleaned = raw.strip()\n    parsed = int(cleaned)\n    \
         return parsed\n",
        "raw",
    );
    let repeated = steps
        .iter()
        .filter(|s| s.contains("cleaned = raw.strip()"))
        .count();
    assert_eq!(repeated, 1, "{steps:#?}");
}
