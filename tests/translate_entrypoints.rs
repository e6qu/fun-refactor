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
fn a_java_main_reaches_python_with_the_programs_arguments() {
    // A body that reads `args` is a program that takes arguments, and the
    // guard hands them over.
    let source = "public class App {\n    public static void main(String[] args) {\n        \
                  System.out.println(args.length);\n    }\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("App.java");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("app_out.txt");
    let plan = transpile::plan_to(&path, Language::Python, Some(&out), false).unwrap();
    for expected in [
        "import sys",
        "if __name__ == \"__main__\":",
        "main(sys.argv[1:])",
    ] {
        assert!(
            plan.output.contains(expected),
            "missing `{expected}`:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_java_main_that_ignores_its_arguments_takes_none() {
    // `main(String[] args)` is the one signature the runtime looks for. A body
    // that never reads `args` says nothing about what the program takes.
    // Carried as data, it came back out as a parameter the source never wrote.
    let source = "public class App {\n    public static void main(String[] args) {\n        \
                  System.out.println(\"run\");\n    }\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("App.java");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("app_out.txt");
    let plan = transpile::plan_to(&path, Language::Python, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("def main() -> None:") && plan.output.contains("    main()"),
        "the guard calls what the signature declares:\n{}",
        plan.output
    );
}

#[test]
fn a_rust_main_gains_pythons_own_guard() {
    let source = "fn main() {\n    println!(\"run\");\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("app.rs");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("app_out.txt");
    let plan = transpile::plan_to(&path, Language::Python, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("if __name__ == \"__main__\":") && plan.output.contains("main()"),
        "the implicit entry becomes the guarded call:\n{}",
        plan.output
    );
}

#[test]
fn a_python_main_stays_gos_own_lowercase_entry() {
    let source =
        "def main() -> None:\n    print(\"run\")\n\n\nif __name__ == \"__main__\":\n    main()\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("app.py");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("app_out.txt");
    let plan = transpile::plan_to(&path, Language::Go, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("func main() {"),
        "the runtime calls `main`, lowercase and niladic; an exported Main never starts.\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains("func Main"),
        "the exported spelling would leave `package main` with no entry point.\n{}",
        plan.output
    );
}

#[test]
fn the_self_running_targets_drop_the_entry_call_and_say_so() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("tasks.ts");
    std::fs::write(&path, TASKS_TS).unwrap();
    for to in [Language::Rust, Language::Go, Language::Java, Language::Zig] {
        let out = tmp.path().join(format!("tasks_{to}.txt"));
        let plan = transpile::plan_to(&path, to, Some(&out), false).unwrap();
        assert!(
            plan.fidelity
                .notes
                .iter()
                .any(|n| n.contains("runs main itself")),
            "{to} says why the call is not there.\n{:?}",
            plan.fidelity.notes
        );
        assert!(
            !plan
                .output
                .contains("top-level statement runs here in the source"),
            "{to} does not carry what it dropped on purpose.\n{}",
            plan.output
        );
    }
}

#[test]
fn an_async_entry_runs_under_asyncio() {
    let source =
        "async function main(): Promise<void> {\n    console.log(\"run\");\n}\n\nmain();\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("app.ts");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("app_out.txt");
    let plan = transpile::plan_to(&path, Language::Python, Some(&out), false).unwrap();
    assert!(
        plan.output.contains("import asyncio") && plan.output.contains("asyncio.run(main())"),
        "an async main called bare would build a coroutine and drop it.\n{}",
        plan.output
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
