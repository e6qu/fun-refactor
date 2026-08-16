//! A distinct type crosses as a distinct type.
//!
//! `Pence = NewType("Pence", int)` read as a constant crossed into every target as a
//! value, and `NewType`, `int` and the quotes crossed with it: output that parses and
//! refers to nothing. Each language here has a real spelling for the idea, and the
//! translation now uses it.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(file: &str, source: &str, to: Language) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, source).expect("the file");
    transpile::plan(&path, to).expect("a translation").output
}

const MONEY: &str = "\
from typing import NewType

Pence = NewType(\"Pence\", int)


def double(total: Pence) -> Pence:
    return Pence(total * 2)
";

#[test]
fn each_language_spells_the_newtype_its_own_way() {
    for (target, expected) in [
        (Language::Rust, "pub struct Pence(pub i64);"),
        (Language::Go, "type Pence int"),
        (Language::Java, "public record Pence(int value) {}"),
        (Language::Zig, "pub const Pence = enum(i64) { _ };"),
        (
            Language::TypeScript,
            "export type Pence = number & { readonly [penceBrand]: true };",
        ),
    ] {
        let out = translated("money.py", MONEY, target);
        assert!(out.contains(expected), "{target}:\n{out}");
        assert!(
            !out.contains("NewType(\"Pence\""),
            "{target} carried the Python incantation through:\n{out}"
        );
    }
}

#[test]
fn the_construction_follows_the_language() {
    for (target, expected) in [
        (Language::Rust, "Pence(total * 2)"),
        (Language::Java, "new Pence(total * 2)"),
        (Language::Zig, "@as(Pence, @enumFromInt(total * 2))"),
        (Language::TypeScript, "return value as Pence;"),
    ] {
        let out = translated("money.py", MONEY, target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

#[test]
fn a_signature_using_the_newtype_is_complete() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("money.py");
    std::fs::write(&path, MONEY).expect("the file");
    let plan = transpile::plan(&path, Language::Rust).expect("a translation");
    assert_eq!(plan.fidelity.newtypes, 1, "{:?}", plan.fidelity);
    assert_eq!(
        plan.fidelity.signatures_with_foreign_types, 0,
        "its own newtype counted as foreign:\n{}",
        plan.output
    );
}

#[test]
fn the_typescript_brand_reads_back_as_a_newtype() {
    let source = "\
declare const penceBrand: unique symbol;
export type Pence = number & { readonly [penceBrand]: true };

export function double(total: Pence): Pence {
    return (total * 2) as Pence;
}
";
    let out = translated("money.ts", source, Language::Python);
    assert!(out.contains("Pence = NewType(\"Pence\", float)"), "{out}");
    assert!(out.contains("from typing import NewType"), "{out}");
    assert!(
        !out.contains("unique symbol"),
        "the brand's plumbing crossed as text:\n{out}"
    );
}

#[test]
fn the_python_spelling_survives_a_round_trip() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("money.py");
    std::fs::write(&path, MONEY).expect("the file");
    let there = transpile::plan(&path, Language::TypeScript).expect("out");
    let ts = tmp.path().join("money_back.ts");
    std::fs::write(&ts, &there.output).expect("the file");
    let back = transpile::plan(&ts, Language::Python).expect("back");
    assert!(
        back.output.contains("Pence = NewType(\"Pence\", int)")
            || back.output.contains("Pence = NewType(\"Pence\", float)"),
        "{}",
        back.output
    );
}
