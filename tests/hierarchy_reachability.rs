//! Class hierarchy analysis: what a call through an abstraction can reach.
//!
//! A call through a `dyn Trait`, an interface value or a base-class reference names no single
//! definition. So resolution refuses to pick one and the call graph used to stop there, leaving
//! every implementation looking dead. The workspace does say which types implement the
//! abstraction, and these tests pin down what that buys, per language, and what it costs in
//! precision.
//!
//! Two rules run through all of it. A hierarchy edge is never `Exact`: which implementation
//! runs is a runtime fact and the tag has to say so. And where the syntax cannot separate an
//! implementation from a same-named method on an unrelated type, the test asserts the
//! over-approximation instead of pretending to precision.

use fun_refactor::analysis::entrypoints::Entrypoints;
use fun_refactor::{
    analysis::call_graph::{CallGraph, EdgeOrigin, HierarchyBasis},
    index::Index,
    model::{Confidence, SymbolId},
    refactor::delete,
    scan::{scan, ScanOptions},
};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

/// The one method named `Owner::name`.
fn method(index: &Index, owner: &str, name: &str) -> SymbolId {
    let found: Vec<_> = index
        .symbols
        .iter()
        .filter(|s| s.name == name && s.qualifier.as_deref() == Some(owner))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected one {owner}::{name}, got {found:?}"
    );
    found[0].id
}

fn only(index: &Index, name: &str) -> SymbolId {
    let found = index.find_symbols(name, None);
    assert_eq!(found.len(), 1, "expected one '{name}', got {found:?}");
    found[0].id
}

/// The evidence for the edge from `from` to `to`, if there is one.
fn edge(graph: &CallGraph, from: SymbolId, to: SymbolId) -> Option<(Confidence, EdgeOrigin)> {
    graph
        .callers(to)
        .into_iter()
        .find(|(caller, _)| *caller == from)
        .map(|(_, edge)| (edge.confidence, edge.origin))
}

/// Assert `from` reaches `to` through hierarchy analysis, on the stated evidence.
#[track_caller]
fn assert_dispatches(
    index: &Index,
    graph: &CallGraph,
    from: SymbolId,
    to: SymbolId,
    basis: HierarchyBasis,
) {
    let name = |id: SymbolId| {
        index
            .symbol(id)
            .map(|s| s.qualified_name())
            .unwrap_or_else(|| "<unknown>".into())
    };
    let found =
        edge(graph, from, to).unwrap_or_else(|| panic!("no edge {} -> {}", name(from), name(to)));
    assert_eq!(
        found,
        (Confidence::FieldBased, EdgeOrigin::Hierarchy(basis)),
        "{} -> {} should be an unproven dispatch candidate",
        name(from),
        name(to)
    );
}

/// No edge into `id` may claim certainty; a dispatch candidate is never `Exact`.
#[track_caller]
fn assert_never_exact(graph: &CallGraph, id: SymbolId) {
    for (_, edge) in graph.callers(id) {
        assert!(
            !edge.confidence.is_safe_to_rewrite(),
            "a dispatch candidate must not be tagged {}",
            edge.confidence.as_str()
        );
    }
}

// ------------------------------------------------------------------------- Rust

/// A trait, two impls, and one call through `&dyn Trait`.
///
/// `Ledger` lives in its own file with an inherent `area`: nothing relates it to
/// `Shape`, and nothing calls it.
fn rust_shapes() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "shapes.rs",
            "\
trait Shape {
    fn area(&self) -> f64;
}

struct Circle;
struct Square;

impl Shape for Circle {
    fn area(&self) -> f64 { 1.0 }
}

impl Shape for Square {
    fn area(&self) -> f64 { 2.0 }
}

fn report(shape: &dyn Shape) -> f64 {
    shape.area()
}

fn main() {
    report(&Circle);
}
",
        ),
        (
            "ledger.rs",
            "struct Ledger;\n\nimpl Ledger {\n    fn area(&self) -> f64 { 3.0 }\n}\n",
        ),
    ]
}

