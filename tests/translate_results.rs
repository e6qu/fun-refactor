//! A `Result` in the signature crosses as each target's own way of failing.
//!
//! Rust spells it `Result<T, E>` and Zig `E!T`, and the readers agree on one name
//! for both. Go writes the `(T, error)` pair it always writes, with `Ok`, `Err`
//! and every propagation becoming the returns and checks that pair means. The
//! exception languages return the ok value bare and raise the Err, which is
//! where their propagated calls already fail to. Zig takes the union back.

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

const CONFIG_RS: &str = "struct Config {\n    name: String,\n    port: i64,\n}\n\n\
    fn parse(s: &str) -> Result<Config, String> {\n    if s.is_empty() {\n        \
    return Err(format!(\"empty input: {}\", s));\n    }\n    \
    Ok(Config { name: s.to_string(), port: 8080 })\n}\n\n\
    fn load(x: &str) -> Result<i64, String> {\n    let c = parse(x)?;\n    \
    Ok(c.port + 1)\n}\n";

const LEDGER_ZIG: &str = "const ParseError = error{Empty};\n\n\
    fn parseLen(s: []const u8) ParseError!usize {\n    if (s.len == 0) return ParseError.Empty;\n    \
    return s.len;\n}\n\n\
    fn touch(s: []const u8) !void {\n    _ = try parseLen(s);\n}\n";

#[test]
fn a_rust_result_becomes_gos_value_and_error_pair() {
    let (_tmp, root) = workspace(&[("config.rs", CONFIG_RS)]);
    let plan = transpile::plan(&root.join("config.rs"), Language::Go).expect("a draft");
    for expected in [
        "func parse(s string) (Config, error) {",
        "return Config{}, fmt.Errorf(\"empty input: %v\", s)",
        "return Config{name: fmt.Sprint(s), port: 8080}, nil",
        "func load(x string) (int, error) {",
        "c, err := parse(x)",
        "if err != nil {",
        "return 0, err",
        "return c.port + 1, nil",
        "import \"fmt\"",
    ] {
        assert!(
            plan.output.contains(expected),
            "missing `{expected}`:\n{}",
            plan.output
        );
    }
    assert!(
        !plan.output.contains(transpile::MARKER),
        "the whole Result mechanism is Go's own idiom, not a loss.\n{}",
        plan.output
    );
}

#[test]
fn a_zig_error_union_becomes_a_rust_result() {
    let (_tmp, root) = workspace(&[("ledger.zig", LEDGER_ZIG)]);
    let plan = transpile::plan(&root.join("ledger.zig"), Language::Rust).expect("a draft");
    // The canonical failure is its message: the declared set stays as the
    // enum it names, and the failure that crosses is the variant's name.
    for expected in [
        "enum ParseError {",
        "Empty,",
        "fn parse_len(s: String) -> Result<i64, String> {",
        "return Err(\"Empty\".to_string());",
        "return Ok((s.len() as i64));",
        "fn touch(s: String) -> Result<(), String> {",
        "_ = parse_len(s)?;",
        "return Ok(());",
    ] {
        assert!(
            plan.output.contains(expected),
            "missing `{expected}`:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_zig_error_union_inherits_the_go_mapping() {
    let (_tmp, root) = workspace(&[("ledger.zig", LEDGER_ZIG)]);
    let plan = transpile::plan(&root.join("ledger.zig"), Language::Go).expect("a draft");
    for expected in [
        "func parseLen(s string) (int, error) {",
        "return 0, errors.New(\"Empty\")",
        "return len(s), nil",
        "func touch(s string) error {",
        "__fr_value1, err := parseLen(s)",
        "return err",
        "return nil",
        "import \"errors\"",
    ] {
        assert!(
            plan.output.contains(expected),
            "missing `{expected}`:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_returned_err_is_raised_where_failure_is_an_exception() {
    let (_tmp, root) = workspace(&[("config.rs", CONFIG_RS)]);
    let python = transpile::plan(&root.join("config.rs"), Language::Python).expect("a draft");
    assert!(
        python
            .output
            .contains("raise Exception(f\"empty input: {s}\")"),
        "the Err is raised, message and all.\n{}",
        python.output
    );
    assert!(
        python.output.contains("return c.port + 1"),
        "the ok value returns bare:\n{}",
        python.output
    );
    let ts = transpile::plan(&root.join("config.rs"), Language::TypeScript).expect("a draft");
    assert!(
        ts.output.contains("throw new Error(`empty input: ${s}`);"),
        "the Err is thrown, message and all.\n{}",
        ts.output
    );
}

#[test]
fn a_rust_result_comes_back_as_zigs_own_error_union() {
    let (_tmp, root) = workspace(&[("config.rs", CONFIG_RS)]);
    let plan = transpile::plan(&root.join("config.rs"), Language::Zig).expect("a draft");
    assert!(
        plan.output
            .contains("fn load(x: []const u8) anyerror!i64 {"),
        "an anyerror union; Zig cannot hold the message.\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("const c = try parse(x);"),
        "the propagation is `try` again.\n{}",
        plan.output
    );
}

const CLASSIFY_ZIG: &str = "// The demo switch.\nfn classify(n: i64) i64 {\n    \
    // Pick the label.\n    const label = switch (n) {\n        0 => @as(i64, 100),\n        \
    1, 2 => 200,\n        else => 300,\n    };\n    return label;\n}\n";

#[test]
fn a_zig_switch_in_value_position_becomes_a_match_expression() {
    let (_tmp, root) = workspace(&[("c.zig", CLASSIFY_ZIG)]);
    let plan = transpile::plan(&root.join("c.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output
            .contains("let label = match n { 0 => (100 as i64), 1 | 2 => 200, _ => 300 };"),
        "the declare-then-assign pair folds back into one match.\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains(transpile::MARKER),
        "a value-position switch with literal arms is not a loss.\n{}",
        plan.output
    );
}

#[test]
fn the_lowered_value_switch_declares_then_assigns_everywhere_else() {
    let (_tmp, root) = workspace(&[("c.zig", CLASSIFY_ZIG)]);
    let plan = transpile::plan(&root.join("c.zig"), Language::Python).expect("a draft");
    for expected in ["label = None", "match n:", "case 1 | 2:", "label = 200"] {
        assert!(
            plan.output.contains(expected),
            "missing `{expected}`:\n{}",
            plan.output
        );
    }
}
