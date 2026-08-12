//! Java's class hierarchy, which the call graph did not read.
//!
//! Class hierarchy analysis answers "who could this call reach?" for a call through an
//! abstraction. Java states that hierarchy more plainly than any other language here,
//! `implements` is a keyword. It was the one language whose hierarchy went unread, because it
//! fell into the same catch-all as Zig and Bash, which genuinely have none.

use fun_refactor::analysis::call_graph::{CallGraph, HierarchyBasis};
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directory");
        }
        std::fs::write(path, content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    (tmp, index)
}

fn callers_in(index: &Index, qualified: &str) -> Vec<String> {
    let id = index
        .symbols
        .iter()
        .find(|s| s.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no `{qualified}`"))
        .id;
    CallGraph::build(index)
        .callers(id)
        .into_iter()
        .filter_map(|(caller, _)| index.symbol(caller).map(|s| s.qualified_name()))
        .collect()
}

fn callers_of(files: &[(&str, &str)], qualified: &str) -> Vec<String> {
    let (_tmp, index) = workspace(files);
    callers_in(&index, qualified)
}

const SHAPES: &str = "\
interface Shape {
    double area();
}

class Circle implements Shape {
    public double area() { return 3.0; }
}

class Square implements Shape {
    public double area() { return 4.0; }
}

public class Report {
    static double total(Shape s) { return s.area(); }
}
";

#[test]
fn a_call_through_an_interface_reaches_every_implementation() {
    for implementation in ["Circle::area", "Square::area"] {
        let callers = callers_of(&[("Shape.java", SHAPES)], implementation);
        assert!(
            callers.iter().any(|c| c == "Report::total"),
            "{implementation} should be reachable from Report::total, got {callers:?}"
        );
    }
}

#[test]
fn a_call_through_a_base_class_reaches_every_subclass() {
    let source = "\
abstract class Element {
    abstract Element copy();
}

class Leaf extends Element {
    Element copy() { return new Leaf(); }
}

class Tree extends Element {
    Element copy() { return new Tree(); }
    Element duplicate(Element other) { return other.copy(); }
}
";
    let callers = callers_of(&[("Element.java", source)], "Leaf::copy");
    assert!(
        callers.iter().any(|c| c == "Tree::duplicate"),
        "got {callers:?}"
    );
}

#[test]
fn an_enum_that_implements_an_interface_is_in_the_hierarchy() {
    // `enum X implements I` wraps its methods in one more level than a class does, so
    // reading only the direct children of the body finds none of them.
    let source = "\
interface Naming {
    String translate(String name);
}

enum Policy implements Naming {
    UPPER,
    LOWER;

    public String translate(String name) { return name; }
}

class Uses {
    static String go(Naming n) { return n.translate(\"x\"); }
}
";
    let callers = callers_of(&[("Policy.java", source)], "Policy::translate");
    assert!(callers.iter().any(|c| c == "Uses::go"), "got {callers:?}");
}

#[test]
fn a_type_argument_is_not_a_supertype() {
    // `implements Holder<Pet>` says nothing about Pet. Taking every type name under the clause
    // made the argument a supertype too. So a call reaching `Box::name` by its name alone was
    // reported as reaching it through a relationship somebody declared, a guess presented as a
    // fact, which is the one thing this layer must not do. The edge is the same either way; the
    // evidence for it is not.
    let source = "\
interface Holder<T> {
    void put(T item);
}

class Pet {
    String name() { return \"p\"; }
}

class Box implements Holder<Pet> {
    public void put(Pet item) { }
    public String name() { return \"b\"; }
}

class Uses {
    static String go(Pet p) { return p.name(); }
}
";
    let (_tmp, index) = workspace(&[("Box.java", source)]);
    let target = index
        .symbols
        .iter()
        .find(|s| s.qualified_name() == "Box::name")
        .expect("no `Box::name`")
        .id;
    let graph = CallGraph::build(&index);
    for (caller, basis) in graph.hierarchy_callers(target) {
        let name = index.symbol(caller).map(|s| s.qualified_name());
        assert_ne!(
            basis,
            HierarchyBasis::DeclaredSupertype,
            "{name:?} reaches Box::name through a supertype nobody declared"
        );
    }
}

#[test]
fn a_nested_class_keeps_its_own_methods() {
    // Collecting methods by walking the whole body would give the outer type every
    // method of every class declared inside it.
    let source = "\
interface Shape {
    double area();
}

class Outer implements Shape {
    public double area() { return 1.0; }

    static class Inner {
        double volume() { return 2.0; }
    }
}

class Uses {
    static double go(Shape s) { return s.area(); }
}
";
    let callers = callers_of(&[("Outer.java", source)], "Inner::volume");
    assert!(
        !callers.iter().any(|c| c == "Uses::go"),
        "a nested class's method was attributed to its enclosing type: {callers:?}"
    );
}