#[test]
fn rust_a_dyn_call_reaches_every_impl_of_the_trait() {
    let (_tmp, index) = workspace(&rust_shapes());
    let graph = CallGraph::build(&index);
    let report = only(&index, "report");

    for owner in ["Circle", "Square"] {
        assert_dispatches(
            &index,
            &graph,
            report,
            method(&index, owner, "area"),
            HierarchyBasis::ImplementedTrait,
        );
    }
    // The trait's own declaration is named by the call too, and stays live with it.
    assert_dispatches(
        &index,
        &graph,
        report,
        method(&index, "Shape", "area"),
        HierarchyBasis::ImplementedTrait,
    );
}

#[test]
fn rust_a_same_named_method_on_an_unrelated_type_gets_no_edge() {
    // `Ledger::area` sits in an inherent impl. Nothing says Ledger is a Shape, so the
    // call through `&dyn Shape` cannot reach it and no edge may claim otherwise.
    let (_tmp, index) = workspace(&rust_shapes());
    let graph = CallGraph::build(&index);

    assert!(
        graph.callers(method(&index, "Ledger", "area")).is_empty(),
        "an inherent impl is not an implementation of the trait"
    );
}

#[test]
fn rust_a_generic_bound_dispatches_like_a_trait_object() {
    // `T: Shape` names no impl either, the call site is the same problem.
    let source = "\
trait Shape { fn area(&self) -> f64; }
struct Circle;
impl Shape for Circle { fn area(&self) -> f64 { 1.0 } }
fn report<T: Shape>(shape: &T) -> f64 { shape.area() }
fn main() { report(&Circle); }
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    let graph = CallGraph::build(&index);
    assert_dispatches(
        &index,
        &graph,
        only(&index, "report"),
        method(&index, "Circle", "area"),
        HierarchyBasis::ImplementedTrait,
    );
}

#[test]
fn rust_a_supertrait_call_reaches_the_subtrait_implementors() {
    // `impl Round for Ball` makes Ball a Shape, because `trait Round: Shape` says so.
    let source = "\
trait Shape { fn area(&self) -> f64; }
trait Round: Shape { fn radius(&self) -> f64; }
struct Ball;
impl Shape for Ball { fn area(&self) -> f64 { 1.0 } }
impl Round for Ball { fn radius(&self) -> f64 { 2.0 } }
fn report(shape: &dyn Shape) -> f64 { shape.area() }
fn main() { report(&Ball); }
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    let graph = CallGraph::build(&index);
    assert_dispatches(
        &index,
        &graph,
        only(&index, "report"),
        method(&index, "Ball", "area"),
        HierarchyBasis::ImplementedTrait,
    );
}

#[test]
fn rust_an_impl_reached_only_by_dispatch_is_not_reported_unused() {
    let (_tmp, index) = workspace(&rust_shapes());
    let report = delete::find_unused_report(&index, &Entrypoints::exactly(&[only(&index, "main")]));

    for owner in ["Circle", "Square"] {
        let id = method(&index, owner, "area");
        assert!(
            !report.unused.contains(&id),
            "{owner}::area is reached through &dyn Shape: {:?}",
            report.unused
        );
    }
    // And the report can say why, and not quietly dropping them.
    let explanation = report
        .explain(&index, method(&index, "Circle", "area"))
        .expect("a spared symbol must carry its reason");
    assert!(
        explanation.contains("dynamic dispatch") && explanation.contains("implemented-trait"),
        "got {explanation}"
    );
}

#[test]
fn rust_an_unrelated_method_nothing_calls_is_still_reported_unused() {
    // The negative half: sparing implementations must not spare everything.
    let (_tmp, index) = workspace(&rust_shapes());
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[only(&index, "main")]));
    assert!(
        unused.contains(&method(&index, "Ledger", "area")),
        "nothing reaches Ledger::area: {unused:?}"
    );
}

