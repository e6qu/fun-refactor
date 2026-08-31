//! One value branched against literal alternatives crosses every boundary here.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;

const STATUS_RS: &str = "pub fn label(code: i64) -> &'static str {\n    match code {\n        \
                         200 => \"ok\",\n        404 => \"missing\",\n        \
                         500 | 503 => \"broken\",\n        _ => \"unknown\",\n    }\n}\n";

#[test]
fn a_rust_match_crosses_into_every_target() {
    let (_tmp, root) = common::tree(&[("status.rs", STATUS_RS)]);
    let cases = [
        (Language::Python, "match code:"),
        (Language::Python, "case 500 | 503:"),
        (Language::Python, "case _:"),
        (Language::TypeScript, "switch (code) {"),
        (Language::TypeScript, "case 500:"),
        (Language::TypeScript, "default:"),
        (Language::Go, "case 500, 503:"),
        (Language::Java, "switch (code) {"),
        (Language::Zig, "500, 503 => {"),
        (Language::Zig, "else => {"),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("status.rs"), to).expect("a draft");
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
fn arm_expressions_return_in_tail_position() {
    let (_tmp, root) = common::tree(&[("status.rs", STATUS_RS)]);
    let plan = transpile::plan(&root.join("status.rs"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains("return \"broken\""),
        "a tail match's arm value is the function's answer.\n{}",
        plan.output
    );
}

#[test]
fn a_zig_switch_becomes_a_match() {
    let source =
        "// The demo switch.\npub fn label(code: i64) []const u8 {\n    switch (code) {\n        \
                  200 => return \"ok\",\n        500, 503 => return \"broken\",\n        \
                  else => return \"unknown\",\n    }\n}\n";
    let (_tmp, root) = common::tree(&[("s.zig", source)]);
    let plan = transpile::plan(&root.join("s.zig"), Language::Rust).expect("a draft");
    assert!(plan.output.contains("match code {"), "{}", plan.output);
    assert!(plan.output.contains("500 | 503 => {"), "{}", plan.output);
}

#[test]
fn a_typescript_switch_becomes_a_match_and_stacked_cases_merge() {
    let source = "export function label(code: number): string {\n    switch (code) {\n        \
                  case 200:\n            return \"ok\";\n        case 500:\n        case 503:\n            \
                  return \"broken\";\n        default:\n            return \"unknown\";\n    }\n}\n";
    let (_tmp, root) = common::tree(&[("s.ts", source)]);
    let plan = transpile::plan(&root.join("s.ts"), Language::Rust).expect("a draft");
    assert!(plan.output.contains("500 | 503 => {"), "{}", plan.output);
}

#[test]
fn a_fall_through_with_statements_carries() {
    // Case 200 runs its statement and falls into 500's body; the IR's arms are
    // disjoint, so translating it would change what runs.
    let source = "export function label(code: number): string {\n    switch (code) {\n        \
                  case 200:\n            log(code);\n        case 500:\n            \
                  return \"seen\";\n        default:\n            return \"other\";\n    }\n}\n";
    let (_tmp, root) = common::tree(&[("f.ts", source)]);
    let plan = transpile::plan(&root.join("f.ts"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "fall-through must carry, never silently becoming disjoint arms.\n{}",
        plan.output
    );
}

#[test]
fn a_guarded_or_structured_pattern_carries() {
    let source = "fn pick(p: (i64, i64)) -> i64 {\n    match p {\n        (0, y) => y,\n        \
                  _ => 0,\n    }\n}\n";
    let (_tmp, root) = common::tree(&[("g.rs", source)]);
    let plan = transpile::plan(&root.join("g.rs"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "a destructuring arm is a match in the full sense and must carry.\n{}",
        plan.output
    );
}

#[test]
fn a_zig_range_case_carries() {
    let source = "pub fn class(code: i64) i64 {\n    switch (code) {\n        \
                  200...299 => return 2,\n        else => return 0,\n    }\n}\n";
    let (_tmp, root) = common::tree(&[("r.zig", source)]);
    let plan = transpile::plan(&root.join("r.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "a range case has no literal crossing and must carry.\n{}",
        plan.output
    );
}
