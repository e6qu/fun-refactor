//! A module constant crosses as something the target can build.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(source: &str, name: &str, target: Language) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = path.with_extension(match target {
        Language::Go => "go",
        Language::Rust => "rs",
        _ => "txt",
    });
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

const CONSTANTS: &str = "RETRY_LIMIT = 3\nNAMES = [\"a\", \"b\"]\n";

#[test]
fn a_literal_says_its_own_rust_type() {
    let rust = translated(CONSTANTS, "m.py", Language::Rust);
    assert!(
        rust.contains("pub const RETRY_LIMIT: i64 = 3;"),
        "an int constant is not a &str.\n{rust}"
    );
    assert!(
        rust.contains("pub const NAMES: &[&str] = &[\"a\", \"b\"];"),
        "a list of literals keeps its name's case and becomes a slice.\n{rust}"
    );
}

#[test]
fn go_reserves_const_for_what_go_can_const() {
    let go = translated(CONSTANTS, "m.py", Language::Go);
    assert!(
        go.contains("const RetryLimit = 3"),
        "a scalar stays const.\n{go}"
    );
    assert!(
        go.contains("var NAMES = []string{\"a\", \"b\"}"),
        "a slice is a var, in the author's own case.\n{go}"
    );
}

#[test]
fn a_module_docstring_is_commented_in_full() {
    let source = "\"\"\"One line.\n\nAnother paragraph, after a blank.\n\"\"\"\n\nX = 1\n";
    let go = translated(source, "m.py", Language::Go);
    for line in ["One line.", "Another paragraph, after a blank."] {
        assert!(
            go.contains(&format!("// {line}")),
            "every docstring line carries the marker.\n{go}"
        );
    }
    assert!(
        !go.lines().any(|l| l.trim() == "One line."),
        "no docstring line lands as raw prose.\n{go}"
    );
}

#[test]
fn a_slash_with_a_string_operand_is_no_division() {
    let source = "BASE = \"tools\"\n\n\ndef join(root: str) -> str:\n    return root / BASE\n";
    let go = translated(source, "m.py", Language::Go);
    assert!(
        !go.contains("float64(\"") && !go.contains("float64(BASE)"),
        "a string never coerces to a float.\n{go}"
    );
    assert!(
        go.contains("not translated"),
        "what it is cannot be said in the target, and the draft says so.\n{go}"
    );
}
