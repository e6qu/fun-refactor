//! The open entries in BUGS.md, held to what they say.

use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn index_of(root: &Path) -> Index {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    Index::build_from_scan(&scanned).expect("index")
}

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

#[test]
fn dispatch_is_followed_as_far_as_the_source_shows_it() {
    // B5.
    let (_tmp, root) = workspace(&[
        (
            "a.rs",
            "pub trait Shape {\n    fn area(&self) -> f64;\n}\npub struct Circle;\n\
             impl Shape for Circle {\n    fn area(&self) -> f64 {\n        1.0\n    }\n}\n\
             pub fn total(s: &dyn Shape) -> f64 {\n    s.area()\n}\n",
        ),
        (
            "b.rs",
            "pub struct Held {\n    pub run: fn() -> f64,\n}\n\
             pub fn go(h: &Held) -> f64 {\n    (h.run)()\n}\n\
             pub fn candidate() -> f64 {\n    2.0\n}\n\
             pub fn make() -> Held {\n    Held { run: candidate }\n}\n",
        ),
        (
            "c.rs",
            "pub struct Table {\n    pub perform: fn() -> f64,\n}\n\
             pub fn dispatch(t: &Table) -> f64 {\n    (t.perform)()\n}\n\
             pub fn unnamed() -> f64 {\n    3.0\n}\n",
        ),
    ]);
    let index = index_of(&root);
    let entrypoints =
        fun_refactor::analysis::entrypoints::Entrypoints::detect(&index).expect("entry points");
    let dead: Vec<&str> = fun_refactor::refactor::delete::find_unused(&index, &entrypoints)
        .into_iter()
        .filter_map(|id| index.symbol(id))
        .map(|s| s.name.as_str())
        .collect();

    assert!(
        !dead.contains(&"area"),
        "a call through the trait reaches the implementation, so it is not dead: {dead:?}"
    );
    assert!(
        !dead.contains(&"candidate"),
        "the source puts `candidate` behind `run`, and `go` calls `run`: {dead:?}"
    );
    assert!(
        dead.contains(&"unnamed"),
        "B5's remaining half is a function the source never names, and the report still \
         holds it. If it stops doing so, update the entry: {dead:?}"
    );

    // And the edge itself, which is what `fr callees` prints.
    let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
    let go = symbol(&index, "go");
    let candidate = symbol(&index, "candidate");
    let reached: Vec<SymbolId> = graph.callees(go).into_iter().map(|(id, _)| id).collect();
    assert!(
        reached.contains(&candidate),
        "`go` reaches `candidate` through the field: {reached:?}"
    );
    assert_eq!(
        graph.origin_breakdown().get("function-value").copied(),
        Some(1),
        "and the edge says where it came from"
    );
}

#[test]
fn a_values_answer_names_the_channel_it_was_never_told_about() {
    let (_tmp, root) = workspace(&[
        ("Chart.yaml", "name: chart\nversion: 0.1.0\n"),
        ("values.yaml", "replicas: 1\n"),
        (
            "templates/deploy.yaml",
            "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: {{ .Values.replicas }}\n",
        ),
    ]);
    let index = index_of(&root);
    let key = symbol(&index, "replicas");
    use fun_refactor::analysis::provenance::{self, SetFlags, ValuesInputs};

    // The report says this in its stops: a channel outside the workspace that could pre-empt
    // every source listed is a stop, and so is a competition the supplied inputs settle.
    let said = |report: &provenance::Provenance| -> String {
        report
            .stops
            .iter()
            .map(|(_, reason)| reason.to_string())
            .collect::<Vec<_>>()
            .join(" | ")
    };

    let nothing_supplied = ValuesInputs::parse(&[], SetFlags::default()).expect("no inputs");
    let without =
        provenance::provenance_with_inputs(&index, key, 5, &nothing_supplied).expect("a report");
    assert!(
        said(&without).contains("overridden externally"),
        "with no inputs the answer is undecided and says so: {}",
        said(&without)
    );

    let some_supplied =
        ValuesInputs::parse(&[], SetFlags::of(&["replicas=3".to_string()])).expect("one --set");
    let with =
        provenance::provenance_with_inputs(&index, key, 5, &some_supplied).expect("a report");
    assert!(
        said(&with).contains("given the inputs the caller gave"),
        "with some inputs the answer settles on them, and names what is missing: {}",
        said(&with)
    );
}

#[test]
fn a_call_through_a_name_reaches_every_function_put_behind_that_name() {
    // The edge carries the name as its key, and two types may hold a field of that name.
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "pub struct A {\n    pub run: fn() -> f64,\n}\npub struct B {\n    pub run: fn() -> f64,\n}\n         pub fn one() -> f64 {\n    1.0\n}\npub fn two() -> f64 {\n    2.0\n}\n         pub fn build() -> (A, B) {\n    (A { run: one }, B { run: two })\n}\n         pub fn call(a: &A) -> f64 {\n    (a.run)()\n}\n         pub fn blind<T>(h: &T, pick: fn(&T) -> fn() -> f64) -> f64 {\n    (pick(h))()\n}\n",
    )]);
    let index = index_of(&root);
    let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
    let reached: Vec<SymbolId> = graph
        .callees(symbol(&index, "call"))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(
        reached.contains(&symbol(&index, "one")),
        "the walk reaches `A`'s own binding: {reached:?}"
    );
    assert!(
        !reached.contains(&symbol(&index, "two")),
        "the `run` a `B` literal named is not `a`'s: {reached:?}"
    );
    for (_, edge) in graph.callees(symbol(&index, "call")) {
        assert_eq!(edge.origin.as_str(), "function-value");
        assert!(edge.origin.is_dispatch(), "not a resolved call");
    }
}