#[test]
fn rust_a_method_call_that_resolves_exactly_stays_exact() {
    // `self.helper()` names one definition. Hierarchy analysis must not touch it, and the edge
    // must exist at all, which it did not before: Rust's queries file `x.m()` as a field
    // access. So the call graph never saw it.
    let source = "\
struct S;
impl S {
    fn helper(&self) {}
    fn run(&self) { self.helper(); }
}
fn main() { S.run(); }
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    let graph = CallGraph::build(&index);
    let found = edge(
        &graph,
        method(&index, "S", "run"),
        method(&index, "S", "helper"),
    );
    assert_eq!(found, Some((Confidence::Exact, EdgeOrigin::Resolved)));
    assert_eq!(graph.hierarchy_edge_count(), 0, "nothing here is dynamic");
}

// --------------------------------------------------------------------------- Go

/// A Go package split across files, so the index cannot cheat with a same-file match.
fn go_shapes() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "shape.go",
            "package shapes\n\ntype Shape interface {\n\tArea() float64\n}\n",
        ),
        (
            "circle.go",
            "package shapes\n\ntype Circle struct{}\n\nfunc (c Circle) Area() float64 { return 1 }\n",
        ),
        (
            "square.go",
            "package shapes\n\ntype Square struct{}\n\nfunc (s Square) Area() float64 { return 2 }\n",
        ),
        (
            "ledger.go",
            // Same method name, different arity: its method set does not cover the
            // interface's, so it is not an implementation of it.
            "package shapes\n\ntype Ledger struct{}\n\nfunc (l Ledger) Area(scale float64) float64 { return 3 }\n",
        ),
        (
            "report.go",
            "package shapes\n\nfunc Report(s Shape) float64 {\n\treturn s.Area()\n}\n\nfunc Main() {\n\tReport(Circle{})\n}\n",
        ),
    ]
}

#[test]
fn go_an_interface_call_reaches_every_type_whose_method_set_covers_it() {
    let (_tmp, index) = workspace(&go_shapes());
    let graph = CallGraph::build(&index);
    let report = only(&index, "Report");

    for owner in ["Circle", "Square", "Shape"] {
        assert_dispatches(
            &index,
            &graph,
            report,
            method(&index, owner, "Area"),
            HierarchyBasis::InterfaceMethodSet,
        );
    }
}

#[test]
fn go_a_method_set_that_does_not_cover_the_interface_gets_no_edge() {
    // `Ledger.Area(float64)` takes an argument, so Ledger does not implement Shape, and this is
    // as far as syntax can separate them. A no-argument `Area()` on an unrelated type *would*
    // get an edge, because in Go that type really does implement the interface. There is no
    // `implements` keyword to disagree with.
    let (_tmp, index) = workspace(&go_shapes());
    let graph = CallGraph::build(&index);
    assert!(
        graph.callers(method(&index, "Ledger", "Area")).is_empty(),
        "arity separates these two"
    );
}

#[test]
fn go_structural_typing_over_approximates_and_the_test_says_so() {
    // The honest half of the same coin. `Timer.Area()` never meets a Shape anywhere in this
    // workspace. But its method set covers the interface, which is the whole of what
    // implementing an interface means in Go. The edge is real by the language's rule even
    // though no human would call Timer a shape, and it is tagged unproven.
    let mut files = go_shapes();
    files.push((
        "timer.go",
        "package shapes\n\ntype Timer struct{}\n\nfunc (t Timer) Area() float64 { return 0 }\n",
    ));
    let (_tmp, index) = workspace(&files);
    let graph = CallGraph::build(&index);

    assert_dispatches(
        &index,
        &graph,
        only(&index, "Report"),
        method(&index, "Timer", "Area"),
        HierarchyBasis::InterfaceMethodSet,
    );
    assert_never_exact(&graph, method(&index, "Timer", "Area"));
}

