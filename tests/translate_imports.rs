//! Names the output uses that the target has to be told about.
//!
//! A translated file that names a type nobody imported does not compile, and the report
//! calls the signature "carried across with its types intact", which it was. The
//! import is a separate thing the writer has to say, and two writers were not saying it.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(file: &str, source: &str, to: Language) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, source).expect("the file");
    transpile::plan(&path, to).expect("a translation").output
}

#[test]
fn java_imports_the_list_it_names() {
    let out = translated(
        "a.rs",
        "pub fn total(items: Vec<i64>) -> i64 {\n    return 0;\n}\n",
        Language::Java,
    );
    assert!(out.contains("import java.util.List;"), "{out}");
    assert!(out.contains("List<Integer> items"), "{out}");
}

#[test]
fn java_imports_the_map_it_names() {
    let out = translated(
        "a.py",
        "def m(units: dict[str, str]) -> str:\n    return \"\"\n",
        Language::Java,
    );
    assert!(out.contains("import java.util.Map;"), "{out}");
}

#[test]
fn java_imports_the_optional_it_names() {
    let out = translated(
        "a.py",
        "from typing import Optional\n\ndef f(x: str) -> Optional[str]:\n    return x\n",
        Language::Java,
    );
    assert!(out.contains("import java.util.Optional;"), "{out}");
}

#[test]
fn java_imports_a_type_only_a_literal_names() {
    // `List.of(…)` is how this writer spells a list literal, so the name is used even
    // where no signature mentions it.
    let out = translated(
        "a.py",
        "def f() -> int:\n    xs = [1, 2, 3]\n    return 0\n",
        Language::Java,
    );
    assert!(out.contains("List.of("), "{out}");
    assert!(out.contains("import java.util.List;"), "{out}");
}

#[test]
fn java_imports_nothing_it_does_not_name() {
    // An import nobody needs is noise, and noise is how a real one gets missed.
    let out = translated(
        "a.rs",
        "pub fn n(a: i64) -> i64 {\n    return a;\n}\n",
        Language::Java,
    );
    assert!(!out.contains("import java.util"), "{out}");
}

#[test]
fn objects_equals_needs_no_import() {
    // Written out in full at its use site, which is why it is spelled that way.
    let out = translated(
        "a.rs",
        "pub fn same(a: String, b: String) -> bool {\n    return a == b;\n}\n",
        Language::Java,
    );
    assert!(out.contains("java.util.Objects.equals"), "{out}");
    assert!(!out.contains("import java.util"), "{out}");
}
