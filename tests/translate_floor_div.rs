//! Python's `//` crosses as the floor division it is, in every target.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;
use std::process::Command;

const CENTS_PY: &str = "def format_cents(cents: int) -> str:\n    \
    dollars = cents // 100\n    return \"$\" + str(dollars)\n";

/// Four sign pairings, printed one per line, so a wrong rounding shows as a diff.
const SIGNS_PY: &str = r#"def floored(a: int, b: int) -> int:
    return a // b


def main() -> None:
    print(floored(7, -2))
    print(floored(-7, 2))
    print(floored(7, 2))
    print(floored(-7, -2))
    print(floored(-8, -2))
    print(floored(8, -2))


main()
"#;

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
fn floor_division_is_a_declared_helper_in_rust() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "cents.py", CENTS_PY, Language::Rust);
    assert!(
        out.contains("floor_div(cents, 100)"),
        "the call site names the helper.\n{out}"
    );
    assert!(
        out.contains("fn floor_div(dividend: i64, divisor: i64) -> i64 {"),
        "and the file declares the helper it names.\n{out}"
    );
    assert!(
        !out.contains("div_euclid"),
        "`div_euclid` rounds the other way when the divisor is negative.\n{out}"
    );
}

#[test]
fn a_declared_floor_div_keeps_its_name_and_the_helper_takes_another() {
    // A module of its own may already have the name.
    let tmp = tempfile::tempdir().unwrap();
    let source = "def floor_div(a: int, b: int) -> int:\n    return a - b\n\n\n\
                  def halves(n: int) -> int:\n    return n // 2\n";
    let out = translated(tmp.path(), "own.py", source, Language::Rust);
    assert!(
        out.contains("pub fn floor_div(a: i64, b: i64) -> i64 {"),
        "the module's own function keeps its name.\n{out}"
    );
    assert!(
        out.contains("floor_div_helper(n, 2)"),
        "and the flooring reaches a helper under another name.\n{out}"
    );
}

#[test]
fn a_float_floor_division_floors_the_quotient_in_rust() {
    // Floats divide without truncating, so the integer helper does not apply
    // and would not compile against them either.
    let tmp = tempfile::tempdir().unwrap();
    let source = "def half(x: float, y: float) -> float:\n    return x // y\n";
    let out = translated(tmp.path(), "half.py", source, Language::Rust);
    assert!(
        out.contains("(x / y).floor()"),
        "a float floors through its own method.\n{out}"
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
    // `Math.floor(a / b)` is what this tool writes; a Python file that crossed and came home
    // must still floor.
    let tmp = tempfile::tempdir().unwrap();
    let ts = translated(tmp.path(), "cents.py", CENTS_PY, Language::TypeScript);
    let back = translated(tmp.path(), "cents.ts", &ts, Language::Python);
    assert!(
        back.contains("Math.floor(cents / 100)") || back.contains("cents // 100"),
        "the flooring is still in the program.\n{back}"
    );
}

fn said(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn ran(command: &mut Command) -> std::process::Output {
    let output = command.output().expect("a toolchain that runs");
    assert!(
        output.status.success(),
        "the translation did not run: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn every_target_prints_the_same_floors_as_python_does() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(dir.join("signs.py"), SIGNS_PY).unwrap();
    let expected = said(&ran(Command::new("python3").arg(dir.join("signs.py"))));
    assert_eq!(expected, "-4\n-4\n3\n3\n4\n-4", "Python's own answers");

    let mut missing = Vec::new();

    if Toolchain::Cargo.is_available() {
        let rust = translated(dir, "signs.py", SIGNS_PY, Language::Rust);
        std::fs::write(dir.join("signs.rs"), &rust).unwrap();
        ran(Command::new("rustc")
            .arg("-o")
            .arg(dir.join("signs"))
            .arg(dir.join("signs.rs")));
        assert_eq!(
            said(&ran(&mut Command::new(dir.join("signs")))),
            expected,
            "Rust floors the way the source did.\n{rust}"
        );
    } else {
        missing.push("rustc".to_string());
    }

    if Toolchain::Go.is_available() {
        let go = translated(dir, "signs.py", SIGNS_PY, Language::Go);
        std::fs::write(dir.join("signs.go"), &go).unwrap();
        std::fs::write(dir.join("go.mod"), "module signs\n\ngo 1.21\n").unwrap();
        assert_eq!(
            said(&ran(Command::new("go")
                .arg("run")
                .arg("signs.go")
                .current_dir(dir))),
            expected,
            "Go floors the way the source did.\n{go}"
        );
    } else {
        missing.push("go".to_string());
    }

    if Toolchain::Javac.is_available() {
        // Java names the file after the public class, so the destination is
        // part of what crosses and no place to put the result.
        let java = transpile::plan_to(
            &dir.join("signs.py"),
            Language::Java,
            Some(&dir.join("Signs.java")),
            false,
        )
        .expect("a plan")
        .output;
        std::fs::write(dir.join("Signs.java"), &java).unwrap();
        ran(Command::new("javac").arg("Signs.java").current_dir(dir));
        assert_eq!(
            said(&ran(Command::new("java").arg("Signs").current_dir(dir))),
            expected,
            "Java floors the way the source did.\n{java}"
        );
    } else {
        missing.push("javac".to_string());
    }

    require_on_ci("floor division, run", &missing);
}