#[test]
fn go_an_implementation_reached_only_by_dispatch_is_not_reported_unused() {
    let (_tmp, index) = workspace(&go_shapes());
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[only(&index, "Main")]));
    for owner in ["Circle", "Square"] {
        assert!(
            !unused.contains(&method(&index, owner, "Area")),
            "{owner}.Area implements Shape: {unused:?}"
        );
    }
    assert!(
        unused.contains(&method(&index, "Ledger", "Area")),
        "nothing reaches Ledger.Area: {unused:?}"
    );
}

// ------------------------------------------------------------------- TypeScript

fn ts_shapes() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "shape.ts",
            "export interface Shape {\n  area(): number;\n}\n",
        ),
        (
            "circle.ts",
            "import { Shape } from './shape';\nexport class Circle implements Shape {\n  area(): number { return 1; }\n}\n",
        ),
        (
            "square.ts",
            "import { Shape } from './shape';\nexport class Square implements Shape {\n  area(): number { return 2; }\n}\n",
        ),
        (
            "ledger.ts",
            "export class Ledger {\n  area(): number { return 3; }\n}\n",
        ),
        (
            "report.ts",
            "import { Shape } from './shape';\nimport { Circle } from './circle';\nexport function report(s: Shape): number {\n  return s.area();\n}\nexport function main(): void {\n  report(new Circle());\n}\n",
        ),
    ]
}

#[test]
fn typescript_an_implements_clause_reaches_both_classes() {
    let (_tmp, index) = workspace(&ts_shapes());
    let graph = CallGraph::build(&index);
    let report = only(&index, "report");

    for owner in ["Circle", "Square"] {
        assert_dispatches(
            &index,
            &graph,
            report,
            method(&index, owner, "area"),
            HierarchyBasis::DeclaredSupertype,
        );
    }
}

#[test]
fn typescript_an_unrelated_class_is_reached_by_name_alone_and_labelled_that_way() {
    // The precision cost, taken deliberately. `Ledger` implements nothing, but most TypeScript
    // never writes `implements` at all. Bucketing call sites by method name is what buys the
    // recall (~66-80% precision, >=85% recall. Feldthaus et al., ICSE'13). The edge exists;
    // what keeps it honest is that it says it rests on the method name alone, where a real
    // `implements` clause says so instead.
    let (_tmp, index) = workspace(&ts_shapes());
    let graph = CallGraph::build(&index);

    assert_dispatches(
        &index,
        &graph,
        only(&index, "report"),
        method(&index, "Ledger", "area"),
        HierarchyBasis::MethodName,
    );
    assert_never_exact(&graph, method(&index, "Ledger", "area"));
}

#[test]
fn typescript_an_abstract_base_reaches_its_subclasses() {
    let files = [
        (
            "base.ts",
            "export abstract class Renderer {\n  abstract render(): string;\n}\n",
        ),
        (
            "html.ts",
            "import { Renderer } from './base';\nexport class HtmlRenderer extends Renderer {\n  render(): string { return 'h'; }\n}\n",
        ),
        (
            "text.ts",
            "import { Renderer } from './base';\nexport class TextRenderer extends Renderer {\n  render(): string { return 't'; }\n}\n",
        ),
        (
            "main.ts",
            "import { Renderer } from './base';\nexport function draw(r: Renderer): string {\n  return r.render();\n}\n",
        ),
    ];
    let (_tmp, index) = workspace(&files);
    let graph = CallGraph::build(&index);
    let draw = only(&index, "draw");

    for owner in ["HtmlRenderer", "TextRenderer"] {
        assert_dispatches(
            &index,
            &graph,
            draw,
            method(&index, owner, "render"),
            HierarchyBasis::DeclaredSupertype,
        );
    }
}

#[test]
fn typescript_an_implementation_reached_only_by_dispatch_is_not_reported_unused() {
    let (_tmp, index) = workspace(&ts_shapes());
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[only(&index, "main")]));
    for owner in ["Circle", "Square"] {
        assert!(
            !unused.contains(&method(&index, owner, "area")),
            "{owner}.area implements Shape: {unused:?}"
        );
    }
}

// ----------------------------------------------------------------------- Python

