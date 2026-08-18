//! An optional parameter carries its absence into the declaration.
//!
//! TypeScript's `punct?: string` lets every caller leave the argument out.
//! Crossing to Python as `punct: str | None` with no default declared the
//! optionality and still required the argument, so every valid call site
//! became a TypeError. The absence is part of the signature: Python writes
//! `= None`, and Rust writes the `Option` the type already said.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

const GREET_TS: &str = "export function greet(name: string, punct?: string): string {\n    \
    return \"hi \" + name + (punct ?? \"!\");\n}\n";

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let out = dir.join(format!("out_{target:?}")).with_extension("txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn an_optional_typescript_parameter_defaults_to_none_in_python() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "greet.ts", GREET_TS, Language::Python);
    assert!(
        out.contains("punct: str | None = None"),
        "the caller may still leave the argument out.\n{out}"
    );
}

#[test]
fn an_optional_typescript_parameter_is_an_option_in_rust() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "greet.ts", GREET_TS, Language::Rust);
    assert!(
        out.contains("punct: Option<String>"),
        "the absence lives in the type, which is where Rust puts it.\n{out}"
    );
}

#[test]
fn an_explicit_default_still_wins_over_the_invented_none() {
    let source = "export function pad(text: string, fill: string = \" \"): string {\n    \
        return text + fill;\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "pad.ts", source, Language::Python);
    assert!(
        out.contains("fill: str = \" \""),
        "a default the source wrote is the default that carries.\n{out}"
    );
}
