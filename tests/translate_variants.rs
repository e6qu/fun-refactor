//! A value of a closed choice crosses as the variant it is.

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
        "Python calls the variant's own constructor.\n{py}"
    );
    let ts = translated(SHAPES_RS, "shapes.rs", Language::TypeScript);
    assert!(
        ts.contains("return { kind: \"point\" };")
            && ts.contains("return { kind: \"circle\", radius: n };"),
        "TypeScript writes the discriminator the type declared.\n{ts}"
    );
    let go = translated(SHAPES_RS, "shapes.rs", Language::Go);
    assert!(
        go.contains("return (Point{})") && go.contains("return (Circle{Radius: n})"),
        "Go builds the variant struct, parenthesised out of the composite-literal trap.\n{go}"
    );
    let java = translated(SHAPES_RS, "shapes.rs", Language::Java);
    assert!(
        java.contains("return new Point();") && java.contains("return new Circle(n);"),
        "Java calls the record constructor positionally.\n{java}"
    );
}

#[test]
fn vec_new_is_the_empty_list_it_builds() {
    let source = "pub fn build() -> Vec<i64> {\n    let mut items = Vec::new();\n    \
        items.push(1);\n    items\n}\n";
    let out = translated(source, "build.rs", Language::Python);
    assert!(
        out.contains("items: list[int] = []"),
        "the binding is the empty list, typed:\n{out}"
    );
    assert!(!out.contains("None()"), "a marker must not run:\n{out}");
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
fn go_composite_literals_settle_as_variants_or_records() {
    let source = "package shape\n\ntype Shape interface{ isShape() }\n\n\
        type Point struct{}\n\nfunc (Point) isShape() {}\n\n\
        type Circle struct {\n\tRadius float64\n}\n\nfunc (Circle) isShape() {}\n\n\
        func Pick(n float64) Shape {\n\tif n <= 0 {\n\t\treturn Point{}\n\t}\n\t\
        return Circle{Radius: n}\n}\n";
    let rust = translated(source, "shape.go", Language::Rust);
    assert!(
        rust.contains("return Shape::Point;")
            && rust.contains("return Shape::Circle { radius: n };"),
        "a composite literal of a consumed struct is that variant:\n{rust}"
    );
}

#[test]
fn typescript_kind_literals_settle_as_variants() {
    // The named-interface form and the inline form spell one idiom; both cross.
    let named = "interface Point {\n  kind: \"point\";\n}\n\n\
        interface Circle {\n  kind: \"circle\";\n  radius: number;\n}\n\n\
        export type Shape = Point | Circle;\n\n\
        export function pick(n: number): Shape {\n  if (n <= 0) {\n    \
        return { kind: \"point\" };\n  }\n  return { kind: \"circle\", radius: n };\n}\n";
    let rust = translated(named, "shape.ts", Language::Rust);
    assert!(
        rust.contains("return Shape::Point;")
            && rust.contains("return Shape::Circle { radius: n };"),
        "an object literal naming a variant is that variant:\n{rust}"
    );
    let inline = "type Shape = { kind: \"point\" } | { kind: \"circle\"; radius: number };\n\n\
        export function pick(n: number): Shape {\n  if (n <= 0) {\n    \
        return { kind: \"point\" };\n  }\n  return { kind: \"circle\", radius: n };\n}\n";
    let rust = translated(inline, "shape.ts", Language::Rust);
    assert!(
        rust.contains("enum Shape") && rust.contains("return Shape::Point;"),
        "the inline union is the same sum, variants named by their literals:\n{rust}"
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
