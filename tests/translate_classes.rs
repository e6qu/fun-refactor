//! A Python class crosses with the fields its `__init__` declares.
//!
//! `self.name = name` declares a field as surely as an annotation does. Read as
//! nothing, every class crossed as an empty struct while its methods went on
//! reading `self.price` from a field the target never had, and `Item(...)` stayed
//! a bare call no target accepts.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

const ITEM_PY: &str = "class Item:\n    def __init__(self, name: str, price: float):\n        \
    self.name = name\n        self.price = price\n\n    def cost(self) -> float:\n        \
    return self.price * 2\n\n\ndef total() -> float:\n    it = Item(\"apple\", 1.5)\n    \
    return it.cost()\n";

fn translated(dir: &Path, target: Language) -> String {
    let path = dir.join("item.py");
    std::fs::write(&path, ITEM_PY).unwrap();
    let out = dir.join(format!("out_{target:?}")).with_extension("txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn the_init_assignments_become_rust_fields_and_a_real_constructor() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), Language::Rust);
    assert!(
        out.contains("pub name: String") && out.contains("pub price: f64"),
        "the fields exist, typed from the parameters:\n{out}"
    );
    assert!(
        out.contains("return Item { name: name, price: price };"),
        "the constructor builds the record instead of todo!():\n{out}"
    );
    assert!(!out.contains("todo!"), "nothing is left unwritten:\n{out}");
}

#[test]
fn construction_is_spelled_the_typescript_way() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), Language::TypeScript);
    assert!(
        out.contains("new Item(\"apple\", 1.5)"),
        "a call to the module's own class is a construction:\n{out}"
    );
    assert!(
        out.contains("name: string;") && out.contains("price: number;"),
        "the class declares its fields:\n{out}"
    );
}

#[test]
fn a_computing_constructor_keeps_its_body() {
    // Only a constructor that does nothing but assign becomes the canonical
    // build-and-return; one that computes keeps its statements, whatever the
    // target then says about them.
    let source = "class Timer:\n    def __init__(self, seconds: int):\n        \
        self.seconds = seconds\n        print(seconds)\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("timer.py");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("timer_out.txt");
    let plan = transpile::plan_to(&path, Language::TypeScript, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("seconds: number;"),
        "the field still derives:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("print(seconds)"),
        "the computing statement survives where the target can keep it:\n{}",
        plan.output
    );
}

#[test]
fn a_lost_supertype_is_said_in_the_output_itself() {
    // The report already named the dropped `extends`; the draft file did not, and
    // the draft file is what a reader has in front of them.
    let source = "export class Repo {\n    find(id: number): number {\n        return id;\n    }\n}\n\n\
        export class TaskRepo extends Repo {\n    close(id: number): number {\n        \
        return this.find(id);\n    }\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("repo.ts");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("repo_out.txt");
    let plan = transpile::plan_to(&path, Language::Rust, Some(&out), false).unwrap();
    assert!(
        plan.output
            .contains("not translated: extends Repo"),
        "the marker sits beside the type that lost its base:\n{}",
        plan.output
    );
}
