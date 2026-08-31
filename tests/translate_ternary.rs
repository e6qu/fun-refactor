//! Go has no conditional expression, and a statement-shaped one lowers.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;

#[test]
fn a_returned_ternary_becomes_a_branching_return() {
    let source = "def pick(flag: bool, a: int, b: int) -> int:\n    return a if flag else b\n";
    let (_tmp, root) = common::tree(&[("t.py", source)]);
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
    let (_tmp, root) = common::tree(&[("a.py", source)]);
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
    let (_tmp, root) = common::tree(&[("c.py", source)]);
    let plan = transpile::plan(&root.join("c.py"), Language::Go).expect("a draft");
    assert!(
        plan.output
            .contains("func() int { if flag { return 1 }; return 2 }()"),
        "the closure carries the branch into the argument:\n{}",
        plan.output
    );
    assert!(!plan.output.contains(transpile::MARKER), "{}", plan.output);
}
