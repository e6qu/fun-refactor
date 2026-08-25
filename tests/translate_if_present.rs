//! Testing an optional and binding its payload crosses every boundary here.
//!
//! Rust spells it `if let Some(v) = o` and Zig `if (o) |v|`. Python and
//! TypeScript spell an optional as a nullable value, so they name it and test
//! against null. Java's `Optional` and Go's pointer cannot unwrap in place.
//! Both hold the value in a second binding and take the payload out inside the
//! branch. The same statement, six spellings.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

const OPT_RS: &str = "fn label(o: Option<i64>) -> i64 {\n    if let Some(v) = o {\n        \
                      return double(v);\n    } else {\n        return 0;\n    }\n}\n";

#[test]
fn an_if_let_crosses_into_every_target() {
    let (_tmp, root) = workspace(&[("opt.rs", OPT_RS)]);
    let cases = [
        (Language::Zig, "if (o) |v| {"),
        (Language::Python, "v = o"),
        (Language::Python, "if v is not None:"),
        (Language::TypeScript, "const v = o;"),
        (Language::TypeScript, "if (v !== null) {"),
        (Language::Go, "if vPtr := o; vPtr != nil {"),
        (Language::Go, "v := *vPtr"),
        (Language::Java, "if (vMaybe.isPresent()) {"),
        (Language::Java, "var v = vMaybe.get();"),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("opt.rs"), to).expect("a draft");
        assert!(
            plan.output.contains(expected),
            "{to} is missing `{expected}`:\n{}",
            plan.output
        );
        assert!(
            !plan.output.contains(transpile::MARKER),
            "{to} carried what it can say:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_zig_payload_if_becomes_if_let() {
    let source =
        "fn label(o: ?i64) i64 {\n    if (o) |v| {\n        return v;\n    } else {\n        \
                  return 0;\n    }\n}\n";
    let (_tmp, root) = workspace(&[("zopt.zig", source)]);
    let plan = transpile::plan(&root.join("zopt.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("if let Some(v) = o {"),
        "{}",
        plan.output
    );
}

#[test]
fn a_pointer_capture_binds_the_payload() {
    // `|*v|` binds the payload in place. The dereference unwraps everywhere now. So the write
    // lands on the binding, which is the payload itself in this value model, where `o` is
    // already this function's own copy.
    let source = "fn bump(o: ?i64) void {\n    if (o) |*v| {\n        v.* += 1;\n    }\n}\n";
    let (_tmp, root) = workspace(&[("bump.zig", source)]);
    let plan = transpile::plan(&root.join("bump.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("if let Some(") && plan.output.contains("v = v + 1;"),
        "the capture binds and the write lands on the binding:\n{}",
        plan.output
    );
}

#[test]
fn another_pattern_than_some_carries() {
    // `if let Ok(x)` narrows a Result, which is a match in disguise.
    let source = "fn read(r: Result<i64, E>) -> i64 {\n    if let Ok(v) = r {\n        \
                  return v;\n    }\n    0\n}\n";
    let (_tmp, root) = workspace(&[("res.rs", source)]);
    let plan = transpile::plan(&root.join("res.rs"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "an Ok pattern is not an optional test and must carry:\n{}",
        plan.output
    );
}
