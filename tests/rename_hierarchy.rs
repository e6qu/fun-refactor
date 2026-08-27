//! A method that participates in declared dispatch renames as one family.

use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::Path;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn symbol_at(
    index: &Index,
    path: &Path,
    source: &str,
    needle: &str,
) -> fun_refactor::model::SymbolId {
    let offset = source.find(needle).expect("the needle") + needle.len() - 1;
    index
        .symbols
        .iter()
        .find(|s| s.file == path && s.name_span.contains_offset(offset))
        .expect("a symbol at the needle")
        .id
}

fn applied(index_root: &Path, file: &str, plan: &rename::RenamePlan) -> String {
    let path = index_root.join(file);
    let before = std::fs::read_to_string(&path).unwrap();
    match plan.edits.edits_for(&path) {
        Some(edits) => fun_refactor::edit::apply_to_string(&before, edits).unwrap(),
        None => before,
    }
}

const SHAPES_RS: &str = "pub trait Shape {\n    fn area(&self) -> f64;\n}\n\n\
    pub struct Circle {\n    pub radius: f64,\n}\n\n\
    impl Shape for Circle {\n    fn area(&self) -> f64 {\n        self.radius * 2.0\n    }\n}\n\n\
    pub fn total(shapes: &[Box<dyn Shape>]) -> f64 {\n    shapes.iter().map(|s| s.area()).sum()\n}\n";

#[test]
fn renaming_the_trait_method_renames_the_implementations() {
    let (tmp, index) = workspace(&[("shapes.rs", SHAPES_RS)]);
    let id = symbol_at(&index, &tmp.path().join("shapes.rs"), SHAPES_RS, "fn area");
    let plan = rename::plan(&index, id, "surface").unwrap();
    let out = applied(tmp.path(), "shapes.rs", &plan);
    assert!(
        !out.contains("fn area"),
        "no member of the family keeps the old name.\n{out}"
    );
    assert!(
        out.contains("fn surface(&self) -> f64;") && out.contains("fn surface(&self) -> f64 {"),
        "declaration and implementation rename together.\n{out}"
    );
}

#[test]
fn renaming_the_implementation_renames_the_trait_method_too() {
    let (tmp, index) = workspace(&[("shapes.rs", SHAPES_RS)]);
    let impl_at = SHAPES_RS.rfind("fn area").unwrap() + 3;
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == "area" && s.name_span.contains_offset(impl_at))
        .expect("the implementation method")
        .id;
    let plan = rename::plan(&index, id, "surface").unwrap();
    let out = applied(tmp.path(), "shapes.rs", &plan);
    assert!(
        !out.contains("fn area"),
        "the family renames from either end.\n{out}"
    );
}

#[test]
fn the_dispatch_site_renames_and_is_reported_at_its_confidence() {
    let (tmp, index) = workspace(&[("shapes.rs", SHAPES_RS)]);
    let id = symbol_at(&index, &tmp.path().join("shapes.rs"), SHAPES_RS, "fn area");
    let plan = rename::plan(&index, id, "surface").unwrap();
    let out = applied(tmp.path(), "shapes.rs", &plan);
    assert!(
        out.contains("s.surface()"),
        "the call through the trait object follows the family.\n{out}"
    );
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.kind == fun_refactor::refactor::WarningKind::DispatchCandidate),
        "a dispatch site is renamed and said, for a person to review: {:?}",
        plan.warnings
    );
}

#[test]
fn a_typescript_interface_family_renames_together() {
    let source = "interface Carrier {\n    quote(kg: number): number;\n}\n\n\
        class Post implements Carrier {\n    quote(kg: number): number {\n        \
        return kg * 120;\n    }\n}\n\n\
        export function cheapest(carriers: Carrier[], kg: number): number {\n    \
        return Math.min(...carriers.map((c) => c.quote(kg)));\n}\n";
    let (tmp, index) = workspace(&[("shapes.ts", source)]);
    let id = symbol_at(&index, &tmp.path().join("shapes.ts"), source, "    quote");
    let plan = rename::plan(&index, id, "price").unwrap();
    let out = applied(tmp.path(), "shapes.ts", &plan);
    assert!(!out.contains("quote("), "{out}");
    assert!(
        out.contains("price(kg: number): number;") && out.contains("c.price(kg)"),
        "declaration, implementation and dispatch site all follow:\n{out}"
    );
}

#[test]
fn a_method_outside_any_hierarchy_renames_alone() {
    let source = "pub struct Lone {\n    pub n: f64,\n}\n\n\
        impl Lone {\n    fn area(&self) -> f64 {\n        self.n\n    }\n}\n\n\
        pub struct Other;\n\n\
        impl Other {\n    fn area(&self) -> f64 {\n        1.0\n    }\n}\n";
    let (tmp, index) = workspace(&[("lone.rs", source)]);
    let first = source.find("fn area").unwrap() + 3;
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == "area" && s.name_span.contains_offset(first))
        .expect("the first method")
        .id;
    let plan = rename::plan(&index, id, "surface").unwrap();
    let out = applied(tmp.path(), "lone.rs", &plan);
    assert!(
        out.contains("fn surface") && out.matches("fn area").count() == 1,
        "a same-named method on an unrelated type is not family:\n{out}"
    );
}

