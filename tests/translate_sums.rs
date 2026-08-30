//! A closed choice crosses every language boundary here.

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

const SHAPE_RS: &str = "pub enum Shape {\n    Empty,\n    Circle { radius: f64 },\n    \
                        Tagged(String),\n}\n";

#[test]
fn a_rust_enum_crosses_into_every_target() {
    let (_tmp, root) = workspace(&[("shape.rs", SHAPE_RS)]);
    let cases = [
        (
            Language::TypeScript,
            "export type Shape = Empty | Circle | Tagged;",
        ),
        (Language::TypeScript, "readonly kind: \"circle\";"),
        (Language::Python, "Shape = Empty | Circle | Tagged"),
        (Language::Python, "class Circle:"),
        (Language::Go, "type Shape interface{ isShape() }"),
        (Language::Go, "func (Circle) isShape() {}"),
        (
            Language::Java,
            "public sealed interface Shape permits Empty, Circle, Tagged {}",
        ),
        (
            Language::Java,
            "record Circle(double radius) implements Shape {}",
        ),
        (Language::Zig, "pub const Shape = union(enum) {"),
        (Language::Zig, "circle: struct { radius: f64 },"),
        (Language::Zig, "tagged: []const u8,"),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("shape.rs"), to).expect("a draft");
        assert!(
            plan.output.contains(expected),
            "{to} is missing `{expected}`:\n{}",
            plan.output
        );
        assert_eq!(plan.fidelity.sums, 1, "{to} did not count the choice");
        assert!(
            !plan.output.contains(transpile::MARKER),
            "{to} carried something it should have translated:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_zig_tagged_union_crosses() {
    let (_tmp, root) = workspace(&[(
        "answer.zig",
        "pub const Answer = union(enum) {\n    none: void,\n    value: i64,\n    \
         span: struct { start: i64, end: i64 },\n};\n",
    )]);
    let plan = transpile::plan(&root.join("answer.zig"), Language::Rust).expect("a draft");
    assert!(plan.output.contains("pub enum Answer {"), "{}", plan.output);
    assert!(plan.output.contains("None,"), "{}", plan.output);
    assert!(
        plan.output.contains("Value { value: i64 },"),
        "{}",
        plan.output
    );
    assert!(
        plan.output.contains("Span { start: i64, end: i64 },"),
        "{}",
        plan.output
    );
}

#[test]
fn a_zig_plain_enum_is_a_choice_with_bare_variants() {
    let (_tmp, root) = workspace(&[(
        "mode.zig",
        "pub const Mode = enum {\n    fast,\n    careful,\n};\n",
    )]);
    let plan = transpile::plan(&root.join("mode.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("pub enum Mode {") && plan.output.contains("Fast,"),
        "{}",
        plan.output
    );
}

#[test]
fn an_untagged_zig_union_stays_carried() {
    // A bare `union` overlays its members and knows nothing about which is live.
    let (_tmp, root) = workspace(&[(
        "raw.zig",
        "pub const Raw = union {\n    as_int: i64,\n    as_float: f64,\n};\n",
    )]);
    let plan = transpile::plan(&root.join("raw.zig"), Language::Rust).expect("a draft");
    assert_eq!(plan.fidelity.sums, 0, "{}", plan.output);
    assert!(
        plan.output.contains(transpile::MARKER),
        "an untagged union has to carry across rather than turn into a guess:\n{}",
        plan.output
    );
}

#[test]
fn a_typescript_discriminated_union_becomes_an_enum() {
    let (_tmp, root) = workspace(&[(
        "payment.ts",
        "export interface Card {\n    readonly kind: \"card\";\n    number: string;\n}\n\n\
         export interface Cash {\n    readonly kind: \"cash\";\n}\n\n\
         export type Payment = Card | Cash;\n",
    )]);
    let plan = transpile::plan(&root.join("payment.ts"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("pub enum Payment {"),
        "{}",
        plan.output
    );
    // The literal field told the members apart; the variant name carries that now.
    assert!(
        plan.output.contains("Card { number: String },"),
        "the discriminator is plumbing and must not become a field:\n{}",
        plan.output
    );
    assert_eq!(plan.fidelity.sums, 1);
    assert_eq!(
        plan.fidelity.records, 0,
        "the members are variants, not standalone records"
    );
}

#[test]
fn a_python_union_of_dataclasses_becomes_a_discriminated_union() {
    let (_tmp, root) = workspace(&[(
        "pay.py",
        "from dataclasses import dataclass\n\n@dataclass\nclass Card:\n    number: str\n\n\
         class Cash:\n    pass\n\nPayment = Card | Cash\n",
    )]);
    let plan = transpile::plan(&root.join("pay.py"), Language::TypeScript).expect("a draft");
    assert!(
        plan.output.contains("export type Payment = Card | Cash;"),
        "{}",
        plan.output
    );
    assert!(
        plan.output.contains("readonly kind: \"card\";"),
        "TypeScript's spelling needs the literal field to narrow on:\n{}",
        plan.output
    );
}

#[test]
fn a_sum_round_trips_through_typescript() {
    let (_tmp, root) = workspace(&[("shape.rs", SHAPE_RS)]);
    let ts = transpile::plan(&root.join("shape.rs"), Language::TypeScript)
        .expect("a draft")
        .output;

    let (_tmp2, root2) = workspace(&[("shape.ts", &ts)]);
    let back = transpile::plan(&root2.join("shape.ts"), Language::Rust).expect("a draft");
    assert_eq!(back.fidelity.sums, 1, "{}", back.output);
    assert!(back.output.contains("enum Shape {"), "{}", back.output);
    assert!(
        back.output.contains("Circle { radius: f64 },"),
        "{}",
        back.output
    );
    assert!(back.output.contains("Empty,"), "{}", back.output);
}

#[test]
fn a_sum_round_trips_through_python() {
    let (_tmp, root) = workspace(&[("shape.rs", SHAPE_RS)]);
    let py = transpile::plan(&root.join("shape.rs"), Language::Python)
        .expect("a draft")
        .output;

    let (_tmp2, root2) = workspace(&[("shape.py", &py)]);
    let back = transpile::plan(&root2.join("shape.py"), Language::Rust).expect("a draft");
    assert_eq!(back.fidelity.sums, 1, "{}", back.output);
    assert!(
        back.output.contains("Circle { radius: f64 },"),
        "{}",
        back.output
    );
}

#[test]
fn an_explicit_discriminant_is_kept_as_words() {
    let (_tmp, root) = workspace(&[(
        "status.rs",
        "pub enum Status {\n    Ok = 200,\n    Missing = 404,\n}\n",
    )]);
    let plan = transpile::plan(&root.join("status.rs"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains("the source gave this the value `200`"),
        "a discriminant that goes has to go out loud:\n{}",
        plan.output
    );
}