fn python_shapes() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "shape.py",
            "class Shape:\n    def area(self):\n        return 0\n",
        ),
        (
            "circle.py",
            "from shape import Shape\n\n\nclass Circle(Shape):\n    def area(self):\n        return 1\n",
        ),
        (
            "square.py",
            "from shape import Shape\n\n\nclass Square(Shape):\n    def area(self):\n        return 2\n",
        ),
        (
            "ledger.py",
            "class Ledger:\n    def area(self):\n        return 3\n",
        ),
        (
            "report.py",
            "from circle import Circle\n\n\ndef report(shape):\n    return shape.area()\n\n\ndef main():\n    report(Circle())\n",
        ),
    ]
}

#[test]
fn python_a_base_class_call_reaches_every_subclass() {
    let (_tmp, index) = workspace(&python_shapes());
    let graph = CallGraph::build(&index);
    let report = only(&index, "report");

    for owner in ["Shape", "Circle", "Square"] {
        assert_dispatches(
            &index,
            &graph,
            report,
            method(&index, owner, "area"),
            HierarchyBasis::DeclaredSupertype,
        );
    }
}

#[test]
fn python_a_class_outside_every_hierarchy_gets_no_edge() {
    // Python gets no name-only tier: bucketing by method name over-links Python badly
    // (PyCG, ICSE'21), so `Ledger.area`, a class with no base and no subclass, is
    // left alone even though the name matches.
    let (_tmp, index) = workspace(&python_shapes());
    let graph = CallGraph::build(&index);
    assert!(
        graph.callers(method(&index, "Ledger", "area")).is_empty(),
        "no declared relationship, no edge"
    );
}

#[test]
fn python_a_subclass_reached_only_by_dispatch_is_not_reported_unused() {
    let (_tmp, index) = workspace(&python_shapes());
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[only(&index, "main")]));
    for owner in ["Circle", "Square"] {
        assert!(
            !unused.contains(&method(&index, owner, "area")),
            "{owner}.area overrides Shape.area: {unused:?}"
        );
    }
    assert!(
        unused.contains(&method(&index, "Ledger", "area")),
        "nothing reaches Ledger.area: {unused:?}"
    );
}

// ------------------------------------------------------------------- honesty

#[test]
fn dispatch_edges_are_counted_apart_from_resolved_ones() {
    let (_tmp, index) = workspace(&rust_shapes());
    let graph = CallGraph::build(&index);

    let origins = graph.origin_breakdown();
    assert_eq!(
        origins.get("implemented-trait").copied().unwrap_or(0),
        3,
        "Circle, Square and the trait's own declaration: {origins:?}"
    );
    assert!(
        origins.get("resolved").copied().unwrap_or(0) >= 1,
        "main -> report is a resolved call: {origins:?}"
    );
    assert_eq!(graph.hierarchy_edge_count(), 3);

    let confidence = graph.confidence_breakdown();
    assert_eq!(
        confidence.get("field-based").copied().unwrap_or(0),
        3,
        "every dispatch candidate is field-based: {confidence:?}"
    );
}

#[test]
fn a_workspace_with_no_abstraction_gains_no_edges() {
    let (_tmp, index) = workspace(&[("a.rs", "fn leaf() {}\nfn main() { leaf(); }\n")]);
    let graph = CallGraph::build(&index);
    assert_eq!(graph.hierarchy_edge_count(), 0);
    assert_eq!(graph.confidence_breakdown().get("exact"), Some(&1));
}

#[test]
fn dispatch_edges_are_dashed_and_named_in_dot() {
    let (tmp, index) = workspace(&rust_shapes());
    let dot = CallGraph::build(&index).to_dot(&index, tmp.path());
    assert!(
        dot.contains("style=dashed, label=\"implemented-trait\""),
        "a picture must not overstate certainty: {dot}"
    );
}