#[test]
fn java_overloads_rename_with_every_call_that_only_they_answer() {
    // Both declarations of `size` rename as one entity.
    let source = "public class App {\n    static int size(String s) {\n        \
        return s.length();\n    }\n\n    static int size(int[] items) {\n        \
        return items.length;\n    }\n\n    public static void main(String[] args) {\n        \
        System.out.println(size(\"hello\") + size(new int[] { 1, 2 }));\n    }\n}\n";
    let (tmp, index) = workspace(&[("App.java", source)]);
    let id = symbol_at(
        &index,
        &tmp.path().join("App.java"),
        source,
        "static int size",
    );
    let plan = rename::plan(&index, id, "len").unwrap();
    let out = applied(tmp.path(), "App.java", &plan);
    assert!(
        !out.contains("size("),
        "no call keeps the dead name:\n{out}"
    );
    assert_eq!(
        out.matches("len(").count(),
        4,
        "two declarations and two calls:\n{out}"
    );
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.kind == fun_refactor::refactor::WarningKind::DispatchCandidate),
        "each renamed call is said, for a person to review: {:?}",
        plan.warnings
    );
}

#[test]
fn a_stranger_answering_the_same_name_keeps_the_calls_in_place() {
    // `Other` also declares a `size`, outside the renamed group.
    let source = "public class App {\n    static int size(String s) {\n        \
        return s.length();\n    }\n\n    static int size(int[] items) {\n        \
        return items.length;\n    }\n\n    public static void main(String[] args) {\n        \
        System.out.println(size(\"hello\"));\n    }\n}\n";
    let other = "public class Other {\n    static int size(double d) {\n        \
        return (int) d;\n    }\n}\n";
    let (tmp, index) = workspace(&[("App.java", source), ("Other.java", other)]);
    let id = symbol_at(
        &index,
        &tmp.path().join("App.java"),
        source,
        "static int size",
    );
    let plan = rename::plan(&index, id, "len").unwrap();
    let out = applied(tmp.path(), "App.java", &plan);
    assert!(
        out.contains("size(\"hello\")"),
        "the call could reach the stranger, so it stays.\n{out}"
    );
}

#[test]
fn typescript_overload_signatures_rename_with_their_implementation() {
    // Two `function pick` signatures over one implementation are one function; renaming any
    // alone left `error TS2389: Function implementation name must be 'pick'`.
    let source = "export function pick(value: string): string;\n\
        export function pick(value: number): number;\n\
        export function pick(value: string | number): string | number {\n    \
        return value;\n}\n\nexport const chosen = pick(\"a\");\n";
    let (tmp, index) = workspace(&[("over.ts", source)]);
    let at = source.rfind("function pick").expect("the implementation") + 10;
    let id = index
        .symbols
        .iter()
        .find(|sym| sym.name == "pick" && sym.name_span.contains_offset(at))
        .expect("the implementation symbol")
        .id;
    let plan = rename::plan(&index, id, "choose").unwrap();
    let out = applied(tmp.path(), "over.ts", &plan);
    assert_eq!(
        out.matches("function choose").count(),
        3,
        "both signatures and the implementation:\n{out}"
    );
    assert!(out.contains("choose(\"a\")"), "and the call:\n{out}");
}

#[test]
fn a_receiver_with_a_declared_type_outside_the_family_holds_its_call() {
    // `b` is declared `B`, and `B` has its own `size`; renaming A's overloads
    // took `b.size(2)` with them as a dispatch candidate, and javac refused.
    let shapes = "public class A {\n    int size(int n) {\n        return n;\n    }\n\n    \
        int size(String s) {\n        return s.length();\n    }\n}\n\n\
        class B {\n    int size(int n) {\n        return n + 1;\n    }\n}\n";
    // A caller in a second file.
    let main = "public class Main {\n    public static void main(String[] args) {\n        \
        A a = new A();\n        B b = new B();\n        \
        System.out.println(a.size(1) + b.size(2) + a.size(\"hey\"));\n    }\n}\n";
    let (tmp, index) = workspace(&[("Shapes.java", shapes), ("Main.java", main)]);
    let id = symbol_at(&index, &tmp.path().join("Shapes.java"), shapes, "int size");
    let plan = rename::plan(&index, id, "count").unwrap();
    let out = applied(tmp.path(), "Main.java", &plan);
    assert!(
        out.contains("a.count(1)") && out.contains("a.count(\"hey\")"),
        "the family's own calls follow:\n{out}"
    );
    assert!(
        out.contains("b.size(2)"),
        "the declared type says this call is not the family's:\n{out}"
    );
}
