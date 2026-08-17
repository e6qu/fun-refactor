//! Keyword arguments settle into their declared positions.
//!
//! Five of these languages call by position alone. When the callee is a
//! function declared in the same module, each keyword names a parameter.
//! The arguments settle into the declared order, defaults filling any gap.
//! A callee declared elsewhere, an unknown keyword, or a hole with no
//! default keeps the call carried. Reordering it would be a guess.

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
fn keywords_settle_into_declared_positions() {
    let source = "def greet(name: str, punct: str) -> str:\n    return name + punct\n\n\n\
                  def main() -> str:\n    return greet(punct=\"?\", name=\"hi\")\n";
    let (_tmp, root) = workspace(&[("kw.py", source)]);
    for to in [Language::Rust, Language::TypeScript, Language::Java] {
        let plan = transpile::plan(&root.join("kw.py"), to).expect("a draft");
        assert!(
            plan.output.contains("greet(\"hi\", \"?\")"),
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
    let (_tmp, root) = workspace(&[("d.py", source)]);
    let plan = transpile::plan(&root.join("d.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("greet(\"hi\", \"!\")"),
        "the declaration's own default fills the position:\n{}",
        plan.output
    );
}

#[test]
fn a_keyword_for_an_unknown_callee_still_carries() {
    let source = "def main() -> str:\n    return elsewhere(punct=\"?\")\n";
    let (_tmp, root) = workspace(&[("u.py", source)]);
    let plan = transpile::plan(&root.join("u.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "a callee declared elsewhere cannot be reordered on a guess:\n{}",
        plan.output
    );
}

#[test]
fn a_hole_with_no_default_still_carries() {
    let source = "def greet(name: str, punct: str) -> str:\n    return name + punct\n\n\n\
                  def main() -> str:\n    return greet(punct=\"?\")\n";
    let (_tmp, root) = workspace(&[("h.py", source)]);
    let plan = transpile::plan(&root.join("h.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "an unfilled position with no default cannot be invented:\n{}",
        plan.output
    );
}