#[test]
fn a_call_through_a_struct_field_stays_inherent() {
    // What hierarchy analysis cannot reach, and what B5 keeps. The handler is stored
    // in a field and called through it; no type declares `on_event` as a method, so
    // there is no method set to look it up in and nothing to over-approximate from.
    let source = "\
struct Bus { handler: fn() }
fn on_event() {}
fn main() {
    let bus = Bus { handler: on_event };
    (bus.handler)();
}
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    let graph = CallGraph::build(&index);
    assert_eq!(
        graph.hierarchy_edge_count(),
        0,
        "a function value in a field is not a method of any type"
    );
}

// -------------------------------------- implementations of the abstraction itself
//
// "What are the Sinks?" is the question people ask of an interface. It used to answer nothing:
// `implementations_of` required a method. So pointing at the type it belongs to returned an
// empty list instead of the three types that implement it. The relationships were already
// known, only the direction of the question was new.

/// Names of the implementations reported for the symbol called `name`.
fn implementations(index: &Index, name: &str) -> Vec<String> {
    let mut found: Vec<String> =
        fun_refactor::navigate::implementations_of(index, only(index, name))
            .into_iter()
            .filter_map(|id| index.symbol(id))
            .map(|s| s.qualified_name())
            .collect();
    found.sort();
    found
}

#[test]
fn a_go_interface_names_the_types_whose_method_set_covers_it() {
    let source = "\
package p

type Sink interface {
	Store(r int) error
	Flush() error
}

type Memory struct{}

func (m *Memory) Store(r int) error { return nil }
func (m *Memory) Flush() error      { return nil }

type Stdout struct{}

func (s *Stdout) Store(r int) error { return nil }
func (s *Stdout) Flush() error      { return nil }

// Covers half the method set, so it is not a Sink.
type Partial struct{}

func (p *Partial) Store(r int) error { return nil }
";
    let (_tmp, index) = workspace(&[("sink.go", source)]);
    assert_eq!(
        implementations(&index, "Sink"),
        vec!["Memory".to_string(), "Stdout".to_string()],
        "Go implements an interface by covering its method set, and only by that"
    );
}

#[test]
fn a_rust_trait_names_the_types_that_impl_it() {
    let source = "\
trait Sink {
    fn store(&self, r: i32);
}

struct Memory;
impl Sink for Memory {
    fn store(&self, r: i32) {}
}

struct Stdout;
impl Sink for Stdout {
    fn store(&self, r: i32) {}
}

struct Unrelated;
impl Unrelated {
    fn store(&self, r: i32) {}
}
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    assert_eq!(
        implementations(&index, "Sink"),
        vec!["Memory".to_string(), "Stdout".to_string()],
        "an inherent method of the same name is not an implementation of the trait"
    );
}

#[test]
fn a_typescript_interface_names_its_implementors() {
    let source = "\
export interface Sink {
  store(r: number): void;
}

export class Memory implements Sink {
  store(r: number): void {}
}

export class Stdout implements Sink {
  store(r: number): void {}
}
";
    let (_tmp, index) = workspace(&[("a.ts", source)]);
    assert_eq!(
        implementations(&index, "Sink"),
        vec!["Memory".to_string(), "Stdout".to_string()]
    );
}

#[test]
fn a_python_base_class_names_its_subclasses_transitively() {
    let source = "\
class Sink:
    def store(self, r):
        pass

class Memory(Sink):
    def store(self, r):
        pass

class Buffered(Memory):
    def store(self, r):
        pass

class Unrelated:
    def store(self, r):
        pass
";
    let (_tmp, index) = workspace(&[("a.py", source)]);
    assert_eq!(
        implementations(&index, "Sink"),
        vec!["Buffered".to_string(), "Memory".to_string()],
        "a subclass of a subclass is still an implementation"
    );
}

#[test]
fn an_empty_go_interface_names_nothing() {
    // Every type satisfies `interface{}`. Answering "all of them" is true and useless,
    // and would bury the cases where the answer means something.
    let source = "\
package p

type Any interface{}

type Memory struct{}

func (m *Memory) Store(r int) error { return nil }
";
    let (_tmp, index) = workspace(&[("a.go", source)]);
    assert!(
        implementations(&index, "Any").is_empty(),
        "an interface with no methods constrains nothing"
    );
}

