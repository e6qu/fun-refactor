//! Go has no conditional expression, and a statement-shaped one lowers.
//!
//! A ternary that is the whole of a return, an assignment, or a typed binding
//! is an `if`/`else` said shorter, so Go writes the `if`/`else`. Each arm
//! renders inside its own branch, which keeps the evaluation the source
//! chose. One buried inside a larger expression still carries. There is no
//! statement to unfold it into.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

#[test]
fn a_returned_ternary_becomes_a_branching_return() {
    let source = "def pick(flag: bool, a: int, b: int) -> int:\n    return a if flag else b\n";
    let (_tmp, root) = workspace(&[("t.py", source)]);
    let plan = transpile::plan(&root.join("t.py"), Language::Go).expect("a draft");
    assert!(plan.output.contains("if flag {"), "{}", plan.output);
    assert!(plan.output.contains("return a"), "{}", plan.output);
    assert!(plan.output.contains("return b"), "{}", plan.output);
    assert!(
        !plan.output.contains(transpile::MARKER),
        "a statement-shaped ternary is not a loss:\n{}",
        plan.output
    );
}

#[test]
fn an_assigned_ternary_branches_the_assignment() {
    let source = "def set_label(flag: bool) -> str:\n    label = \"\"\n    \
                  label = \"on\" if flag else \"off\"\n    return label\n";
    let (_tmp, root) = workspace(&[("a.py", source)]);
    let plan = transpile::plan(&root.join("a.py"), Language::Go).expect("a draft");
    assert!(
        plan.output.contains("label = \"on\"") && plan.output.contains("label = \"off\""),
        "{}",
        plan.output
    );
    assert!(!plan.output.contains(transpile::MARKER), "{}", plan.output);
}

#[test]
fn a_ternary_inside_an_argument_list_runs_in_a_closure() {
    // There is no statement to unfold it into, so a closure gives the `if`
    // somewhere to put its result, right in the argument list.
    let source = "def send(flag: bool) -> int:\n    return post(1 if flag else 2)\n";
    let (_tmp, root) = workspace(&[("c.py", source)]);
    let plan = transpile::plan(&root.join("c.py"), Language::Go).expect("a draft");
    assert!(
        plan.output
            .contains("func() int { if flag { return 1 }; return 2 }()"),
        "the closure carries the branch into the argument:\n{}",
        plan.output
    );
    assert!(!plan.output.contains(transpile::MARKER), "{}", plan.output);
}
