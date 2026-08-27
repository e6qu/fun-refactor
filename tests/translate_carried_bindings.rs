//! A declaration whose initializer or type cannot cross still declares its name.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

const CONC_GO: &str = "package main\n\nimport (\n\t\"fmt\"\n\t\"sync\"\n)\n\n\
    func main() {\n\tch := make(chan int, 4)\n\tvar wg sync.WaitGroup\n\t\
    wg.Add(1)\n\tfor v := range ch {\n\t\tfmt.Println(v)\n\t}\n\twg.Wait()\n}\n";

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let out = dir.join(format!("out_{target:?}")).with_extension("txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn a_value_less_go_var_keeps_its_name_in_python() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "conc.go", CONC_GO, Language::Python);
    assert!(
        out.contains("wg = None"),
        "the binding exists, so `wg.Add(1)` reads a declared name.\n{out}"
    );
    assert!(
        out.contains("# fun-refactor: not translated: var wg sync.WaitGroup"),
        "and the original sits right above it.\n{out}"
    );
}

#[test]
fn a_channel_making_short_declaration_keeps_its_name_in_python() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "conc.go", CONC_GO, Language::Python);
    assert!(
        out.contains("ch = None"),
        "the failed initializer collapses to one carried value and the name stays.\n{out}"
    );
    assert!(
        out.contains("for v in ch:"),
        "so the loop after it still reads a declared name.\n{out}"
    );
}

#[test]
fn typescript_types_a_carried_binding_any_so_strict_accepts_it() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "conc.go", CONC_GO, Language::TypeScript);
    assert!(
        out.contains(
            "let wg: any = null /* fun-refactor: not translated: var wg sync.WaitGroup */;"
        ),
        "no type survived, and `any` keeps the declaration compiling.\n{out}"
    );
    assert!(
        out.contains("let ch: any = null"),
        "the `:=` form declares the same way.\n{out}"
    );
}

#[test]
fn a_go_var_with_a_value_translates_as_the_binding_it_is() {
    let source = "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tvar total = 3\n\t\
        fmt.Println(total)\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "sum.go", source, Language::Python);
    assert!(
        out.contains("total = 3"),
        "a var whose initializer crosses is an ordinary binding.\n{out}"
    );
    assert!(
        !out.contains("not translated: var_declaration"),
        "and no marker stands over it.\n{out}"
    );
}
