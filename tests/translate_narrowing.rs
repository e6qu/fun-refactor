//! Consuming a sum value crosses: the question "which variant is this?"
//!
//! The construction crossed a pass before the consumption did. `s.kind ==
//! "circle"` and `s.radius` went to Rust verbatim, against an enum that
//! declares neither, while the header said every signature carried. Each
//! language now asks its own way. TypeScript compares the discriminator,
//! Python asks `isinstance`, Rust matches, Go switches on type, Java tests
//! `instanceof` and sheds the cast.

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

const AREA_TS: &str = "interface Point {\n  kind: \"point\";\n}\n\n\
    interface Circle {\n  kind: \"circle\";\n  radius: number;\n}\n\n\
    export type Shape = Point | Circle;\n\n\
    export function area(s: Shape): number {\n  if (s.kind === \"circle\") {\n    \
    return 3.14 * s.radius * s.radius;\n  }\n  return 0;\n}\n";

#[test]
fn a_kind_check_narrows_in_every_target() {
    let rust = translated(AREA_TS, "area.ts", Language::Rust);
    assert!(
        rust.contains("match s {")
            && rust.contains("Shape::Circle { radius, .. } => {")
            && rust.contains("return 3.14 * radius * radius;"),
        "Rust matches and binds the payload:\n{rust}"
    );
    assert!(
        rust.contains("return 0.0;"),
        "an integer literal under a float signature gains its point:\n{rust}"
    );
    let py = translated(AREA_TS, "area.ts", Language::Python);
    assert!(
        py.contains("if isinstance(s, Circle):") && py.contains("radius = s.radius"),
        "Python asks isinstance and binds the payload:\n{py}"
    );
    let go = translated(AREA_TS, "area.ts", Language::Go);
    assert!(
        go.contains("switch v := s.(type) {")
            && go.contains("case Circle:")
            && go.contains("radius := v.Radius"),
        "Go switches on the type:\n{go}"
    );
    let java = translated(AREA_TS, "area.ts", Language::Java);
    assert!(
        java.contains("if (s instanceof Circle) {")
            && java.contains("var radius = ((Circle) s).radius();"),
        "Java tests instanceof and reads the accessor:\n{java}"
    );
}

#[test]
fn two_sums_sharing_a_tag_settle_by_the_declared_type() {
    let source = "interface FIdle { kind: \"idle\" }\n\
        interface FBusy { kind: \"busy\"; url: string }\n\
        type Fetch = FIdle | FBusy;\n\n\
        interface SIdle { kind: \"idle\" }\n\
        interface SDone { kind: \"done\" }\n\
        type Save = SIdle | SDone;\n\n\
        export function fetchState(): Fetch {\n  return { kind: \"idle\" };\n}\n\n\
        export function saveState(): Save {\n  return { kind: \"idle\" };\n}\n";
    let rust = translated(source, "twosums.ts", Language::Rust);
    assert!(
        rust.contains("return Fetch::FIdle;") && rust.contains("return Save::SIdle;"),
        "the position's declared type says which sum was meant.\n{rust}"
    );
    assert!(
        !rust.contains("HashMap"),
        "no wrong-typed map stands in for a sum value.\n{rust}"
    );
}

#[test]
fn a_variant_dodging_a_name_collision_is_built_under_its_dodge() {
    let source = "pub enum Status {\n    Ok,\n    Failed { code: i64 },\n}\n\n\
        pub struct Ok {\n    pub note: String,\n}\n\n\
        pub fn status(n: i64) -> Status {\n    if n == 0 {\n        return Status::Ok;\n    }\n    \
        Status::Failed { code: n }\n}\n";
    let py = translated(source, "collide.py.rs", Language::Python);
    assert!(
        py.contains("class StatusOk:") && py.contains("return StatusOk()"),
        "the declaration's dodge and the construction agree:\n{py}"
    );
}

#[test]
fn a_member_used_concretely_keeps_its_struct_beside_the_variant() {
    let source = "package inv\n\ntype Shape interface{ isShape() }\n\n\
        type Point struct{}\n\nfunc (Point) isShape() {}\n\n\
        func Standalone() Point {\n\tp := Point{}\n\treturn p\n}\n\n\
        func Pick() Shape {\n\treturn Point{}\n}\n";
    let rust = translated(source, "inv.go", Language::Rust);
    assert!(
        rust.contains("pub fn standalone() -> Point") && rust.contains("let mut p = Point {};"),
        "the concrete position builds the struct:\n{rust}"
    );
    assert!(
        rust.contains("return Shape::Point;"),
        "the sum position builds the variant:\n{rust}"
    );
}

#[test]
fn a_shadowed_union_member_never_settles_as_the_variant() {
    let source = "from dataclasses import dataclass\n\n\n@dataclass\nclass Card:\n    \
        number: str\n\n\n@dataclass\nclass Cash:\n    pass\n\n\nPayment = Card | Cash\n\n\n\
        def pay(number: str) -> str:\n    def Card(n: str) -> str:\n        \
        return \"card:\" + n\n\n    label = Card(number)\n    return label\n";
    let rust = translated(source, "shadow.py", Language::Rust);
    assert!(
        !rust.contains("Payment::Card"),
        "the local definition shadows the sum's member:\n{rust}"
    );
    assert!(
        rust.contains("a call to a shadowed name")
            || rust.contains("not translated: Card(1 argument(s))"),
        "the call carries, visibly:\n{rust}"
    );
}

#[test]
fn a_java_sealed_interface_is_a_sum_end_to_end() {
    let source = "sealed interface Shape permits Point, Circle {}\n\n\
        record Point() implements Shape {}\n\n\
        record Circle(double radius) implements Shape {}\n\n\
        public final class Geo {\n    public static Shape pick(double n) {\n        \
        if (n <= 0) {\n            return new Point();\n        }\n        \
        return new Circle(n);\n    }\n\n    \
        public static double area(Shape s) {\n        // Narrowing under test.\n        \
        if (s instanceof Circle) {\n            \
        var c = (Circle) s;\n            return 3.14 * c.radius() * c.radius();\n        }\n        \
        return 0;\n    }\n}\n";
    let rust = translated(source, "Geo.java", Language::Rust);
    assert!(
        rust.contains("enum Shape") && rust.contains("Circle { radius: f64 }"),
        "the sealed interface is the sum it declares.\n{rust}"
    );
    assert!(
        rust.contains("return Shape::Point;")
            && rust.contains("return Shape::Circle { radius: n };"),
        "constructions cross as variants:\n{rust}"
    );
    assert!(
        rust.contains("Shape::Circle { radius, .. } => {")
            && rust.contains("return 3.14 * radius * radius;"),
        "instanceof and the cast collapse into the match.\n{rust}"
    );
    assert!(
        rust.contains("if n <= 0.0 {"),
        "a float parameter's comparison literal gains its point:\n{rust}"
    );
}
