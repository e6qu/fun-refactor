//! An assert crosses as a check that stops the program, in every target.
//!
//! Python's `assert c, "m"` used to carry as a comment, so a translated test
//! file checked nothing and still printed "all tests passed". The targets with
//! an assert keep it: Rust's `assert!`, Zig's `std.debug.assert`, Python's own
//! statement. The targets without one say it longhand, test the condition and
//! throw or panic, which is the same program stopping for the same reason.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

const CHECKS_PY: &str = "def check(total: int) -> None:\n    \
    assert total >= 0, \"total went negative\"\n    assert total < 100\n";

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let out = dir.join(format!("out_{target:?}")).with_extension("txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn a_python_assert_is_a_typescript_check_that_throws() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "checks.py", CHECKS_PY, Language::TypeScript);
    assert!(
        out.contains("if (!(total >= 0)) {")
            && out.contains("throw new Error(\"total went negative\");"),
        "the message travels with the check.\n{out}"
    );
    assert!(
        out.contains("if (!(total < 100)) {")
            && out.contains("throw new Error(\"assertion failed\");"),
        "a messageless assert still says why it threw.\n{out}"
    );
    assert!(
        !out.contains("not translated: assert_statement"),
        "nothing about an assert is a gap any more.\n{out}"
    );
}

#[test]
fn a_python_assert_is_a_go_check_that_panics() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "checks.py", CHECKS_PY, Language::Go);
    assert!(
        out.contains("if !(total >= 0) {") && out.contains("panic(\"total went negative\")"),
        "the check panics with the source's words.\n{out}"
    );
}

#[test]
fn a_python_assert_is_a_rust_assert_macro() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "checks.py", CHECKS_PY, Language::Rust);
    assert!(
        out.contains("assert!(total >= 0, \"total went negative\");")
            && out.contains("assert!(total < 100);"),
        "a literal message rides in the macro and a missing one is left out.\n{out}"
    );
}

#[test]
fn a_python_assert_is_a_java_check_that_throws() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "checks.py", CHECKS_PY, Language::Java);
    assert!(
        out.contains("if (!(total >= 0)) {")
            && out.contains("throw new Error(\"total went negative\");"),
        "Java's own `assert` is off by default, so the longhand check runs everywhere.\n{out}"
    );
}

#[test]
fn a_python_assert_is_zigs_own_library_check() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "checks.py", CHECKS_PY, Language::Zig);
    assert!(
        out.contains("std.debug.assert(total >= 0);"),
        "Zig has the check in its library.\n{out}"
    );
    assert!(
        out.contains("const std = @import(\"std\");"),
        "and the file binds `std` to reach it.\n{out}"
    );
    assert!(
        out.contains("// the assert's message: \"total went negative\""),
        "the message has no slot in the call, so it rides above the statement.\n{out}"
    );
}

#[test]
fn a_python_assert_with_a_computed_message_passes_it_to_the_rust_macro() {
    // The macro takes a format string and arguments, and evaluates them only
    // when the check fails. Rendering the message into a comment above the
    // check dropped it, along with any effect computing it had.
    let source = "def check(n: int) -> None:\n    assert n > 0, \"got \" + str(n)\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "computed.py", source, Language::Rust);
    assert!(
        out.contains("assert!(n > 0, \"{}\", "),
        "the message rides as an argument.\n{out}"
    );
    assert!(
        !out.contains("// the assert's message:"),
        "nothing is left in a comment.\n{out}"
    );
}

#[test]
fn rusts_assert_family_reads_back_as_the_checks_they_are() {
    let source =
        "pub fn check(total: i64) {\n    assert!(total >= 0, \"total went negative\");\n    \
        assert_eq!(total % 2, 0);\n    assert_ne!(total, 13);\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "checks.rs", source, Language::Python);
    assert!(
        out.contains("assert total >= 0, \"total went negative\""),
        "`assert!` crosses with its message.\n{out}"
    );
    assert!(
        out.contains("assert total % 2 == 0"),
        "`assert_eq!` is the comparison it abbreviates.\n{out}"
    );
    assert!(
        out.contains("assert total != 13"),
        "and `assert_ne!` the other one.\n{out}"
    );
}

#[test]
fn zigs_library_assert_reads_back_as_pythons_statement() {
    let source = "const std = @import(\"std\");\n\npub fn check(total: i64) void {\n    \
        std.debug.assert(total >= 0);\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "checks.zig", source, Language::Python);
    assert!(
        out.contains("assert total >= 0"),
        "the library call is the statement.\n{out}"
    );
    assert!(
        !out.contains("std.debug.assert"),
        "no path from another language's library survives.\n{out}"
    );
}

#[test]
fn a_translated_python_test_still_checks_something() {
    // The defect this exists for: a test file whose asserts carried as comments
    // ran, checked nothing, and printed its success line anyway.
    let source = "def test_math() -> None:\n    assert 1 + 1 == 2\n\n\n\
        def run_all() -> None:\n    test_math()\n    print(\"all tests passed\")\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "test_math.py", source, Language::TypeScript);
    assert!(
        out.contains("if (!(1 + 1 === 2)) {"),
        "the check is live code in the target.\n{out}"
    );
}
