//! The program's own entry crosses, and so do the shapes around it.
//!
//! `main();` at the bottom of a TypeScript file is the program. Dropped as an
//! unsupported construct, the translated file parsed, ran and printed
//! nothing. A field's initializer and a returned object literal belong to the
//! same story. Without them the dataclass could not construct, and the caller
//! read attributes off a plain dict.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

const TASKS_TS: &str =
    "export class Summary {\n    open: number = 0;\n    closed: number = 0;\n}\n\n\
    export class Repo {\n    rows: number[] = [];\n\n    add(row: number): void {\n        \
    this.rows.push(row);\n    }\n\n    summarize(): Summary {\n        \
    return { open: this.rows.length, closed: 0 };\n    }\n}\n\n\
    function main(): void {\n    const repo = new Repo();\n    repo.add(4);\n}\n\nmain();\n";

fn to_python(source: &str, name: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("out.txt");
    transpile::plan_to(&path, Language::Python, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn the_entry_statement_survives_under_pythons_own_guard() {
    let out = to_python(TASKS_TS, "tasks.ts");
    assert!(
        out.contains("if __name__ == \"__main__\":") && out.contains("main()"),
        "the program still runs:\n{out}"
    );
    assert!(
        !out.contains("not translated: expression_statement"),
        "the entry is not a gap:\n{out}"
    );
}

#[test]
fn a_field_initializer_becomes_a_default_the_dataclass_accepts() {
    let out = to_python(TASKS_TS, "tasks.ts");
    assert!(
        out.contains("rows: list[float] = field(default_factory=list)"),
        "a mutable default builds per instance.\n{out}"
    );
    assert!(
        out.contains("from dataclasses import dataclass, field"),
        "and the import follows:\n{out}"
    );
}

#[test]
fn a_returned_object_literal_builds_the_record_the_signature_promised() {
    let out = to_python(TASKS_TS, "tasks.ts");
    assert!(
        out.contains("return Summary(open="),
        "the caller gets attributes, the type the signature named:\n{out}"
    );
}

#[test]
fn a_python_main_guard_crosses_as_the_entry_it_is() {
    let source =
        "def main() -> None:\n    print(\"run\")\n\n\nif __name__ == \"__main__\":\n    main()\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("app.py");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("app_out.txt");
    let plan = transpile::plan_to(&path, Language::TypeScript, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("main();"),
        "the guarded call becomes the module's own entry:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains("not translated: if_statement"),
        "the guard is not a gap:\n{}",
        plan.output
    );
}
