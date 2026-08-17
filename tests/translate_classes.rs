//! A Python class crosses with the fields its `__init__` declares.
//!
//! `self.name = name` declares a field as surely as an annotation does. Read
//! as nothing, every class crossed as an empty struct while its methods went
//! on reading `self.price` from a field the target never had. `Item(...)`
//! stayed a bare call no target accepts.

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
        "the fields exist, typed from the parameters.\n{out}"
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
        plan.output.contains("console.log(seconds)"),
        "the computing statement survives, in the target's own spelling:\n{}",
        plan.output
    );
}

#[test]
fn a_lost_supertype_is_said_in_the_output_itself() {
    // The base lives in another module, so nothing can lay it flat. The report
    // already named the loss, and the draft file, the thing a reader has in
    // front of them, says it too.
    let source = "import { Repo } from \"./repo\";\n\n\
        export class TaskRepo extends Repo {\n    close(id: number): number {\n        \
        return id;\n    }\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("repo.ts");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("repo_out.txt");
    let plan = transpile::plan_to(&path, Language::Rust, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("not translated: extends Repo"),
        "the marker sits beside the type that lost its base:\n{}",
        plan.output
    );
}

#[test]
fn a_property_keeps_its_reads_where_the_idiom_exists() {
    // `@property def total` is read as data at every use site. As an ordinary
    // method, `it.total` in the target was the function object, and every
    // comparison against it was quietly false.
    let source = "class Item:\n    def __init__(self, price: float, qty: int):\n        \
        self.price = price\n        self.qty = qty\n\n    @property\n    \
        def total(self) -> float:\n        return self.price * self.qty\n\n\n\
        def expensive(items: list[Item], cutoff: float) -> list[Item]:\n    \
        return [it for it in items if it.total > cutoff]\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("prop.py");
    std::fs::write(&path, source).unwrap();

    let ts = transpile::plan_to(
        &path,
        Language::TypeScript,
        Some(&tmp.path().join("a")),
        false,
    )
    .unwrap()
    .output;
    assert!(
        ts.contains("get total(): number {") && ts.contains("it.total > cutoff"),
        "TypeScript has the idiom and keeps the reads.\n{ts}"
    );

    let rust = transpile::plan_to(&path, Language::Rust, Some(&tmp.path().join("b")), false)
        .unwrap()
        .output;
    assert!(
        rust.contains("it.total() > cutoff"),
        "Rust has no properties, so the read becomes the call it is:\n{rust}"
    );
}

#[test]
fn a_local_base_lays_flat_where_nothing_inherits() {
    // `TaskRepo extends Repo` and `Repo` sits in the same file. Rust has no
    // inheritance, and the marker alone left `self.find(id)` calling a method
    // the struct never had. The base's fields and methods belong to every
    // instance of the extender, so they lay flat into it. The marker stands
    // only for a base the module does not hold.
    let source = "export class Repo {\n    rows: number[] = [];\n\n    \
        find(id: number): number {\n        return id;\n    }\n}\n\n\
        export class TaskRepo extends Repo {\n    close(id: number): number {\n        \
        return this.find(id);\n    }\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("repo.ts");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("repo_out.txt");
    let plan = transpile::plan_to(&path, Language::Rust, Some(&out), false).unwrap();
    assert!(
        plan.output
            .contains("pub struct TaskRepo {\n    pub rows: Vec<f64>,\n}"),
        "the base's field belongs to the extender.\n{}",
        plan.output
    );
    let task_repo = plan.output.split("impl TaskRepo").nth(1).unwrap_or("");
    assert!(
        task_repo.contains("pub fn find(&self, id: f64) -> f64"),
        "and so does its method:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains("not translated: extends Repo"),
        "an absorbed base needs no marker:\n{}",
        plan.output
    );
}

#[test]
fn zig_tests_named_by_declaration_cross_too() {
    let source = "pub fn double(n: u32) u32 {\n    return n * 2;\n}\n\n\
        test double {\n    const four = double(2);\n    _ = four;\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("math.zig");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("math_out.txt");
    let plan = transpile::plan_to(&path, Language::Python, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("def test_double"),
        "the identifier-named test is a test:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains("not translated: test_declaration"),
        "not a gap:\n{}",
        plan.output
    );
}
