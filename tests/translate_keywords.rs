//! Keyword arguments settle into their declared positions.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;

#[test]
fn keywords_settle_into_declared_positions() {
    let source = "def greet(name: str, punct: str) -> str:\n    return name + punct\n\n\n\
                  def main() -> str:\n    return greet(punct=\"?\", name=\"hi\")\n";
    let (_tmp, root) = common::tree(&[("kw.py", source)]);
    let spelled = [
        (
            Language::Rust,
            "greet(\"hi\".to_string(), \"?\".to_string())",
        ),
        (Language::TypeScript, "greet(\"hi\", \"?\")"),
        (Language::Java, "greet(\"hi\", \"?\")"),
    ];
    for (to, expected) in spelled {
        let plan = transpile::plan(&root.join("kw.py"), to).expect("a draft");
        assert!(
            plan.output.contains(expected),
            "{to} did not settle the keywords:\n{}",
            plan.output
        );
        assert!(
            !plan.output.contains(transpile::MARKER),
            "{to} carried a call it can settle:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_declared_default_fills_the_gap() {
    let source = "def greet(name: str, punct: str = \"!\") -> str:\n    return name + punct\n\n\n\
                  def main() -> str:\n    return greet(name=\"hi\")\n";
    let (_tmp, root) = common::tree(&[("d.py", source)]);
    let plan = transpile::plan(&root.join("d.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output
            .contains("greet(\"hi\".to_string(), \"!\".to_string())"),
        "the declaration's own default fills the position:\n{}",
        plan.output
    );
}

#[test]
fn a_keyword_for_an_unknown_callee_passes_by_position_and_says_so() {
    // Nothing here can check a foreign signature, so the value crosses in the
    // order written and the note says the name had nowhere to go.
    let source = "def main() -> str:\n    return elsewhere(punct=\"?\")\n";
    let (_tmp, root) = common::tree(&[("u.py", source)]);
    let plan = transpile::plan(&root.join("u.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("elsewhere(\"?\")"),
        "the value still crosses, positionally:\n{}",
        plan.output
    );
    assert!(
        plan.fidelity
            .notes
            .iter()
            .any(|n| n.contains("named argument passes by position")),
        "the changed call shape is said out loud:\n{:?}",
        plan.fidelity.notes
    );
}

#[test]
fn a_hole_with_no_default_still_carries() {
    let source = "def greet(name: str, punct: str) -> str:\n    return name + punct\n\n\n\
                  def main() -> str:\n    return greet(punct=\"?\")\n";
    let (_tmp, root) = common::tree(&[("h.py", source)]);
    let plan = transpile::plan(&root.join("h.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "an unfilled position with no default cannot be invented:\n{}",
        plan.output
    );
}
