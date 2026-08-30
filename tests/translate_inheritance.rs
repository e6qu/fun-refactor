//! What a class inherits crosses, or the report names it.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(source: &str, name: &str, target: Language) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("out.txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

const MI_PY: &str = "class Taxed:\n    def cost(self) -> int:\n        return 10\n\n\n\
    class Levied:\n    def levy(self) -> int:\n        return 5\n\n\n\
    class Import(Taxed, Levied):\n    def cost(self) -> int:\n        \
    return super().cost() + 100\n";

#[test]
fn the_first_base_rides_and_the_rest_are_named() {
    let ts = translated(MI_PY, "mi.py", Language::TypeScript);
    assert!(
        ts.contains("class Import extends Taxed {"),
        "the base `super()` dispatches to carries.\n{ts}"
    );
    assert!(
        ts.contains("Levied"),
        "the notes name the base that could not carry.\n{ts}"
    );
    assert!(
        ts.contains("super.cost()"),
        "the call keeps a base to reach.\n{ts}"
    );
}

#[test]
fn a_single_base_still_carries_without_a_note() {
    let source = "class Base:\n    def cost(self) -> int:\n        return 1\n\n\n\
        class Only(Base):\n    def cost(self) -> int:\n        return super().cost() + 1\n";
    let ts = translated(source, "one.py", Language::TypeScript);
    assert!(
        ts.contains("class Only extends Base {"),
        "one base needs no apology.\n{ts}"
    );
    assert!(
        !ts.contains("one base is all that carries"),
        "nothing dropped, so the notes stay quiet.\n{ts}"
    );
}
