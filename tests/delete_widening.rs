//! How much comes away with a definition.

use fun_refactor::index::Index;
use fun_refactor::refactor::{cascade, delete};
use fun_refactor::scan::ScanOptions;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    (tmp, index)
}

fn deleted(files: &[(&str, &str)], symbol: &str, file: &str) -> String {
    let (tmp, index) = workspace(files);
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == symbol && s.file.ends_with(file))
        .unwrap_or_else(|| panic!("no `{symbol}`"))
        .id;
    let plan = delete::plan(&index, id).expect("a delete plan");
    let path = tmp.path().join(file);
    fun_refactor::edit::apply_to_string(
        &std::fs::read_to_string(&path).expect("the file"),
        plan.edits.edits_for(&path).expect("edits"),
    )
    .expect("applying")
}

#[test]
fn deleting_a_lone_java_field_keeps_the_class() {
    // The methods beside it are a different kind.
    let source = "public class B {\n    static final boolean UNUSED = true;\n\n    \
                  static int f(int x) { return x; }\n}\n";
    let after = deleted(&[("B.java", source)], "UNUSED", "B.java");
    assert!(after.contains("public class B {"), "{after}");
    assert!(after.contains("static int f(int x)"), "{after}");
    assert!(!after.contains("UNUSED"), "{after}");
}

#[test]
fn deleting_the_only_member_of_a_java_class_keeps_the_class() {
    // Nothing remains inside it, and a class with an empty body is still a class.
    let source = "public class C {\n    static final boolean UNUSED = true;\n}\n";
    let after = deleted(&[("C.java", source)], "UNUSED", "C.java");
    assert!(after.contains("public class C {"), "{after}");
    assert!(after.contains('}'), "{after}");
}

#[test]
fn deleting_a_lone_css_selector_still_takes_its_block() {
    let source = ".gone {\n  color: red;\n}\n\n.kept {\n  color: blue;\n}\n";
    let after = deleted(&[("a.css", source)], "gone", "a.css");
    assert!(!after.contains("color: red"), "{after}");
    assert!(after.contains(".kept"), "{after}");
}

#[test]
fn removing_a_typescript_flag_takes_its_whole_declaration() {
    // `fr remove-flag` deleted the symbol's own span, which for TypeScript is the declarator
    // inside the declaration: `NEW_UI = true` out of `const NEW_UI = true;` leaves `const ;`.
    let source = "const NEW_UI = true;\n\nfunction render(x: number): number {\n    \
                  if (NEW_UI) {\n        return 1;\n    }\n    return 2;\n}\n";
    let (tmp, _index) = workspace(&[("c.ts", source)]);
    let plan = cascade::remove_flag(tmp.path(), "NEW_UI", true).expect("a cascade");
    let path = tmp.path().join("c.ts");
    let after = fun_refactor::edit::apply_to_string(source, plan.edits.edits_for(&path).unwrap())
        .expect("applying");
    assert!(!after.contains("NEW_UI"), "{after}");
    assert!(!after.contains("const ;"), "{after}");
    assert!(after.contains("return 1;"), "{after}");
    assert!(!after.contains("if ("), "{after}");
}
