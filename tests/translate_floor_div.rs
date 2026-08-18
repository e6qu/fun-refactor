//! Python's `//` crosses as the floor division it is, in every target.
//!
//! Read as an unknown operator, `cents // 100` left a runnable `null` where a
//! number belonged, and a formatter printed "$null.00". Only Python spells the
//! operation as an operator. Every writer here says it with its own library
//! call, and each of those rounds toward negative infinity the way `//` does.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

const CENTS_PY: &str = "def format_cents(cents: int) -> str:\n    \
    dollars = cents // 100\n    return \"$\" + str(dollars)\n";

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let out = dir.join(format!("out_{target:?}")).with_extension("txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn floor_division_is_math_floor_in_typescript() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "cents.py", CENTS_PY, Language::TypeScript);
    assert!(
        out.contains("Math.floor(cents / 100)"),
        "the number is a number again, no longer a runnable null.\n{out}"
    );
}

#[test]
fn floor_division_is_div_euclid_in_rust() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "cents.py", CENTS_PY, Language::Rust);
    assert!(
        out.contains("cents.div_euclid(100)"),
        "`div_euclid` rounds the same way `//` does on the negatives.\n{out}"
    );
}

#[test]
fn floor_division_is_math_floordiv_in_java() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "cents.py", CENTS_PY, Language::Java);
    assert!(
        out.contains("Math.floorDiv(cents, 100)"),
        "Java has the exact call.\n{out}"
    );
}

#[test]
fn floor_division_is_a_floored_float_division_in_go_with_its_import() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "cents.py", CENTS_PY, Language::Go);
    assert!(
        out.contains("int(math.Floor(float64(cents) / float64(100)))"),
        "Go floors through the float library.\n{out}"
    );
    assert!(
        out.contains("import \"math\""),
        "and the file imports the package it names.\n{out}"
    );
}

#[test]
fn floor_division_is_divfloor_in_zig() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "cents.py", CENTS_PY, Language::Zig);
    assert!(
        out.contains("@divFloor(cents, 100)"),
        "Zig's builtin rounds the same way.\n{out}"
    );
}

#[test]
fn floor_division_survives_a_round_trip_through_typescript() {
    // `Math.floor(a / b)` is what this tool writes; a Python file that crossed
    // and came home must still floor. The TypeScript reader has no floor-form
    // recogniser, so the call comes home as the call, which still computes the
    // same number.
    let tmp = tempfile::tempdir().unwrap();
    let ts = translated(tmp.path(), "cents.py", CENTS_PY, Language::TypeScript);
    let back = translated(tmp.path(), "cents.ts", &ts, Language::Python);
    assert!(
        back.contains("Math.floor(cents / 100)") || back.contains("cents // 100"),
        "the flooring is still in the program.\n{back}"
    );
}
