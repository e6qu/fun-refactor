//! Changing a signature in Java, which spells nothing the way the other five do.

use fun_refactor::index::Index;
use fun_refactor::refactor::signature::{self, Change};
use fun_refactor::scan::ScanOptions;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).expect("writing the file");
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn changed(files: &[(&str, &str)], symbol: &str, file: &str, change: Change) -> String {
    let (_tmp, root) = workspace(files);
    let index = Index::build(&root, &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.qualified_name() == symbol && s.file.ends_with(file))
        .unwrap_or_else(|| panic!("no `{symbol}` in {file}"))
        .id;
    let plan = signature::change(&index, id, change).expect("a signature change");
    let path = root.join(file);
    fun_refactor::edit::apply_to_string(
        &std::fs::read_to_string(&path).expect("the file"),
        plan.edits.edits_for(&path).expect("edits"),
    )
    .expect("applying")
}

#[test]
fn a_java_call_is_a_method_invocation() {
    // The lookup matched on `kind().contains("call")`, and Java spells a call a
    // `method_invocation`.
    let source = "public class A {\n    int add(int a, String b) { return a; }\n    \
                  int use() { return add(1, \"x\"); }\n}\n";
    let after = changed(
        &[("A.java", source)],
        "A::add",
        "A.java",
        Change::Move { from: 0, to: 1 },
    );
    assert!(after.contains("int add(String b, int a)"), "{after}");
    assert!(after.contains("return add(\"x\", 1);"), "{after}");
}

#[test]
fn a_constructor_call_is_a_call_whatever_it_was_written_down_as() {
    // Extraction records `new Thing(1, "x")` as a reference to the *type*, which it also is, so
    // filtering on the recorded kind skipped it.
    let source = "public class B {\n    B(int a, String b) { }\n    \
                  static B make() { return new B(1, \"x\"); }\n}\n";
    let after = changed(
        &[("B.java", source)],
        "B::B",
        "B.java",
        Change::Move { from: 0, to: 1 },
    );
    assert!(after.contains("B(String b, int a)"), "{after}");
    assert!(after.contains("new B(\"x\", 1)"), "{after}");
}

#[test]
fn a_mention_that_is_not_a_call_is_passed_over() {
    // `static B make()` names the type in a return position.
    let source = "public class C {\n    C(int a, String b) { }\n    \
                  static C make() { return new C(1, \"x\"); }\n}\n";
    let after = changed(&[("C.java", source)], "C::C", "C.java", Change::Remove(1));
    assert!(after.contains("static C make()"), "{after}");
    assert!(after.contains("new C(1)"), "{after}");
}

#[test]
fn a_type_named_in_an_argument_is_not_the_call() {
    // Dropping the kind filter meant every mention took the walk up the tree looking for a
    // call.
    let source = "public class D {\n    D(int a, String b) { }\n    \
                  static void go() { register(D.class, 7); }\n    \
                  static void register(Object t, int n) { }\n}\n";
    let after = changed(
        &[("D.java", source)],
        "D::D",
        "D.java",
        Change::Move { from: 0, to: 1 },
    );
    assert!(after.contains("D(String b, int a)"), "{after}");
    assert!(after.contains("register(D.class, 7)"), "{after}");
}
