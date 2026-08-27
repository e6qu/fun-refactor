//! What a declaration binds, asked of every grammar that spells it differently.

use fun_refactor::analysis::types;
use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn index_of(root: &Path) -> Index {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    Index::build_from_scan(&scanned).expect("index")
}

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

fn applied(root: &Path, file: &str, edits: &fun_refactor::edit::EditSet) -> String {
    let path = root.join(file);
    let before = std::fs::read_to_string(&path).expect("read");
    match edits.edits_for(&path) {
        Some(for_file) => {
            fun_refactor::edit::apply_to_string(&before, for_file).expect("the edits apply")
        }
        None => before,
    }
}

/// Every language's way of writing "this local holds the result of that call", and what
/// each has to inline to.
fn locals_bound_to_a_call() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "a.rs",
            "fn f() -> usize {\n    let total = g();\n    total * 2\n}\n\
             fn g() -> usize {\n    1\n}\n",
            "g() * 2",
        ),
        (
            "b.go",
            "package b\n\nfunc F() int {\n\ttotal := G()\n\treturn total * 2\n}\n\n\
             func G() int {\n\treturn 1\n}\n",
            "G() * 2",
        ),
        (
            "c.ts",
            "export function f(): number {\n  const total = g();\n  return total * 2;\n}\n\
             export function g(): number {\n  return 1;\n}\n",
            "g() * 2",
        ),
        (
            "d.py",
            "def f():\n    total = g()\n    return total * 2\n\n\ndef g():\n    return 1\n",
            "g() * 2",
        ),
        (
            "e.zig",
            "pub fn f() usize {\n    const total = g();\n    return total * 2;\n}\n\
             pub fn g() usize {\n    return 1;\n}\n",
            "g() * 2",
        ),
        // The one that was refused outright: Java puts the name and the value together in
        // a declarator, because one statement may declare several.
        (
            "F.java",
            "public class F {\n    static int f() {\n        int total = g();\n        \
             return total * 2;\n    }\n\n    static int g() {\n        return 1;\n    }\n}\n",
            "g() * 2",
        ),
    ]
}

#[test]
fn a_local_bound_to_a_call_inlines_in_every_language() {
    for (name, source, expected) in locals_bound_to_a_call() {
        let (_tmp, root) = workspace(&[(name, source)]);
        let index = index_of(&root);
        let plan = fun_refactor::refactor::inline::variable(&index, symbol(&index, "total"))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let out = applied(&root, name, &plan.edits);
        assert!(
            out.contains(expected),
            "{name} should have inlined to `{expected}`:\n{out}"
        );
    }
}

#[test]
fn a_java_local_declaring_several_names_keeps_the_others() {
    // The reason the value hangs off a declarator in the first place.
    let (_tmp, root) = workspace(&[(
        "M.java",
        "public class M {\n    static int f() {\n        int a = 1, b = 2, c = 3;\n        \
         return a + b + c;\n    }\n}\n",
    )]);
    let index = index_of(&root);
    let plan =
        fun_refactor::refactor::inline::variable(&index, symbol(&index, "b")).expect("a plan");
    let out = applied(&root, "M.java", &plan.edits);
    assert!(out.contains("a + 2 + c"), "got:\n{out}");
    assert!(
        out.contains("int a = 1") && out.contains("c = 3"),
        "the other two declarations stay:\n{out}"
    );
}

#[test]
fn a_keyword_that_means_work_it_out_is_not_a_type() {
    // `var` is Java's way of writing no type at all.
    let (_tmp, root) = workspace(&[(
        "V.java",
        "public class V {\n    static String describe() {\n        var total = compute();\n        \
         return String.valueOf(total);\n    }\n\n    static int compute() {\n        \
         return 1;\n    }\n}\n",
    )]);
    let index = index_of(&root);
    let answer = types::of(&index, symbol(&index, "total")).expect("an answer");

    assert_eq!(
        answer.declared, None,
        "`var` states nothing, so nothing is what was declared"
    );
    let inferred = answer.inferred.expect("the call states what it returns");
    assert_eq!(inferred.ty, "int");
    assert_eq!(inferred.basis, types::Basis::ReturnOfCall);
}

#[test]
fn a_java_construction_names_the_class_it_builds() {
    // Java spells a call `method_invocation` and a construction `object_creation_expression`,
    // and names the callee `name` and `type` where every other grammar here says `function`.
    let (_tmp, root) = workspace(&[(
        "W.java",
        "public class W {\n    static class Money {}\n\n    static void f() {\n        \
         var m = new Money();\n        System.out.println(m);\n    }\n}\n",
    )]);
    let index = index_of(&root);
    let answer = types::of(&index, symbol(&index, "m")).expect("an answer");
    let inferred = answer.inferred.expect("the class constructed is the type");
    assert_eq!(inferred.ty, "Money");
    assert_eq!(inferred.basis, types::Basis::Construction);
}

#[test]
fn a_declaration_that_binds_nothing_says_so() {
    // The refusal this reader is allowed to give, and the one it was giving wrongly.
    let (_tmp, root) = workspace(&[(
        "D.java",
        "public class D {\n    static int f(boolean cond) {\n        int total;\n        \
         if (cond) {\n            total = 1;\n        } else {\n            total = 2;\n        \
         }\n        return total;\n    }\n}\n",
    )]);
    let index = index_of(&root);
    let error = fun_refactor::refactor::inline::variable(&index, symbol(&index, "total"))
        .expect_err("there is genuinely nothing bound here")
        .to_string();
    assert!(error.contains("no initialiser"), "{error}");
}
