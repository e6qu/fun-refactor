//! Extract function: the data-flow half of the extract/inline family.
//!
//! What makes this refactoring hard is not moving the text but working out the
//! interface: which locals flow in as parameters, which flow out as returns, and
//! whether a call can reproduce the region's control flow at all.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::refactor::extract;
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::span::{LineCol, LineIndex, Span};
use std::path::{Path, PathBuf};

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

/// Byte span of whole lines `from..=to` (1-based).
fn lines(source: &str, from: usize, to: usize) -> Span {
    let index = LineIndex::new(source);
    let start = index.line_span(from).unwrap().start;
    let end = index.line_span(to).unwrap().end;
    let _ = index.offset(LineCol { line: from, col: 1 }, source);
    Span::new(start, end)
}

fn apply(plan: &extract::ExtractFunctionPlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
}

#[test]
fn extracts_statements_with_no_inputs_or_outputs() {
    let src = "def main():\n    print(\"one\")\n    print(\"two\")\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 2, 3), "greet").unwrap();
    assert!(plan.parameters.is_empty(), "got {:?}", plan.parameters);
    assert!(plan.returns.is_empty(), "got {:?}", plan.returns);

    let out = apply(&plan, &path);
    assert!(out.contains("    greet()"), "call replaces the region:\n{out}");
    assert!(out.contains("def greet():"), "definition added:\n{out}");
    assert!(out.contains("print(\"one\")"), "body moved:\n{out}");
}

#[test]
fn locals_read_in_the_region_become_parameters() {
    let src = "def main():\n    width = 3\n    height = 4\n    area = width * height\n    print(area)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    // Extract only the `area = ...` line: it reads width and height.
    let plan = extract::function(&index, &path, lines(src, 4, 4), "compute").unwrap();
    let names: Vec<&str> = plan.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["height", "width"], "got {names:?}");
}

#[test]
fn locals_used_after_the_region_become_returns() {
    let src = "def main():\n    width = 3\n    area = width * 2\n    print(area)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "compute").unwrap();
    assert_eq!(plan.returns, vec!["area".to_string()]);

    let out = apply(&plan, &path);
    assert!(out.contains("area = compute(width)"), "got:\n{out}");
    assert!(out.contains("return area"), "got:\n{out}");
}

#[test]
fn a_local_not_used_afterwards_is_not_returned() {
    let src = "def main():\n    tmp = 1\n    print(tmp)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    // Both lines move, so `tmp` never escapes the new function.
    let plan = extract::function(&index, &path, lines(src, 2, 3), "work").unwrap();
    assert!(plan.returns.is_empty(), "got {:?}", plan.returns);
}

#[test]
fn comments_inside_the_region_survive() {
    // gopls is documented to drop comments here; reusing the original bytes keeps them.
    let src = "def main():\n    # explain the next line\n    x = compute()  # trailing\n    print(x)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 2, 3), "prepare").unwrap();
    let out = apply(&plan, &path);
    assert!(out.contains("# explain the next line"), "got:\n{out}");
    assert!(out.contains("# trailing"), "got:\n{out}");
}

#[test]
fn refuses_a_region_containing_a_return() {
    // A call cannot reproduce a return from the enclosing function.
    let src = "def main():\n    if bad():\n        return 1\n    print(2)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let err = extract::function(&index, &path, lines(src, 2, 3), "guard")
        .unwrap_err()
        .to_string();
    assert!(err.contains("return"), "got: {err}");
    assert!(err.contains("cannot"), "got: {err}");
}

#[test]
fn a_loop_carrying_its_own_break_is_extractable() {
    // `break` bound to a loop inside the region does not escape it.
    let src = "def main():\n    for i in items():\n        if done(i):\n            break\n    print(1)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 2, 4), "scan");
    assert!(plan.is_ok(), "should extract: {:?}", plan.err().map(|e| e.to_string()));
}

#[test]
fn refuses_a_name_already_in_use() {
    let src = "def existing():\n    pass\n\ndef main():\n    print(1)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let err = extract::function(&index, &path, lines(src, 5, 5), "existing").unwrap_err();
    assert!(
        err.downcast_ref::<fun_refactor::refactor::Refusal>()
            .is_some(),
        "got: {err}"
    );
}

#[test]
fn rust_copies_written_parameter_types() {
    let src = "fn main() {\n    let width: i32 = 3;\n    println!(\"{}\", width * 2);\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "show").unwrap();
    assert_eq!(plan.parameters.len(), 1);
    assert_eq!(plan.parameters[0].name, "width");
    assert_eq!(plan.parameters[0].type_annotation.as_deref(), Some("i32"));

    let out = apply(&plan, &path);
    assert!(out.contains("fn show(width: i32)"), "got:\n{out}");
}

#[test]
fn rust_refuses_when_a_parameter_type_was_never_written() {
    // There is no type inference here, and inventing one would not compile.
    let src = "fn main() {\n    let width = 3;\n    println!(\"{}\", width * 2);\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let err = extract::function(&index, &path, lines(src, 3, 3), "show")
        .unwrap_err()
        .to_string();
    assert!(err.contains("never written down"), "got: {err}");
    assert!(err.contains("width"), "the blocking name should be named: {err}");
}

#[test]
fn refuses_unsupported_languages() {
    let (tmp, index) = workspace(&[("a.sh", "main() {\n  echo hi\n}\n")]);
    let path = tmp.path().join("a.sh");
    let err = extract::function(&index, &path, Span::new(0, 5), "f").unwrap_err();
    assert!(
        err.downcast_ref::<fun_refactor::refactor::Refusal>()
            .is_some_and(|r| matches!(r, fun_refactor::refactor::Refusal::Unsupported { .. })),
        "got: {err}"
    );
}

#[test]
fn the_result_still_parses() {
    let src = "def main():\n    width = 3\n    area = width * 2\n    print(area)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "compute").unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("an extraction must not break the file");
    assert_eq!(outcomes.len(), 1);
    let _ = Path::new(&path);
}

#[test]
fn typescript_extracts_without_type_annotations() {
    let src = "function main() {\n  const w = 3;\n  console.log(w * 2);\n}\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "show").unwrap();
    let out = apply(&plan, &path);
    assert!(out.contains("function show(w)"), "got:\n{out}");
    assert!(out.contains("show(w);"), "got:\n{out}");
}
