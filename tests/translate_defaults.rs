//! Python spells no default that reads another parameter.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(source: &str, name: &str, target: Language) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("out.txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn a_default_reading_a_parameter_becomes_the_sentinel_idiom() {
    let source = "export function pad(text: string, width: number = text.length + 2): string {\n  \
        return text;\n}\n";
    let py = translated(source, "pad.ts", Language::Python);
    assert!(
        py.contains("width: float | None = None"),
        "the sentinel widens the type it stands in for.\n{py}"
    );
    assert!(
        py.contains("if width is None:"),
        "the default is computed where the parameters exist.\n{py}"
    );
    assert!(
        !py.contains("= text.length + 2)"),
        "nothing reads a parameter from the signature.\n{py}"
    );
}

#[test]
fn a_default_that_reads_nothing_stays_in_the_signature() {
    let source = "export function pad(text: string, width: number = 8): string {\n  \
        return text;\n}\n";
    let py = translated(source, "pad.ts", Language::Python);
    assert!(
        py.contains("width: float = 8"),
        "a constant default needs no sentinel.\n{py}"
    );
    assert!(!py.contains("is None"), "and no guard in the body.\n{py}");
}
