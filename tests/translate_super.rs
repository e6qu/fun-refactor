//! `super` and the exception bases speak the target, both ways.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

const MODELS_PY: &str = "class InventoryError(Exception):\n    \
    def __init__(self, message: str) -> None:\n        super().__init__(message)\n        \
    self.message = message\n";

const MODELS_TS: &str = "export class InventoryError extends Error {\n    \
    constructor(message: string) {\n        super(message);\n    }\n}\n";

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let out = dir.join(format!("out_{target:?}")).with_extension("txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn a_python_exception_class_extends_typescripts_error_and_calls_super() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "models.py", MODELS_PY, Language::TypeScript);
    assert!(
        out.contains("export class InventoryError extends Error {"),
        "`Exception` is the base `Error` here.\n{out}"
    );
    assert!(
        out.contains("super(message);"),
        "the base constructor call is the keyword's own form.\n{out}"
    );
    assert!(
        !out.contains("super_"),
        "no escaped name stands where the keyword belongs.\n{out}"
    );
}

#[test]
fn a_python_super_method_call_is_typescripts_super_dot() {
    let source = "class Base:\n    def label(self) -> str:\n        return \"base\"\n\n\n\
        class Child(Base):\n    def label(self) -> str:\n        \
        return super().label() + \"!\"\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "labels.py", source, Language::TypeScript);
    assert!(
        out.contains("`${super.label()}!`"),
        "a base method is reached through the keyword.\n{out}"
    );
}

#[test]
fn a_python_abc_base_is_dropped_with_a_note_and_the_methods_stay() {
    let source = "from abc import ABC, abstractmethod\n\n\nclass Shape(ABC):\n    \
        def __init__(self, name: str) -> None:\n        self.name = name\n\n    \
        @abstractmethod\n    def area(self) -> float:\n        ...\n\n    \
        def label(self) -> str:\n        return \"shape:\" + self.name\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("shape.py");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("shape_out.txt");
    let plan = transpile::plan_to(&path, Language::TypeScript, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("export class Shape {"),
        "TypeScript has no abstract base classes, so the base goes.\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("label(): string {"),
        "and the methods stay.\n{}",
        plan.output
    );
    assert!(
        plan.fidelity.notes.iter().any(|n| n.contains("ABC")),
        "the dropped base is in the report.\n{:?}",
        plan.fidelity.notes
    );
}

#[test]
fn a_typescript_error_subclass_comes_home_as_a_python_exception() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "models.ts", MODELS_TS, Language::Python);
    assert!(
        out.contains("class InventoryError(Exception):"),
        "`Error` is the base `Exception` here.\n{out}"
    );
    assert!(
        out.contains("super().__init__(message)"),
        "the base constructor call is this language's own spelling.\n{out}"
    );
    assert!(
        !out.contains("raise NotImplementedError"),
        "a constructor whose body was the super call is not an empty body.\n{out}"
    );
}

#[test]
fn a_typescript_super_method_call_comes_home_as_pythons() {
    let source =
        "export class Base {\n    label(): string {\n        return \"base\";\n    }\n}\n\n\
        export class Child extends Base {\n    label(): string {\n        \
        return super.label() + \"!\";\n    }\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "labels.ts", source, Language::Python);
    assert!(
        out.contains("f\"{super().label()}!\""),
        "the reach through the base keeps its meaning.\n{out}"
    );
}

#[test]
fn the_no_inheritance_targets_carry_the_super_call_visibly() {
    // The override shadows the base's method when the base lays flat, so the reach through
    // `super` has no method left to land on.
    let source = "class Base:\n    def label(self) -> str:\n        return \"base\"\n\n\n\
        class Child(Base):\n    def label(self) -> str:\n        \
        return super().label() + \"!\"\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "labels.py", source, Language::Go);
    assert!(
        out.contains("fun-refactor: not translated: super.label()"),
        "Go has no base to reach, and the loss is inline where it happened.\n{out}"
    );
}
