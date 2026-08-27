//! A translated Go file has to be one `go build` accepts.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

/// The package clause is taken from the file being written, so the destination
/// is the one a `--write` would choose.
fn translated(source: &str, name: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = path.with_extension("go");
    transpile::plan_to(&path, Language::Go, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn a_library_is_named_after_its_file() {
    let go = translated("def helper() -> int:\n    return 1\n", "shapes.py");
    assert!(
        go.contains("package shapes"),
        "`package main` without an entry point does not build.\n{go}"
    );
}

#[test]
fn an_entry_point_still_lands_in_package_main() {
    let go = translated("def main():\n    print(\"hi\")\n", "app.py");
    assert!(
        go.contains("package main"),
        "Go's runtime calls `main` in `main`.\n{go}"
    );
}

#[test]
fn an_empty_list_takes_the_type_it_goes_on_to_hold() {
    let source = "class Point:\n    def __init__(self, x: int):\n        self.x = x\n\n\n\
        def collect(n: int) -> list[Point]:\n    out = []\n    for i in range(n):\n        \
        out.append(Point(i))\n    return out\n";
    let go = translated(source, "pts.py");
    assert!(
        go.contains("out := []Point{}"),
        "`[]any` under a signature promising `[]Point` does not compile.\n{go}"
    );
}

#[test]
fn a_body_that_did_not_translate_panics_instead_of_falling_off_the_end() {
    let source = "def total(xs: list[int]) -> int:\n    return sum(x for x in xs)\n";
    let go = translated(source, "sum.py");
    assert!(
        go.contains("panic("),
        "Go refuses a function that promises a value and returns none.\n{go}"
    );
}

#[test]
fn a_function_that_promises_nothing_is_left_to_end_where_it_ends() {
    let source = "def shout(name: str) -> None:\n    print(name)\n";
    let go = translated(source, "shout.py");
    assert!(
        !go.contains("panic("),
        "nothing was promised, so nothing is missing.\n{go}"
    );
}

#[test]
fn a_switch_whose_every_arm_returns_needs_nothing_after_it() {
    // From TypeScript, whose `switch` the reader models.
    let source = "export function word(n: number): string {\n    switch (n) {\n        \
        case 0:\n            return \"none\";\n        default:\n            \
        return \"some\";\n    }\n}\n";
    let go = translated(source, "word.ts");
    assert!(
        !go.contains("panic("),
        "an exhaustive switch terminates, and code after it is unreachable.\n{go}"
    );
}

#[test]
fn an_if_let_whose_branches_both_return_needs_nothing_after_it() {
    let source = "pub fn label(o: Option<i64>) -> i64 {\n    if let Some(v) = o {\n        \
        return v * 2;\n    } else {\n        return 0;\n    }\n}\n";
    let go = translated(source, "opt.rs");
    assert!(
        !go.contains("panic("),
        "both branches leave, so nothing follows them.\n{go}"
    );
}
