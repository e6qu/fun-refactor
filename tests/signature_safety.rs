//! Signature edits preserve binding and evaluation, not merely parseable syntax.

use fun_refactor::{
    edit::{apply_to_string, EditSet},
    index::Index,
    refactor::signature::{self, Change},
    scan::ScanOptions,
};
use std::{fs, path::Path, process::Command};

fn workspace(file: &str, source: &str) -> (tempfile::TempDir, Index) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(file), source).unwrap();
    let index = Index::build(dir.path(), &ScanOptions::default()).unwrap();
    (dir, index)
}

fn changed(index: &Index, change: Change) -> anyhow::Result<signature::SignaturePlan> {
    let id = index.find_symbols("adjust", None)[0].id;
    signature::change(index, id, change)
}

fn apply(root: &Path, file: &str, edits: &EditSet) -> String {
    let path = root.join(file);
    let before = fs::read_to_string(&path).unwrap();
    apply_to_string(&before, edits.edits_for(&path).unwrap()).unwrap()
}

fn output(program: &str, source: &str) -> Vec<u8> {
    let result = Command::new(program).args(["-c", source]).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    result.stdout
}

#[test]
fn nonadjacent_python_moves_preserve_binding_and_intervening_order() {
    let source = "def adjust(a, b, c, d):\n    return a * 1000 + b * 100 + c * 10 + d\nprint(adjust(1, 2, 3, 4))\n";
    for (from, to, declaration, call) in [
        (0, 2, "def adjust(b, c, a, d)", "adjust(2, 3, 1, 4)"),
        (3, 0, "def adjust(d, a, b, c)", "adjust(4, 1, 2, 3)"),
    ] {
        let (dir, index) = workspace("subject.py", source);
        let plan = changed(&index, Change::Move { from, to }).unwrap();
        let after = apply(dir.path(), "subject.py", &plan.edits);
        assert!(after.contains(declaration), "{after}");
        assert!(after.contains(call), "{after}");
        assert_eq!(output("python3", source), output("python3", &after));
    }
}

#[test]
fn python_removal_follows_keyword_binding_and_keeps_retained_effects() {
    for source in [
        "def adjust(a=1, unused=0, c=3):\n    return a + c\nprint(adjust(unused=2))\n",
        "def adjust(a, unused, c):\n    return a + c\nprint(adjust(1, 2, c=3))\n",
        "def adjust(a, unused=0, c=3):\n    return a + c\nprint(adjust(c=3, unused=2, a=1))\n",
        "def record():\n    print('called')\n    return 1\ndef adjust(a, unused=0, c=3):\n    return a + c\nprint(adjust(record(), 2, c=3))\n",
    ] {
        let (dir, index) = workspace("subject.py", source);
        let plan = changed(&index, Change::Remove(1)).unwrap();
        let after = apply(dir.path(), "subject.py", &plan.edits);
        assert!(!after.contains("unused"), "{after}");
        assert_eq!(output("python3", source), output("python3", &after));
    }
}

#[test]
fn effectful_removal_refuses_in_each_programming_language() {
    for (file, source) in [
        ("a.rs", "fn adjust(unused: i32) {}\nfn record() -> i32 { 1 }\nfn run() { adjust(record()); }\n"),
        ("a.py", "def adjust(unused):\n    pass\ndef record():\n    return 1\nadjust(record())\n"),
        ("a.ts", "function adjust(unused: number) {}\nfunction record() { return 1; }\nadjust(record());\n"),
        ("a.tsx", "function adjust(unused: number) {}\nfunction record() { return 1; }\nadjust(record());\nconst view = <div/>;\n"),
        ("a.go", "package main\nfunc adjust(unused int) {}\nfunc record() int { return 1 }\nfunc main() { adjust(record()) }\n"),
        ("A.java", "class A { static void adjust(int unused) {} static int record() { return 1; } static void run() { adjust(record()); } }\n"),
        ("a.zig", "fn adjust(unused: i32) void {}\nfn record() i32 { return 1; }\nfn run() void { adjust(record()); }\n"),
    ] {
        let (dir, index) = workspace(file, source);
        let error = changed(&index, Change::Remove(0)).unwrap_err().to_string();
        assert!(error.contains("evaluation"), "{file}: {error}");
        assert_eq!(fs::read_to_string(dir.path().join(file)).unwrap(), source);
    }
}

#[test]
fn moving_across_an_effectful_middle_argument_refuses() {
    let source = "def adjust(a, b, c):\n    return a + b + c\ndef record():\n    return 2\nadjust(1, record(), 3)\n";
    let (_dir, index) = workspace("a.py", source);
    let error = changed(&index, Change::Move { from: 0, to: 2 }).unwrap_err().to_string();
    assert!(error.contains("evaluation"), "{error}");
}