#[test]
fn an_untyped_receiver_still_reaches_every_function_behind_the_name() {
    // The same two records, called through a field read the tool cannot type.
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "pub struct A {\n    pub run: fn() -> f64,\n}\npub struct B {\n    pub run: fn() -> f64,\n}\n         pub fn one() -> f64 {\n    1.0\n}\npub fn two() -> f64 {\n    2.0\n}\n         pub fn build() -> (A, B) {\n    (A { run: one }, B { run: two })\n}\n         pub fn call(pair: &(A, B)) -> f64 {\n    (pair.0.run)()\n}\n",
    )]);
    let index = index_of(&root);
    let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
    let reached: Vec<SymbolId> = graph
        .callees(symbol(&index, "call"))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(reached.contains(&symbol(&index, "one")), "{reached:?}");
    assert!(
        reached.contains(&symbol(&index, "two")),
        "nothing types `pair.0`, so the name reaches both: {reached:?}"
    );
}

#[test]
fn callable_aliases_follow_the_binding_that_holds_them() {
    let cases = [
        (
            "a.rs",
            "fn target() {}\nfn caller() { let first = target; let second = first; second(); }\n",
        ),
        (
            "a.go",
            "package p\nfunc target() {}\nfunc caller() { first := target; second := first; second() }\n",
        ),
        (
            "a.ts",
            "function target() {}\nfunction caller() { const first = target; const second = first; second(); }\n",
        ),
        (
            "a.py",
            "def target():\n    pass\n\ndef caller():\n    first = target\n    second = first\n    second()\n",
        ),
    ];
    for (path, source) in cases {
        let (_tmp, root) = workspace(&[(path, source)]);
        let index = index_of(&root);
        let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
        let reached: Vec<SymbolId> = graph
            .callees(symbol(&index, "caller"))
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert!(
            reached.contains(&symbol(&index, "target")),
            "{path}: {reached:?}"
        );
    }
}

#[test]
fn a_callable_reached_through_aliases_is_not_unused() {
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "fn target() {}\nfn main() { let first = target; let second = first; second(); }\n",
    )]);
    let index = index_of(&root);
    let entrypoints =
        fun_refactor::analysis::entrypoints::Entrypoints::exactly(&[symbol(&index, "main")]);
    let unused = fun_refactor::refactor::delete::find_unused(&index, &entrypoints);
    assert!(
        !unused.contains(&symbol(&index, "target")),
        "the alias call reaches target: {unused:?}."
    );
}

#[test]
fn a_reassigned_callable_keeps_each_written_target_live() {
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "fn one() {}\nfn two() {}\nfn main() { let mut run = one; run = two; run(); }\n",
    )]);
    let index = index_of(&root);
    let entrypoints =
        fun_refactor::analysis::entrypoints::Entrypoints::exactly(&[symbol(&index, "main")]);
    let unused = fun_refactor::refactor::delete::find_unused(&index, &entrypoints);
    for name in ["one", "two"] {
        assert!(
            !unused.contains(&symbol(&index, name)),
            "the reassignment reaches {name}: {unused:?}."
        );
    }
}

#[test]
fn same_named_callable_bindings_do_not_escape_their_lexical_scope() {
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "fn one() {}\nfn two() {}\nfn left() { let run = one; run(); }\nfn right() { let run = two; run(); }\n",
    )]);
    let index = index_of(&root);
    let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
    let callees = |name| {
        graph
            .callees(symbol(&index, name))
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
    };
    let one = symbol(&index, "one");
    let two = symbol(&index, "two");
    let left = callees("left");
    let right = callees("right");
    assert!(
        left.contains(&one) && !left.contains(&two),
        "left: {left:?}"
    );
    assert!(
        right.contains(&two) && !right.contains(&one),
        "right: {right:?}"
    );
}

#[test]
fn calling_a_factory_result_reaches_the_returned_callable() {
    let cases = [
        (
            "a.rs",
            "fn target() {}\nfn factory() -> fn() { let selected = target; return selected; }\nfn caller() { (factory())(); }\n",
        ),
        (
            "a.go",
            "package p\nfunc target() {}\nfunc factory() func() { return target }\nfunc caller() { factory()() }\n",
        ),
        (
            "a.ts",
            "function target() {}\nfunction factory(): () => void { return target; }\nfunction caller() { factory()(); }\n",
        ),
        (
            "a.py",
            "def target():\n    pass\n\ndef factory():\n    return target\n\ndef caller():\n    factory()()\n",
        ),
    ];
    for (path, source) in cases {
        let (_tmp, root) = workspace(&[(path, source)]);
        let index = index_of(&root);
        let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
        let caller = symbol(&index, "caller");
        let target = symbol(&index, "target");
        let reached = graph.callees(caller);
        assert!(
            reached.iter().any(|(id, _)| *id == target),
            "{path}: {reached:?}"
        );
        let edge = reached
            .into_iter()
            .find(|(id, _)| *id == target)
            .map(|(_, edge)| edge)
            .expect("the target edge");
        assert_eq!(edge.origin.as_str(), "function-value");
    }
}
