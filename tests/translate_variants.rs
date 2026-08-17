//! A value of a closed choice crosses as the variant it is.
//!
//! The types crossed for eleven passes while every value of one carried, in
//! every direction at once: `Shape::Point` reached Python as a comment, and a
//! Zig `.{ .one = n }` took its whole `if` with it. Each language builds the
//! same thing its own way, and a path that names anything else, `Vec::new`,
//! an enum from another crate, goes back to being carried, which is what every
//! such path was before.

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

const SHAPES_RS: &str = "pub enum Shape {\n    Point,\n    Circle { radius: f64 },\n}\n\n\
    pub fn pick(n: f64) -> Shape {\n    if n <= 0.0 {\n        return Shape::Point;\n    }\n    \
    Shape::Circle { radius: n }\n}\n";

#[test]
fn rust_variants_reach_every_target_as_constructions() {
    let py = translated(SHAPES_RS, "shapes.rs", Language::Python);
    assert!(
        py.contains("return Point()") && py.contains("return Circle(radius=n)"),
        "Python calls the variant's own constructor:\n{py}"
    );
    let ts = translated(SHAPES_RS, "shapes.rs", Language::TypeScript);
    assert!(
        ts.contains("return { kind: \"point\" };")
            && ts.contains("return { kind: \"circle\", radius: n };"),
        "TypeScript writes the discriminator the type declared:\n{ts}"
    );
    let go = translated(SHAPES_RS, "shapes.rs", Language::Go);
    assert!(
        go.contains("return (Point{})") && go.contains("return (Circle{Radius: n})"),
        "Go builds the variant struct, parenthesised out of the composite-literal trap:\n{go}"
    );
    let java = translated(SHAPES_RS, "shapes.rs", Language::Java);
    assert!(
        java.contains("return new Point();") && java.contains("return new Circle(n);"),
        "Java calls the record constructor positionally:\n{java}"
    );
}

#[test]
fn a_path_naming_no_sum_of_the_module_still_carries() {
    let source = "pub fn build() -> Vec<i64> {\n    let mut items = Vec::new();\n    \
        items.push(1);\n    items\n}\n";
    let out = translated(source, "build.rs", Language::Python);
    assert!(
        out.contains("items = None") && out.contains("Vec::new"),
        "the binding stays and the call is carried, never called as a marker:\n{out}"
    );
    assert!(
        !out.contains("None()"),
        "a marker must not run:\n{out}"
    );
}

#[test]
fn zig_anonymous_variants_settle_against_the_modules_own_unions() {
    let source = "pub const Pick = union(enum) {\n    none: void,\n    one: u32,\n};\n\n\
        pub fn pick(n: u32) Pick {\n    if (n == 0) {\n        return .{ .none = {} };\n    }\n    \
        return .{ .one = n };\n}\n";
    let rust = translated(source, "pick.zig", Language::Rust);
    assert!(
        rust.contains("return Pick::None;") && rust.contains("return Pick::One { value: n };"),
        "the union the position expects is settled from the module:\n{rust}"
    );
}

#[test]
fn python_union_members_construct_as_variants() {
    let source = "from dataclasses import dataclass\n\n\n@dataclass\nclass Card:\n    \
        number: str\n\n\n@dataclass\nclass Cash:\n    pass\n\n\nPayment = Card | Cash\n\n\n\
        def pay(cash: bool, number: str) -> Payment:\n    if cash:\n        return Cash()\n    \
        return Card(number)\n";
    let rust = translated(source, "payment.py", Language::Rust);
    assert!(
        rust.contains("return Payment::Cash;")
            && rust.contains("return Payment::Card { number: number };"),
        "a consumed class constructs as its variant, positionally matched:\n{rust}"
    );
}