#[test]
fn omitted_defaults_cannot_silently_change_positions() {
    let source = "def adjust(a=1, b=2, c=3):\n    return a * 100 + b * 10 + c\nprint(adjust(9))\n";
    let (_dir, index) = workspace("a.py", source);
    let error = changed(&index, Change::Move { from: 0, to: 2 }).unwrap_err().to_string();
    assert!(error.contains("default binding"), "{error}");
}

#[test]
fn removed_defaults_and_annotations_cannot_hide_effects() {
    for parameter in ["unused=record()", "unused: record() = 0"] {
        let source = format!("def record():\n    print('effect')\n    return int\ndef adjust({parameter}):\n    return 1\nprint(adjust())\n");
        let (_dir, index) = workspace("a.py", &source);
        let error = changed(&index, Change::Remove(0)).unwrap_err().to_string();
        assert!(error.contains("evaluation") || error.contains("annotation"), "{error}");
    }
}

#[test]
fn variadic_and_keyword_only_signatures_refuse_positional_edits() {
    for parameters in ["*args", "**kwargs", "a, *, b=0", "a, /, b=0"] {
        let source = format!("def adjust({parameters}):\n    return 1\n");
        let (_dir, index) = workspace("a.py", &source);
        let error = changed(&index, Change::Remove(0)).unwrap_err().to_string();
        assert!(error.contains("bindings"), "{parameters}: {error}");
    }
}

#[test]
fn rust_macro_calls_use_the_same_evaluation_rule() {
    for argument in ["record()", "{ record(); 0 }"] {
        let source = format!("fn adjust(unused: i32) -> i32 {{ 1 }}\nfn record() -> i32 {{ 0 }}\nfn run() {{ assert_eq!(adjust({argument}), 1); }}\n");
        let (_dir, index) = workspace("a.rs", &source);
        let error = changed(&index, Change::Remove(0)).unwrap_err().to_string();
        assert!(error.contains("evaluation"), "{error}");
    }
    let source = "fn adjust(unused: i32) -> i32 { 1 }\nfn run() { assert_eq!(adjust(-2), 1); }\n";
    let (dir, index) = workspace("a.rs", source);
    let plan = changed(&index, Change::Remove(0)).unwrap();
    assert!(apply(dir.path(), "a.rs", &plan.edits).contains("assert_eq!(adjust(), 1)"));
}

#[test]
fn shell_moves_shift_positions_and_preserve_output() {
    let source = "adjust() { printf '%s:%s:%s\\n' \"$1\" \"$2\" \"$3\"; }\nadjust alpha beta gamma\n";
    for (from, to) in [(0, 2), (2, 0)] {
        let (dir, index) = workspace("a.sh", source);
        let plan = changed(&index, Change::Move { from, to }).unwrap();
        let after = apply(dir.path(), "a.sh", &plan.edits);
        assert_eq!(output("bash", source), output("bash", &after));
    }
}

#[test]
fn quoted_shell_substitutions_keep_their_effects() {
    for argument in ["\"$(printf effect)\"", "\"$((counter += 1))\"", "\"${value:=changed}\""] {
        let source = format!("adjust() {{ printf '%s' \"$1\"; }}\nadjust kept {argument}\n");
        let (_dir, index) = workspace("a.sh", &source);
        let error = changed(&index, Change::Remove(1)).unwrap_err().to_string();
        assert!(error.contains("evaluation"), "{error}");
    }
}

#[test]
fn added_parameters_cannot_capture_existing_names_or_ignore_positions() {
    let source = "LIMIT = 4\ndef adjust(a):\n    return a + LIMIT\nprint(adjust(1))\n";
    let (_dir, index) = workspace("a.py", source);
    let error = changed(&index, Change::Add { at: 1, declaration: "LIMIT".into(), argument: "0".into() }).unwrap_err().to_string();
    assert!(error.contains("means something else"), "{error}");
    let error = changed(&index, Change::Add { at: 4, declaration: "unused".into(), argument: "0".into() }).unwrap_err().to_string();
    assert!(error.contains("position 4"), "{error}");
}

#[test]
fn removing_a_commented_argument_keeps_a_valid_separator() {
    let source = "fn adjust(a: i32, /* unused */ b: i32, c: i32) -> i32 { a + c }\nfn run() { adjust(1, /* drop */ 2, 3); }\n";
    let (dir, index) = workspace("a.rs", source);
    let plan = changed(&index, Change::Remove(1)).unwrap();
    let after = apply(dir.path(), "a.rs", &plan.edits);
    assert!(after.contains("/* unused */"));
    assert!(after.contains("/* drop */"));
    fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict).unwrap();
}