#[test]
fn a_concrete_type_has_no_implementations() {
    let source = "\
struct Memory;
impl Memory {
    fn store(&self) {}
}
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    assert!(implementations(&index, "Memory").is_empty());
}

// ------------------------------------ a call that resolved *to the abstraction*
//
// The dispatch layer looks at call sites that resolved to nothing. There is a second shape it
// never saw. `sink.Store(r)` where `sink` is declared as the interface type resolves perfectly
// well, to the interface's own declaration, which has no body. The graph stopped there, so
// every implementation was unreached and every one of them was reported as dead code.

#[test]
fn go_a_call_typed_as_the_interface_reaches_the_implementations() {
    let source = "\
package p

type Sink interface {
	Store(r int) error
}

type Memory struct{}

func (m *Memory) Store(r int) error { return nil }

type Stdout struct{}

func (s *Stdout) Store(r int) error { return nil }

func Ingest(sink Sink) error {
	return sink.Store(1)
}

func main() {
	_ = Ingest(&Memory{})
}
";
    let (_tmp, index) = workspace(&[("a.go", source)]);
    let graph = CallGraph::build(&index);
    let ingest = only(&index, "Ingest");

    for owner in ["Memory", "Stdout"] {
        let target = method(&index, owner, "Store");
        let found = edge(&graph, ingest, target);
        assert!(
            found.is_some(),
            "Ingest should reach {owner}::Store: the call resolved to the interface's \
             declaration, and every implementation of it is a candidate"
        );
        let (confidence, origin) = found.unwrap();
        assert_ne!(
            confidence,
            Confidence::Exact,
            "which implementation runs is a runtime fact"
        );
        assert_eq!(
            origin,
            EdgeOrigin::Hierarchy(HierarchyBasis::InterfaceMethodSet)
        );
    }
}

#[test]
fn go_an_implementation_of_a_typed_interface_call_is_not_reported_unused() {
    let source = "\
package p

type Sink interface {
	Store(r int) error
}

type Memory struct{}

func (m *Memory) Store(r int) error { return nil }

func Ingest(sink Sink) error {
	return sink.Store(1)
}

func main() {
	_ = Ingest(&Memory{})
}
";
    let (_tmp, index) = workspace(&[("a.go", source)]);
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[only(&index, "main")]));
    let names: Vec<String> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.qualified_name())
        .collect();
    assert!(
        !names.contains(&"Memory::Store".to_string()),
        "the only implementation of the interface `Ingest` calls through is not dead; \
         got {names:?}"
    );
}

#[test]
fn rust_a_call_through_a_trait_bound_reaches_the_impls() {
    let source = "\
trait Sink {
    fn store(&self, r: i32);
}

struct Memory;
impl Sink for Memory {
    fn store(&self, r: i32) {}
}

struct Stdout;
impl Sink for Stdout {
    fn store(&self, r: i32) {}
}

fn ingest(sink: &dyn Sink) {
    sink.store(1);
}

fn main() {
    ingest(&Memory);
}
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    let graph = CallGraph::build(&index);
    let ingest = only(&index, "ingest");
    for owner in ["Memory", "Stdout"] {
        assert!(
            edge(&graph, ingest, method(&index, owner, "store")).is_some(),
            "ingest should reach {owner}::store"
        );
    }
}

#[test]
fn a_resolved_call_to_a_concrete_method_gains_no_extra_edges() {
    // The fan-out must not fire where the callee is already the implementation: a
    // graph that doubles every ordinary method call is worse than no graph.
    let source = "\
struct Memory;
impl Memory {
    fn store(&self, r: i32) {}
}

fn main() {
    let m = Memory;
    m.store(1);
}
";
    let (_tmp, index) = workspace(&[("a.rs", source)]);
    let graph = CallGraph::build(&index);
    assert_eq!(
        graph.hierarchy_edge_count(),
        0,
        "an inherent method call is not dispatch"
    );
}
